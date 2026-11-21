// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The jet take-off field length of the optimiser constraint and the reports.
//!
//! One definition: [`alas_perf::performance::takeoff_field`] (CS 25.113(a)
//! with the CS 25.109 accelerate-stop balance) fed by the engine deck thrust
//! at every speed (take-off rating, Jenkinson take-off chart), the failed
//! engine's windmilling plus trim drag from the shared engine-out evidence
//! builder, and the candidate drag in the low-speed quadratic form of
//! [`field_polar`]. The optimiser residual `takeoff_field`, the airport
//! feasibility finding, the ISA sea-level field reference and the report
//! inputs all call [`jet_takeoff_field`], once per ground plane of the
//! lift-off lift ([`longest_on_ground_planes`]).

use alas_config::airports::Airport;
use alas_config::{AlasConfig, PerformanceConfig};
use alas_perf::performance::{
    compute_v_speeds_at_masses, field_breakdown, takeoff_field, FieldBreakdown, FieldPolar,
    LiftOffSchedule, TakeoffField, TakeoffFieldInputs, VSpeeds, ROTATION_PITCH_RATE_DEG_S,
};

use super::improved_climb::{resolve_improved_climb, ImprovedClimbInputs};
use super::net_path::V2ClimbOutPolar;
use super::oei_deck::{EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe};
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::{CandidateDrag, ConstraintFamily, ConstraintResidual};
use crate::mdo::ResidualRole;

mod correlation;
mod planes;
#[cfg(test)]
pub(super) use correlation::correlation_residual;
use correlation::{correlation_distance_m, correlation_row};
pub(super) use planes::field_on_planes;
pub use planes::longest_on_ground_planes;

/// AMC1 CAT.POL.A.210 (a)(2)(i): the take-off distance is at least 115 % of the
/// all-engine distance to 35 ft.
const AEO_DISTANCE_FACTOR: f64 = 1.15;

/// Standard gravity used for the weight, m/s^2 (the program-wide `g0`).
const G: f64 = alas_atmo::STANDARD_GRAVITY;

/// Spacing of the lift-coefficient grid of [`FieldPolar::lift_drag`].
const LIFT_GRID_STEP: f64 = 0.1;
/// Largest lift coefficient of that grid: above the take-off `CLmax` of any
/// registered configuration.
const LIFT_GRID_MAX: f64 = 3.0;

/// Low-speed field polar of a candidate's drag model.
///
/// Three samples of the airport-base drag (no lift-dependent viscous term)
/// at the field atmosphere and `mach` give the parabola `(cd0, k)`; the
/// table is also sampled on a lift grid for [`FieldPolar::lift_drag`].
///
/// # Errors
///
/// The fit is not finite and positive.
pub fn field_polar(
    drag: &CandidateDrag,
    perf: &PerformanceConfig,
    mach: f64,
    airport: &Airport,
) -> Result<FieldPolar, String> {
    let cd = |cl| {
        drag.cd_airport_base_at_atmosphere(
            cl,
            mach,
            airport.elevation_m,
            airport.isa_deviation_c,
            crate::mdo::drag_table::high_lift_lift_shift(perf.cl_max_to, perf.cl_max_clean),
        )
    };
    let (c0, c_half, c_one) = (cd(0.0), cd(0.5), cd(1.0));
    let k = 2.0 * (c_one - 2.0 * c_half + c0);
    let c1 = c_one - c0 - k;
    let cd0 = c0 + c1.max(0.0) * perf.cl_max_to;
    if !(cd0.is_finite() && cd0 > 0.0 && k.is_finite() && k > 0.0 && c0.is_finite() && c0 > 0.0) {
        return Err(
            "candidate field polar needs finite positive CD0 and induced factor".to_owned(),
        );
    }
    let cells = (LIFT_GRID_MAX / LIFT_GRID_STEP).round() as usize;
    let lift_drag: Vec<f64> = (0..=cells)
        .map(|i| cd(i as f64 * LIFT_GRID_STEP) - c0)
        .collect();
    if !lift_drag.iter().all(|value| value.is_finite()) {
        return Err("candidate field drag table is not finite over the lift grid".to_owned());
    }
    Ok(FieldPolar::from_table(
        cd0,
        k,
        c0,
        LIFT_GRID_STEP,
        lift_drag,
    ))
}

/// The speed schedule of the field-length row at the design gross mass, with
/// the improved-climb `V2` resolved at that mass (`base` is the configuration
/// with the computed `VMC` and no improved-climb override). `None` when the
/// engine-out evidence cannot be bound.
pub(super) fn design_mass_speeds(
    base: &AlasConfig,
    outcome: &SizingOutcome,
    departure: &Airport,
    s_ref_m2: f64,
    gravity_m_s2: f64,
) -> Option<VSpeeds> {
    let sized = &outcome.sized;
    let mass_kg = sized.design_gross_mass_kg;
    let static_thrust_n = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0;
    let model = EngineOutModel::new(
        base,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref_m2,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()?
    .with_weight_kg(mass_kg);
    let (config, _) = resolve_improved_climb(
        base,
        &ImprovedClimbInputs {
            model: &model,
            departure,
            mass_kg,
            s_ref_m2,
            drag: &sized.fuel_artifacts.drag,
            installed_tw_sls: static_thrust_n / (mass_kg * gravity_m_s2),
            aspect_ratio: super::net_path::climb_out_aspect_ratio(
                base,
                outcome.plane.b_ref,
                outcome.plane.s_ref,
            ),
        },
    );
    let perf = &config.performance;
    Some(compute_v_speeds_at_masses(
        mass_kg,
        sized.design_landing_mass_kg,
        s_ref_m2,
        departure,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    ))
}

/// Inputs of [`jet_takeoff_field`].
pub struct JetFieldInputs<'a> {
    /// Performance configuration (friction, braking, drag increments).
    pub perf: &'a PerformanceConfig,
    /// Engine deck and airframe evidence, weight bound.
    pub engine_out: &'a EngineOutModel,
    /// Departure aerodrome (elevation and ISA deviation).
    pub airport: &'a Airport,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Take-off mass, kg.
    pub mass_kg: f64,
    /// Installed sea-level static thrust of all engines, N.
    pub static_thrust_n: f64,
    /// Clean field polar from [`field_polar`].
    pub polar: &'a FieldPolar,
    /// Speed schedule at this mass and field (improved-climb V2 included).
    pub speeds: VSpeeds,
    /// The V2 climb-out locus of the take-off configuration (Torenbeek 1982,
    /// Eq. 5-29), the polar the CS 25.121(b) gradient and the improved-climb
    /// `V2` are judged on. With it the air legs of the integration fly that
    /// polar (gear increment added); `None` flies them on the drag table.
    pub climb_out: Option<V2ClimbOutPolar>,
    /// The take-off configuration's runway airframe on one ground plane
    /// ([`crate::envelope::liftoff_lifts`]): `VMU` and its CS 25.107(e)(1)(iv)
    /// margin, the rotation at [`ROTATION_PITCH_RATE_DEG_S`] with the lift,
    /// strut extension and wing rise of every instant, and the drag's ground
    /// effect on the same wing ([`crate::envelope::LiftOffLift::ground_effect()`]).
    /// `None` keeps the fixed rotation time, no `VMU` and the engine-out
    /// model's wing.
    pub liftoff: Option<crate::envelope::LiftOffLift>,
}

/// Solve the CS 25.113(a) field length of a jet.
///
/// # Errors
///
/// An engine-deck or drag-evidence failure, a missing asymmetric trim drag,
/// or a take-off the physics cannot complete (as text).
pub fn jet_takeoff_field(inputs: &JetFieldInputs<'_>) -> Result<TakeoffField, String> {
    with_jet_field_inputs(inputs, takeoff_field)
}

/// [`jet_takeoff_field`] with the distance of every leg (diagnostics).
///
/// # Errors
///
/// As [`jet_takeoff_field`].
pub fn jet_takeoff_field_breakdown(
    inputs: &JetFieldInputs<'_>,
) -> Result<(TakeoffField, FieldBreakdown), String> {
    with_jet_field_inputs(inputs, |field| {
        let solved = takeoff_field(field)?;
        Ok((solved, field_breakdown(field, &solved)?))
    })
}

/// Build the integration inputs of `inputs` and hand them to `solve` (the
/// probes vary them as the pipeline does not).
///
/// # Errors
///
/// As [`jet_takeoff_field`], and the errors of `solve`.
pub fn with_jet_field_inputs<R>(
    inputs: &JetFieldInputs<'_>,
    solve: impl FnOnce(&TakeoffFieldInputs<'_>) -> Result<R, String>,
) -> Result<R, String> {
    let perf = inputs.perf;
    let airport = inputs.airport;
    let model = inputs.engine_out;
    let n = model.n_engines();
    if n < 2 {
        return Err("balanced field length needs at least two engines".to_owned());
    }
    let atmosphere = alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
    let at = |tas_m_s| EngineOutCondition {
        altitude_m: airport.elevation_m,
        isa_deviation_c: airport.isa_deviation_c,
        tas_m_s,
    };
    let polar = inputs.polar;
    let point = model.point(
        at(inputs.speeds.v2_ms),
        EngineOutRating::TakeoffGoAround,
        polar.cd0,
    )?;
    let trim = point.asymmetric_trim_cd.ok_or_else(|| {
        "jet field length needs the asymmetric trim drag: configure oei_asymmetric_trim_cd \
         or evaluate with an airframe that has a fin"
            .to_owned()
    })?;
    let aeo = |v: f64| {
        let ratio = model
            .thrust(at(v), EngineOutRating::TakeoffGoAround, false)?
            .1;
        Ok(ratio * inputs.static_thrust_n)
    };
    let oei = |v: f64| aeo(v).map(|thrust| thrust * (n - 1) as f64 / n as f64);
    // The air legs on the climb-out locus: `C0 + CL^2 / (pi A E)`, with `E`
    // carrying the asymmetric trim where the source states the reduction (the
    // separate trim term is then not added, as in the gradient check), and
    // the failed engine's windmilling drag. The locus is read at no more than
    // the take-off CLmax, its domain (the air legs fly below it: VLOF >= VR).
    let cl_max_to = perf.cl_max_to;
    let climb_out = inputs.climb_out;
    let airborne = move |cl: f64, engine_failed: bool| -> (f64, f64) {
        let Some(locus) = climb_out else {
            return (f64::NAN, f64::NAN);
        };
        let cl = cl.clamp(1e-6, cl_max_to);
        let zero = locus.cd(1e-6, cl_max_to, engine_failed).unwrap_or(f64::NAN);
        let total = locus.cd(cl, cl_max_to, engine_failed).unwrap_or(f64::NAN);
        let failed = if engine_failed {
            point.windmilling_cd
                + if locus.engine_failure_e_factor().is_some() {
                    0.0
                } else {
                    trim
                }
        } else {
            0.0
        };
        (zero + failed, total - zero)
    };
    // CS 25.107(d): VMU at the lift of the tail-strike attitude in ground
    // effect, or at CLmax,TO when the wing stalls first.
    let weight_n = inputs.mass_kg * G;
    let liftoff = inputs.liftoff.as_ref().map(|lift| {
        let (cl_unstick, geometry_limited) = lift.unstick_lift(perf.cl_max_to);
        LiftOffSchedule {
            vmu_m_s: (2.0 * weight_n
                / (atmosphere.density_kg_m3 * inputs.wing_area_m2 * cl_unstick))
                .sqrt(),
            geometry_limited,
            pitch_rate_deg_s: ROTATION_PITCH_RATE_DEG_S,
            cl_max_to: perf.cl_max_to,
            airframe: lift,
        }
    });
    solve(&TakeoffFieldInputs {
        weight_n,
        wing_area_m2: inputs.wing_area_m2,
        density_kg_m3: atmosphere.density_kg_m3,
        speeds: inputs.speeds,
        rolling_friction: perf.propeller_takeoff_rolling_friction,
        stop_deceleration_g: perf.propeller_takeoff_stop_deceleration_g,
        cd0: polar.zero_lift() + perf.oei_climb_delta_cd,
        gear_delta_cd: perf.takeoff_gear_delta_cd,
        lift_drag: &|cl| polar.lift_drag(cl),
        // The schedule's wing on its own plane, else the engine-out model's.
        ground_effect: match &inputs.liftoff {
            Some(lift) => lift.ground_effect(),
            None => model.ground_effect(),
        },
        failed_engine_cd: point.windmilling_cd + trim,
        thrust_aeo_n: &aeo,
        thrust_oei_n: &oei,
        airborne_drag: inputs
            .climb_out
            .is_some()
            .then_some(&airborne as &dyn Fn(f64, bool) -> (f64, f64)),
        liftoff,
    })
}

/// The 35 ft screen of the take-off the net flight path starts from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ScreenPoint {
    /// Take-off distance to 35 ft from the start of the roll, m (AMC1
    /// CAT.POL.A.210 (a)(2): the longer of 115 % of the all-engine distance and
    /// the continued one-engine-inoperative distance).
    pub(super) distance_m: f64,
    /// Time the continued take-off spends between lift-off and the screen, s;
    /// `None` where the field is a correlation that has no air leg.
    pub(super) airborne_time_s: Option<f64>,
}

/// Time from lift-off to the 35 ft screen of the continued take-off, s: its air
/// distance over the mean of the lift-off speed and `V2` (the integration
/// raises speed and height together from `VLOF` to `V2` at the screen).
fn airborne_time_s(legs: &FieldBreakdown, v2_m_s: f64) -> Option<f64> {
    let mean_speed = 0.5 * (legs.continued_vlof_m_s + v2_m_s.max(legs.continued_vlof_m_s));
    let time = legs.continued_air_m / mean_speed;
    (time.is_finite() && time >= 0.0).then_some(time)
}

/// Everything the `takeoff_field` residual reads.
pub(super) struct ResidualInputs<'a> {
    pub(super) perf: &'a PerformanceConfig,
    pub(super) engine_out: Option<&'a EngineOutModel>,
    pub(super) drag: &'a CandidateDrag,
    pub(super) departure: &'a Airport,
    pub(super) speeds: Option<VSpeeds>,
    pub(super) s_ref_m2: f64,
    /// Design gross mass (the mass the field is certified at), kg.
    pub(super) mass_kg: f64,
    /// Installed sea-level static thrust of all engines, N.
    pub(super) static_thrust_n: f64,
    pub(super) gravity_m_s2: f64,
    pub(super) role: ResidualRole,
    /// The V2 climb-out locus of the candidate, see [`JetFieldInputs::climb_out`].
    pub(super) climb_out: Option<V2ClimbOutPolar>,
}

/// The `takeoff_field` residual.
///
/// Jets: the CS 25.113 field length against the take-off distance available,
/// in metres (violation = `(TOFL - TODA) / TOFL`). Turboprops and the
/// translated replay keep the Raymer required-versus-installed `T/W`
/// correlation (`legacy_field_correlations`; the turboprop field is judged on
/// the Torenbeek integration in the pipeline). A jet whose field cannot be
/// evaluated because the continued take-off has no climb gradient is an
/// infinite violation; a missing evidence term is a Diagnostic gap.
/// The `takeoff_field` residual and the take-off distance of the net flight
/// path (AMC1 CAT.POL.A.210 (a)(2): the longer of 115 % of the all-engine
/// distance to 35 ft and the continued one-engine-inoperative distance to
/// 35 ft, from the start of the take-off), m. A turboprop or the legacy
/// correlation reads it by inverting the correlation at the installed
/// thrust-to-weight. `None` when the field cannot be evaluated. `liftoff` is
/// the lift-off lift of one ground plane ([`field_on_planes`] flies them all).
pub(super) fn takeoff_field_outcome(
    inputs: &ResidualInputs<'_>,
    liftoff: Option<crate::envelope::LiftOffLift>,
) -> (ConstraintResidual, Option<ScreenPoint>) {
    let perf = inputs.perf;
    let departure = inputs.departure;
    let physical = planes::flies_physical(inputs);
    let (Some(model), Some(speeds), true) = (inputs.engine_out, inputs.speeds, physical) else {
        return (
            correlation_row(inputs),
            correlation_distance_m(inputs).map(|distance_m| ScreenPoint {
                distance_m,
                airborne_time_s: None,
            }),
        );
    };
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let gap = planes::evidence_gap;
    let Ok(polar) = field_polar(inputs.drag, perf, mach, departure) else {
        return (gap(), None);
    };
    let polar = &polar;
    match jet_takeoff_field_breakdown(&JetFieldInputs {
        perf,
        engine_out: model,
        airport: departure,
        wing_area_m2: inputs.s_ref_m2,
        mass_kg: inputs.mass_kg,
        static_thrust_n: inputs.static_thrust_n,
        polar,
        speeds,
        climb_out: inputs.climb_out,
        liftoff,
    }) {
        Ok((field, legs)) => (
            ConstraintResidual::scaled(
                "takeoff_field",
                ConstraintFamily::Performance,
                departure.toda_m,
                field.tofl_m,
                "m",
                field.tofl_m - departure.toda_m,
                inputs.role,
            ),
            Some(ScreenPoint {
                distance_m: (AEO_DISTANCE_FACTOR * field.all_engine_m).max(field.continued_m),
                airborne_time_s: airborne_time_s(&legs, speeds.v2_ms),
            }),
        ),
        Err(reason) if reason.contains("asymmetric trim") => (gap(), None),
        Err(reason) => (
            ConstraintResidual::direct(
                "takeoff_field",
                ConstraintFamily::Performance,
                departure.toda_m,
                f64::NAN,
                "m",
                f64::NAN,
                f64::INFINITY,
                inputs.role,
            )
            .with_detail(reason),
            None,
        ),
    }
}
