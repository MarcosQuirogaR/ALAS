// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimum control speed from the one-engine-inoperative yaw balance, the
//! take-off speed rule that consumes it, and the control-based fin checks.
//!
//! `VMCA` is the speed at which the rudder (at its stop) and at most 5 deg of
//! bank balance the yawing moment of the live-engine thrust and the failed
//! engine's windmilling drag, CS 25.149(b); the balance and its sources are
//! in [`alas_perf::performance::yaw_balance`]. The sideslip follows from the
//! lateral force balance, and the weight is the operating empty weight, the
//! lightest and so the least bank side force (CS 25.149(c)(4)). The thrust
//! is the live-engine take-off rating at the speed itself and the airport, so
//! `VMCA` is a fixed point of the deck thrust (a few iterations). `VMCG` is
//! the ground run of the same airframe after the failure, rudder and main
//! tyre side force against the thrust moment and no nose-wheel steering
//! (CS 25.149(e)), the deviation limited to 30 ft.
//!
//! # How the computed value enters the speed rule
//!
//! CS 25.107(b) puts `V2MIN` at no less than 1.10 `VMC` and CS 25.107(e) puts
//! `VR` at no less than 1.05 `VMC`. [`performance_with_vmc`] writes the
//! computed `VMC / VSR` into `PerformanceConfig::vmc_vstall_factor` of a
//! copy of the configuration, so every speed schedule built from it uses the
//! computed value. The 1.13 `VSR` figure of CS 25.149(c) is a ceiling, not an
//! estimate: for a registered aircraft, which is certified, the value is
//! capped at the declared ratio (`vmc_vstall_factor`, default 1.13) and
//! the excess is reported as a calibration gap of the fin model; for a
//! clean-sheet design the computed value is used as it is and the ceiling is a
//! hard residual.
//!
//! # Residuals
//! - `vmc_over_vsr`: `VMC / VSR` against the 1.13 ceiling (hard for clean
//!   sheet, diagnostic for a registered aircraft). Evaluated at sea level
//!   ISA, where the thrust is greatest, as CS 25.149 prescribes.
//! - `crosswind_rudder`: rudder deflection that holds the sideslip of the
//!   design crosswind at the approach speed against the rudder stop
//!   (Torenbeek Eq. 9-68). The design crosswind is the certification basis
//!   of CS 25.237(a)(3)(ii) (14 CFR 25.237(a)(3)(ii)): the greater of 20 kt
//!   and 0.2 `VSR0`, need not exceed 25 kt ([`design_crosswind_m_s`]). The
//!   30 kt of Sadraey Table 12.13 (p. 651) is a textbook preference, not the
//!   requirement.
//! - `directional_stability`: `Cn_beta` of fuselage plus fin at the approach
//!   Mach number, at least zero (CS 25.177; Sadraey p. 282). Wing and nacelle
//!   terms are not modelled, so the 0.1 to 0.4 per rad of complete aircraft is
//!   not the floor. Hard for clean sheet, diagnostic for a registered aircraft.
//! - `vmcg_estimate_over_vsr` and `fin_area_ratio_for_vmc_ceiling`:
//!   diagnostics (the latter is the fin area, at constant aspect ratio and
//!   arm, that would bring `VMC` to the ceiling over the actual area).

use std::borrow::Cow;

use alas_atmo::us1976_compute_values;
use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignMode};
use alas_geom::aircraft::airplane::Airplane;
use alas_perf::performance::yaw_balance::{
    ControlLimit, GroundRunThrust, MinimumControlSpeed, MinimumControlSpeedFailure,
    YawControlGeometry, YawTrimMode, DIRECTIONAL_STABILITY_SCALE_PER_RAD,
    MINIMUM_DIRECTIONAL_STABILITY_PER_RAD, VMC_CEILING_OVER_VSR,
};

use super::residuals_performance::oei_deck::{
    EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe,
};
use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// Lower bound of the CS 25.237(a)(3)(ii) design crosswind, m/s (20 kt).
pub const CROSSWIND_MINIMUM_M_S: f64 = 20.0 * 0.514_444_444;
/// The CS 25.237(a)(3)(ii) design crosswind need not exceed 25 kt, m/s.
pub const CROSSWIND_CAP_M_S: f64 = 25.0 * 0.514_444_444;

/// CS 25.237(a)(3)(ii) design 90-degree crosswind for a landplane, m/s: the
/// greater of 20 kt and 0.2 `VSR0` (the landing-configuration reference stall
/// speed), need not exceed 25 kt.
#[must_use]
pub fn design_crosswind_m_s(vsr0_m_s: f64) -> f64 {
    (0.2 * vsr0_m_s).clamp(CROSSWIND_MINIMUM_M_S, CROSSWIND_CAP_M_S)
}
/// Iterations of the thrust at the minimum control speed (the secant steps of
/// the root search, then the damped fixed point when no root is bracketed).
const THRUST_ITERATIONS: usize = 40;
/// Convergence of the speed in the thrust iteration, m/s.
const SPEED_TOLERANCE_M_S: f64 = 0.01;

/// One evaluated minimum control speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmcAssessment {
    /// Airborne minimum control speed (true airspeed at the condition), m/s.
    pub vmca_m_s: f64,
    /// Reference stall speed `VSR` at the same weight and condition, m/s.
    pub vsr_m_s: f64,
    /// `VMCA / VSR`.
    pub vmc_over_vsr: f64,
    /// Ground minimum control speed, m/s: the engine-failure speed of a
    /// 30 ft deviation in the CS 25.149(e) take-off run (no nose-wheel
    /// steering), `alas_perf::performance::yaw_balance::GroundRunGeometry`.
    pub vmcg_m_s: Option<f64>,
    /// Rudder deflection at `VMCA`, deg.
    pub rudder_deg: f64,
    /// Sideslip at `VMCA`, deg (positive with the wind from the live side).
    pub sideslip_deg: f64,
    /// Bank at `VMCA`, deg.
    pub bank_deg: f64,
    /// What limits control at `VMCA`.
    pub limit: ControlLimit,
    /// Live-engine thrust plus windmilling drag at `VMCA`, N.
    pub asymmetric_force_n: f64,
    /// Fin area over the actual one that brings `VMCA` to the ceiling
    /// (below one: the fin has margin), at constant aspect ratio and arm.
    pub fin_area_ratio_for_ceiling: Option<f64>,
    /// Whether the fixed point of the take-off thrust at `VMCA` closed within
    /// its speed tolerance. `false`: the iteration cap ended it and the
    /// values above are its last iterate.
    pub thrust_iteration_converged: bool,
}

/// Why no minimum control speed was evaluated.
#[derive(Debug, Clone, PartialEq)]
pub enum VmcFailure {
    /// The airframe cannot hold the asymmetry at any speed of the search.
    NotControllable,
    /// The airframe holds the asymmetry with control to spare down to the
    /// bottom of the search range: the engine-out yawing moment sets no
    /// minimum control speed, and the declared ratio stays in the speed rule.
    NotLimiting {
        /// Bottom of the search range, true airspeed, m/s.
        below_m_s: f64,
    },
    /// A model input was missing or invalid.
    Unavailable(String),
}

impl From<MinimumControlSpeedFailure> for VmcFailure {
    fn from(failure: MinimumControlSpeedFailure) -> Self {
        match failure {
            MinimumControlSpeedFailure::AboveSearchRange { .. } => Self::NotControllable,
            MinimumControlSpeedFailure::BelowSearchRange { tas_m_s } => {
                Self::NotLimiting { below_m_s: tas_m_s }
            }
            MinimumControlSpeedFailure::TrimUnavailable { tas_m_s } => Self::Unavailable(format!(
                "the engine-out yaw trim could not be evaluated at {tas_m_s:.1} m/s"
            )),
            MinimumControlSpeedFailure::UnusableInputs => Self::Unavailable(
                "air density or speed of sound is not a positive finite number".to_owned(),
            ),
        }
    }
}

/// The crosswind rudder check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrosswindAssessment {
    /// Approach speed the sideslip is taken at, m/s.
    pub approach_speed_m_s: f64,
    /// Sideslip of the design crosswind, deg.
    pub sideslip_deg: f64,
    /// Rudder deflection that holds it, deg.
    pub rudder_deg: f64,
    /// The rudder limit, deg.
    pub rudder_limit_deg: f64,
    /// `Cn_beta` per rad of fuselage plus fin at approach (Torenbeek Eq. 9-62);
    /// `NaN` when unavailable.
    pub cn_beta_per_rad: f64,
}

/// Everything the optimizer residual and the feasibility finding need.
#[derive(Debug, Clone, PartialEq)]
pub struct VmcOutcome {
    /// At the departure aerodrome (feeds the speed rule).
    pub at_airport: Result<VmcAssessment, VmcFailure>,
    /// At sea level ISA (the certification condition of the ceiling).
    pub at_sea_level: Result<VmcAssessment, VmcFailure>,
    /// The crosswind check, when the fin data and the landing mass exist.
    pub crosswind: Option<CrosswindAssessment>,
    /// The `VMC / VSR` the speed rule uses, `None` when the declared value
    /// stays.
    pub rule_ratio: Option<f64>,
    /// Whether the computed value exceeded the declared ratio of a registered
    /// aircraft and was capped.
    pub capped_to_declared: bool,
    /// Whether the aircraft is a registered one (reference adaptation).
    pub registered: bool,
}

/// Inputs of [`evaluate_vmc`].
pub struct VmcInputs<'a> {
    /// The candidate or reported configuration.
    pub config: &'a AlasConfig,
    /// The built airplane.
    pub plane: &'a Airplane,
    /// Centre-of-gravity x station, m.
    pub cg_x_m: f64,
    /// Take-off mass, kg.
    pub takeoff_mass_kg: f64,
    /// Operating empty mass, kg: the lightest mass the aircraft flies at,
    /// the weight of the airborne `VMCA` balance (`NaN` or not positive:
    /// the take-off mass). The bank's weight component is the only term of
    /// the balance that grows with mass, so the lightest weight is the most
    /// unfavourable one of CS 25.149(c)(4) ("or any lesser weight necessary
    /// to show VMC").
    pub minimum_mass_kg: f64,
    /// Landing mass, kg (for the crosswind check).
    pub landing_mass_kg: f64,
    /// Reference wing area, m^2.
    pub s_ref_m2: f64,
    /// Installed static thrust per engine (turboprops), N.
    pub static_per_engine_n: Option<f64>,
    /// Departure aerodrome, when resolved.
    pub departure: Option<&'a Airport>,
    /// Clean zero-lift drag coefficient at `(mach, altitude_m, isa_c)`.
    pub clean_cd0: &'a dyn Fn(f64, f64, f64) -> f64,
}

fn unavailable(text: impl Into<String>) -> VmcFailure {
    VmcFailure::Unavailable(text.into())
}

/// Evaluate `VMCA` (and the `VMCG` estimate) at one condition.
fn assess_at(
    model: &EngineOutModel,
    control: YawControlGeometry,
    inputs: &VmcInputs<'_>,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<VmcAssessment, VmcFailure> {
    let gravity = inputs.config.requirements.gravity_m_s2;
    let weight_n = inputs.takeoff_mass_kg * gravity;
    // The bank's weight component at the most unfavourable (lightest) weight.
    let control_weight_n = if inputs.minimum_mass_kg.is_finite() && inputs.minimum_mass_kg > 0.0 {
        inputs.minimum_mass_kg.min(inputs.takeoff_mass_kg) * gravity
    } else {
        weight_n
    };
    let atmosphere = us1976_compute_values(altitude_m, isa_deviation_c);
    let (rho, a_m_s) = (atmosphere.density_kg_m3, atmosphere.speed_of_sound_m_s);
    let cl_max = inputs.config.performance.cl_max_to;
    let vsr = (2.0 * weight_n / (rho * inputs.s_ref_m2 * cl_max)).sqrt();
    if !(vsr.is_finite() && vsr > 0.0) {
        return Err(unavailable("reference stall speed is not positive"));
    }
    let force_at = |tas: f64| -> Result<f64, VmcFailure> {
        let point = model
            .point(
                EngineOutCondition {
                    altitude_m,
                    isa_deviation_c,
                    tas_m_s: tas,
                },
                EngineOutRating::TakeoffGoAround,
                (inputs.clean_cd0)(tas / a_m_s, altitude_m, isa_deviation_c),
            )
            .map_err(unavailable)?;
        let q = 0.5 * rho * tas * tas;
        Ok(point.thrust_per_engine_n + point.windmilling_cd * q * inputs.s_ref_m2)
    };
    // VMCA is the speed v at which the minimum control speed of the thrust at
    // v is v itself: the root of h(v) = VMC(F(v)) - v. The first two points
    // (1.15 VSR and the VMC it gives) bracket the root whenever VMC falls
    // or stays flat as the thrust falls with speed, the usual case; the
    // bracket is closed by regula falsi with the Illinois modification
    // (Dowell and Jarratt, BIT 11 (1971) 168-174), which keeps the bracket
    // and converges superlinearly. Without a bracket the damped fixed point
    // (half step) is kept. A damped step alone needs about
    // log2(gap / tolerance) steps: 12 left the LJ60 (VMC 0.64 VSR, a gap
    // of 0.5 VSR from the start) 0.014 m/s short of the tolerance.
    let evaluate = |speed: f64| -> Result<(MinimumControlSpeed, f64, f64), VmcFailure> {
        let force = force_at(speed)?;
        let vmc = control.minimum_control_speed_outcome(
            force,
            rho,
            a_m_s,
            Some(control_weight_n),
            YawTrimMode::LeastRudder,
        )?;
        Ok((vmc, force, speed))
    };
    let residual = |solved: &(MinimumControlSpeed, f64, f64)| solved.0.tas_m_s - solved.2;
    let mut solved = evaluate(1.15 * vsr)?;
    let mut converged = residual(&solved).abs() < SPEED_TOLERANCE_M_S;
    if !converged {
        let first = solved;
        let second = evaluate(first.0.tas_m_s)?;
        solved = second;
        converged = residual(&second).abs() < SPEED_TOLERANCE_M_S;
        let bracketed = residual(&first).signum() != residual(&second).signum();
        if !converged && bracketed {
            let (mut a, mut b) = (first, second);
            let (mut ha, mut hb) = (residual(&a), residual(&b));
            for _ in 0..THRUST_ITERATIONS {
                let speed = (a.2 * hb - b.2 * ha) / (hb - ha);
                let next = evaluate(speed)?;
                let h = residual(&next);
                solved = next;
                if h.abs() < SPEED_TOLERANCE_M_S {
                    converged = true;
                    break;
                }
                if h.signum() == hb.signum() {
                    ha *= 0.5;
                } else {
                    a = b;
                    ha = hb;
                }
                b = next;
                hb = h;
            }
        } else if !converged {
            let mut speed = 0.5 * (second.2 + second.0.tas_m_s);
            for _ in 0..THRUST_ITERATIONS {
                let next = evaluate(speed)?;
                solved = next;
                if residual(&next).abs() < SPEED_TOLERANCE_M_S {
                    converged = true;
                    break;
                }
                speed = 0.5 * (speed + next.0.tas_m_s);
            }
        }
    }
    let (vmc, force, thrust_speed) = solved;
    if !converged {
        tracing::warn!(
            vmca_m_s = vmc.tas_m_s,
            thrust_speed_m_s = thrust_speed,
            iterations = THRUST_ITERATIONS,
            "minimum control speed: the thrust fixed point did not converge; the last iterate is reported"
        );
    }
    let engines = inputs
        .config
        .geometry
        .engine
        .spanwise_positions_m
        .len()
        .max(2) as f64;
    let ground_thrust = |tas: f64| -> Option<GroundRunThrust> {
        let point = model
            .point(
                EngineOutCondition {
                    altitude_m,
                    isa_deviation_c,
                    tas_m_s: tas,
                },
                EngineOutRating::TakeoffGoAround,
                (inputs.clean_cd0)(tas / a_m_s, altitude_m, isa_deviation_c),
            )
            .ok()?;
        Some(GroundRunThrust {
            thrust_per_engine_n: point.thrust_per_engine_n,
            windmilling_drag_n: point.windmilling_cd * 0.5 * rho * tas * tas * inputs.s_ref_m2,
            total_thrust_n: point.thrust_per_engine_n * engines,
        })
    };
    let ground_cd0 = (inputs.clean_cd0)(0.2, altitude_m, isa_deviation_c);
    let vmcg =
        ground::ground_run_geometry(inputs, model.is_turboprop(), ground_cd0).and_then(|ground| {
            control.ground_minimum_control_speed(&ground, &ground_thrust, rho, a_m_s)
        });
    let target = VMC_CEILING_OVER_VSR * vsr;
    Ok(VmcAssessment {
        vmca_m_s: vmc.tas_m_s,
        vsr_m_s: vsr,
        vmc_over_vsr: vmc.tas_m_s / vsr,
        vmcg_m_s: vmcg,
        rudder_deg: vmc.trim.rudder_rad.to_degrees(),
        sideslip_deg: vmc.trim.sideslip_rad.to_degrees(),
        bank_deg: vmc.trim.bank_rad.to_degrees(),
        limit: vmc.limit,
        asymmetric_force_n: force,
        fin_area_ratio_for_ceiling: fin_area_ratio(
            control,
            force,
            rho,
            a_m_s,
            control_weight_n,
            target,
        ),
        thrust_iteration_converged: converged,
    })
}

/// Fin (and rudder) area ratio at which `VMCA` equals `target_m_s`, by
/// bisection on the area at constant aspect ratio, sweep and arm.
fn fin_area_ratio(
    control: YawControlGeometry,
    force_n: f64,
    rho: f64,
    a_m_s: f64,
    weight_n: f64,
    target_m_s: f64,
) -> Option<f64> {
    let vmc_at = |ratio: f64| {
        let mut scaled = control;
        scaled.balance.fin_area_m2 *= ratio;
        scaled.balance.rudder_area_m2 *= ratio;
        scaled
            .minimum_control_speed(
                force_n,
                rho,
                a_m_s,
                Some(weight_n),
                YawTrimMode::LeastRudder,
            )
            .map(|vmc| vmc.tas_m_s)
    };
    let (mut low, mut high) = (0.05_f64, 20.0_f64);
    // VMC falls as the fin grows; the target must lie between the ends. A
    // fin that cannot be controlled at all (`None`) counts as above target.
    if vmc_at(high).is_some_and(|v| v > target_m_s) {
        return None;
    }
    if vmc_at(low).is_some_and(|v| v <= target_m_s) {
        return Some(low);
    }
    for _ in 0..60 {
        let mid = (low * high).sqrt();
        if vmc_at(mid).is_none_or(|v| v > target_m_s) {
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(high)
}

/// The crosswind rudder check at the landing mass.
fn crosswind(control: YawControlGeometry, inputs: &VmcInputs<'_>) -> Option<CrosswindAssessment> {
    let perf = &inputs.config.performance;
    let sea_level = us1976_compute_values(0.0, 0.0);
    let weight_n = inputs.landing_mass_kg * inputs.config.requirements.gravity_m_s2;
    let vs_land =
        (2.0 * weight_n / (sea_level.density_kg_m3 * inputs.s_ref_m2 * perf.cl_max_land)).sqrt();
    let approach = perf.vapp_vstall_land_factor * vs_land;
    if !(approach.is_finite() && approach > 0.0) {
        return None;
    }
    let sideslip = (design_crosswind_m_s(vs_land) / approach).atan();
    let rudder = control.crosswind_rudder_rad(sideslip, approach / sea_level.speed_of_sound_m_s)?;
    Some(CrosswindAssessment {
        approach_speed_m_s: approach,
        sideslip_deg: sideslip.to_degrees(),
        rudder_deg: rudder.to_degrees(),
        rudder_limit_deg: control.rudder_max_deflection_rad.to_degrees(),
        cn_beta_per_rad: control
            .cn_beta_total_per_rad(approach / sea_level.speed_of_sound_m_s)
            .unwrap_or(f64::NAN),
    })
}

/// Evaluate the minimum control speed and the crosswind check of a built
/// airplane. Never fails: each result carries its own failure.
pub fn evaluate_vmc(inputs: &VmcInputs<'_>) -> VmcOutcome {
    let registered = inputs.config.optimizer.design_space.mode == DesignMode::ReferenceAdaptation;
    let setup = || -> Result<(EngineOutModel, YawControlGeometry), VmcFailure> {
        let model = EngineOutModel::new(
            inputs.config,
            Some(YawAirframe {
                plane: inputs.plane,
                cg_x_m: inputs.cg_x_m,
            }),
            inputs.s_ref_m2,
            // The turboprop thrust ratio is not used here; any positive
            // static thrust satisfies the binding.
            Some(inputs.static_per_engine_n.unwrap_or(1.0)),
        )
        .map_err(unavailable)?;
        let control = model
            .yaw_control()
            .ok_or_else(|| unavailable("no fin or fuselage data for the yaw balance"))?;
        Ok((model, control))
    };
    let (model, control) = match setup() {
        Ok(pair) => pair,
        Err(failure) => {
            return VmcOutcome {
                at_airport: Err(failure.clone()),
                at_sea_level: Err(failure),
                crosswind: None,
                rule_ratio: None,
                capped_to_declared: false,
                registered,
            }
        }
    };
    let (altitude_m, isa_c) = inputs.departure.map_or((0.0, 0.0), |airport| {
        (airport.elevation_m, airport.isa_deviation_c)
    });
    let at_airport = assess_at(&model, control, inputs, altitude_m, isa_c);
    let at_sea_level = if altitude_m == 0.0 && isa_c == 0.0 {
        at_airport.clone()
    } else {
        assess_at(&model, control, inputs, 0.0, 0.0)
    };
    let declared = inputs.config.performance.vmc_vstall_factor;
    let (rule_ratio, capped_to_declared) = match &at_airport {
        Ok(assessment) if registered && assessment.vmc_over_vsr > declared => {
            (Some(declared), true)
        }
        Ok(assessment) => (Some(assessment.vmc_over_vsr), false),
        Err(_) => (None, false),
    };
    VmcOutcome {
        at_airport,
        at_sea_level,
        crosswind: crosswind(control, inputs),
        rule_ratio,
        capped_to_declared,
        registered,
    }
}

/// The configuration the speed schedule is built from: a copy with the
/// computed `VMC / VSR` in `vmc_vstall_factor`, or the borrowed original when
/// nothing was computed.
pub fn performance_with_vmc<'a>(
    config: &'a AlasConfig,
    outcome: &VmcOutcome,
) -> Cow<'a, AlasConfig> {
    match outcome.rule_ratio {
        Some(ratio) if ratio.is_finite() && ratio > 0.0 => {
            let mut resolved = config.clone();
            resolved.performance.vmc_vstall_factor = ratio;
            Cow::Owned(resolved)
        }
        _ => Cow::Borrowed(config),
    }
}

/// The minimum-control-speed residuals of one candidate.
pub(crate) fn vmc_residuals(outcome: &VmcOutcome, role: ResidualRole) -> Vec<ConstraintResidual> {
    let role = if outcome.registered {
        ResidualRole::Diagnostic
    } else {
        role
    };
    let mut residuals = Vec::new();
    let ceiling = outcome
        .at_sea_level
        .as_ref()
        .ok()
        .or(outcome.at_airport.as_ref().ok());
    if let Some(assessment) = ceiling {
        residuals.push(ConstraintResidual::scaled(
            "vmc_over_vsr",
            Performance,
            assessment.vmc_over_vsr,
            VMC_CEILING_OVER_VSR,
            "VMC/VSR",
            assessment.vmc_over_vsr - VMC_CEILING_OVER_VSR,
            role,
        ));
        if let Some(vmcg) = assessment.vmcg_m_s {
            let ratio = vmcg / assessment.vsr_m_s;
            residuals.push(ConstraintResidual::scaled(
                "vmcg_estimate_over_vsr",
                Performance,
                ratio,
                VMC_CEILING_OVER_VSR,
                "VMCG/VSR",
                ratio - VMC_CEILING_OVER_VSR,
                ResidualRole::Diagnostic,
            ));
        }
        if let Some(ratio) = assessment.fin_area_ratio_for_ceiling {
            residuals.push(ConstraintResidual::scaled(
                "fin_area_ratio_for_vmc_ceiling",
                Performance,
                ratio,
                1.0,
                "ratio",
                ratio - 1.0,
                ResidualRole::Diagnostic,
            ));
        }
    } else {
        // Not controllable at any speed is a violation; a missing input is
        // reported without ranking. An asymmetry the airframe holds with
        // control to spare at every searched speed sets no minimum control
        // speed: there is no ratio to hold against the ceiling and no row.
        let row = match &outcome.at_sea_level {
            Err(VmcFailure::NotControllable) => Some((
                f64::INFINITY,
                role,
                "the fin cannot balance the engine-out yawing moment at any speed".to_owned(),
            )),
            Err(VmcFailure::NotLimiting { .. }) => None,
            Err(VmcFailure::Unavailable(reason)) => {
                Some((f64::NAN, ResidualRole::Diagnostic, reason.clone()))
            }
            Ok(_) => Some((f64::NAN, ResidualRole::Diagnostic, String::new())),
        };
        if let Some((value, row_role, detail)) = row {
            residuals.push(
                ConstraintResidual::scaled(
                    "vmc_over_vsr",
                    Performance,
                    value,
                    VMC_CEILING_OVER_VSR,
                    "VMC/VSR",
                    value,
                    row_role,
                )
                .with_detail(detail),
            );
        }
    }
    if let Some(check) = outcome.crosswind {
        residuals.push(ConstraintResidual::scaled(
            "crosswind_rudder",
            Performance,
            check.rudder_deg,
            check.rudder_limit_deg,
            "deg",
            check.rudder_deg - check.rudder_limit_deg,
            role,
        ));
        if check.cn_beta_per_rad.is_finite() {
            let shortfall = MINIMUM_DIRECTIONAL_STABILITY_PER_RAD - check.cn_beta_per_rad;
            residuals.push(ConstraintResidual::direct(
                "directional_stability",
                Performance,
                check.cn_beta_per_rad,
                MINIMUM_DIRECTIONAL_STABILITY_PER_RAD,
                "1/rad",
                shortfall,
                (shortfall / DIRECTIONAL_STABILITY_SCALE_PER_RAD).max(0.0),
                role,
            ));
        }
    }
    residuals
}

/// Evaluate the minimum control speed of a sized candidate.
pub(crate) fn evaluate_candidate(outcome: &SizingOutcome, config: &AlasConfig) -> VmcOutcome {
    let s_ref_m2 = outcome.plane.s_ref.max(1e-9);
    let drag = &outcome.sized.fuel_artifacts.drag;
    evaluate_vmc(&VmcInputs {
        config,
        plane: &outcome.plane,
        cg_x_m: outcome.cg_x,
        takeoff_mass_kg: outcome.sized.takeoff_mass_kg,
        minimum_mass_kg: operating_empty_mass_kg(&outcome.masses),
        landing_mass_kg: outcome.sized.design_landing_mass_kg,
        s_ref_m2,
        static_per_engine_n: Some(outcome.static_thrust_kn * 1_000.0)
            .filter(|thrust| thrust.is_finite() && *thrust > 0.0),
        departure: outcome.departure,
        clean_cd0: &|mach, altitude_m, isa_c| drag.cd_at_atmosphere(0.0, mach, altitude_m, isa_c),
    })
}

/// Operating empty mass of a mass breakdown, kg: the groups of
/// `alas_mass::breakdown::OEW_KEYS` (everything but payload and fuel).
fn operating_empty_mass_kg(masses: &alas_mass::breakdown::MassBreakdown) -> f64 {
    masses.wing
        + masses.h_stab
        + masses.v_stab
        + masses.fuselage
        + masses.gear
        + masses.propulsion
        + masses.systems
        + masses.furnishings
}

mod ground;

#[cfg(test)]
mod tests;
