// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimum control speed from the one-engine-inoperative yaw balance, the
//! take-off speed rule that consumes it, and the control-based fin checks.
//!
//! `VMCA` is the speed at which the rudder (at its stop) and at most 5 deg of
//! bank balance the yawing moment of the live-engine thrust and the failed
//! engine's windmilling drag, CS 25.149(b); the balance and its sources are
//! in [`alas_perf::performance::yaw_balance`]. The thrust is the live-engine
//! take-off rating at the speed itself and the airport, so `VMCA` is a
//! fixed point of the deck thrust (a few iterations). `VMCG` is the same
//! balance with the wings level and no sideslip, i.e. the rudder alone: an
//! ESTIMATE that ignores tyre side force and nose-wheel steering (CS 25.149(e)
//! forbids the latter, the former lowers the true value), so it bounds
//! the real `VMCG` from above in that respect.
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
//!
//! - `vmc_over_vsr`: `VMC / VSR` against the 1.13 ceiling (hard for clean
//!   sheet, diagnostic for a registered aircraft). Evaluated at sea level
//!   ISA, where the thrust is greatest, as CS 25.149 prescribes.
//! - `crosswind_rudder`: rudder deflection that holds the sideslip of a
//!   30 kt crosswind at the approach speed against the rudder stop
//!   (Torenbeek Eq. 9-68; CS 25.237 demonstrates at least 0.2 `VSO`, to 25 kt,
//!   so 30 kt is a design margin).
//! - `vmcg_estimate_over_vsr` and `fin_area_ratio_for_vmc_ceiling`:
//!   diagnostics (the latter is the fin area, at constant aspect ratio and
//!   arm, that would bring `VMC` to the ceiling over the actual area).

use std::borrow::Cow;

use alas_atmo::us1976_compute_values;
use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignMode};
use alas_geom::aircraft::airplane::Airplane;
use alas_perf::performance::yaw_balance::{
    ControlLimit, YawControlGeometry, YawTrimMode, VMC_CEILING_OVER_VSR,
};

use super::residuals_performance::oei_deck::{
    EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe,
};
use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// Design crosswind at the approach speed, m/s (30 kt; Torenbeek Sec. 9.6.3
/// puts the design sideslip at 12 to 15 deg for a transport in 30 kt).
pub const CROSSWIND_DESIGN_M_S: f64 = 30.0 * 0.514_444_444;
/// Fixed-point iterations of the thrust at the minimum control speed.
const THRUST_ITERATIONS: usize = 12;
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
    /// Ground minimum control speed estimate (rudder alone), m/s.
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
}

/// Why the minimum control speed could not be evaluated.
#[derive(Debug, Clone, PartialEq)]
pub enum VmcFailure {
    /// The airframe cannot hold the asymmetry at any speed of the search.
    NotControllable,
    /// A model input was missing or invalid.
    Unavailable(String),
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
    let mut speed = 1.15 * vsr;
    let mut solved = None;
    for _ in 0..THRUST_ITERATIONS {
        let force = force_at(speed)?;
        let vmc = control
            .minimum_control_speed(force, rho, a_m_s, Some(weight_n), YawTrimMode::LeastRudder)
            .ok_or(VmcFailure::NotControllable)?;
        let converged = (vmc.tas_m_s - speed).abs() < SPEED_TOLERANCE_M_S;
        solved = Some((vmc, force));
        if converged {
            break;
        }
        speed = 0.5 * (speed + vmc.tas_m_s);
    }
    let (vmc, force) = solved.ok_or(VmcFailure::NotControllable)?;
    let vmcg = control
        .minimum_control_speed(force, rho, a_m_s, None, YawTrimMode::RudderOnly)
        .map(|ground| ground.tas_m_s);
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
        fin_area_ratio_for_ceiling: fin_area_ratio(control, force, rho, a_m_s, weight_n, target),
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
    let sideslip = (CROSSWIND_DESIGN_M_S / approach).atan();
    let rudder = control.crosswind_rudder_rad(sideslip, approach / sea_level.speed_of_sound_m_s)?;
    Some(CrosswindAssessment {
        approach_speed_m_s: approach,
        sideslip_deg: sideslip.to_degrees(),
        rudder_deg: rudder.to_degrees(),
        rudder_limit_deg: control.rudder_max_deflection_rad.to_degrees(),
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
        // reported without ranking.
        let (value, row_role, detail) = match &outcome.at_sea_level {
            Err(VmcFailure::NotControllable) => (
                f64::INFINITY,
                role,
                "the fin cannot balance the engine-out yawing moment at any speed".to_owned(),
            ),
            Err(VmcFailure::Unavailable(reason)) => {
                (f64::NAN, ResidualRole::Diagnostic, reason.clone())
            }
            Ok(_) => (f64::NAN, ResidualRole::Diagnostic, String::new()),
        };
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
        landing_mass_kg: outcome.sized.design_landing_mass_kg,
        s_ref_m2,
        static_per_engine_n: Some(outcome.static_thrust_kn * 1_000.0)
            .filter(|thrust| thrust.is_finite() && *thrust > 0.0),
        departure: outcome.departure,
        clean_cd0: &|mach, altitude_m, isa_c| drag.cd_at_atmosphere(0.0, mach, altitude_m, isa_c),
    })
}

#[cfg(test)]
mod tests;
