// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CS 25.121(c), 25.121(d) and 25.119 climb checks as thrust requirements.
//!
//! The second segment (25.121(b)) goes through
//! [`alas_perf::performance::assess_oei_climb`]; the three checks here reuse
//! the same steady-flight balance, [`required_sls_thrust_to_weight`], at their
//! own speed, weight, configuration and thrust rating:
//!
//! | check | speed | CL | configuration | weight | thrust |
//! |---|---|---|---|---|---|
//! | 25.121(c) final segment | 1.18 VSR (clean) | `CLmax_clean / 1.18^2` | flaps and gear up | take-off | maximum continuous, one engine out, 1500 ft above the field |
//! | 25.121(d) approach climb | 1.4 VSR of the approach configuration | `CLmax_app / 1.4^2` | take-off flaps if `CLmax_to >= CLmax_land / 1.10^2`, else landing flaps; gear up | landing | go-around, one engine out, arrival field |
//! | 25.119 landing climb | VREF = 1.23 VSR0 | `CLmax_land / 1.23^2` | landing flaps, gear down | landing | go-around x spool-up fraction, all engines, arrival field |
//!
//! `VSR` is read on a 1-g basis from the configured `CLmax` (see the module
//! documentation of `alas_config::performance`). The approach climb is flown in
//! the lower-drag of the two modelled flap settings whose `VSR` does not exceed
//! 110 % of the landing `VSR0` (see [`approach_configuration`]). The clean lattice drag is
//! extrapolated beyond the clean `CLmax` at the landing lift coefficients
//! (no flap induced-drag model beyond the configured increments): a recorded
//! limitation.
//!
//! CS 25.115 (net flight path, 0.8/0.9/1.0 percent) is a take-off
//! obstacle-clearance reduction, weaker than the 25.121(b) gradient, so it is
//! reported as a Diagnostic margin only (no obstacle model).

use alas_atmo::us1976_compute_values;
use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_perf::performance::{
    cl_at_speed_ratio, cs25_115_net_path_reduction, far25_engine_count_supported,
    required_sls_thrust_to_weight, ClimbPoint, ClimbSegment, APPROACH_CLIMB_MAX_OVER_VSR,
    APPROACH_CONFIG_MAX_VSR_OVER_LANDING, FINAL_TAKEOFF_MIN_OVER_VSR, VREF_MIN_OVER_VSR0,
};

use super::oei_deck::{EngineOutCondition, EngineOutModel, EngineOutRating, TrimDragBasis};
use super::thrust_uncertainty::{takeoff_banded_rows, takeoff_thrust_scatter};
use crate::mdo::types::ConstraintFamily::Performance;
use crate::mdo::types::{CandidateDrag, ConstraintResidual};
use crate::mdo::ResidualRole;

/// Height above the take-off surface at which the final segment ends: 1500 ft
/// (CS 25.111(c)(4)), m.
pub const FINAL_SEGMENT_HEIGHT_M: f64 = 457.2;

/// Everything the three checks read.
pub struct CertificationInputs<'a> {
    /// The configuration (performance, requirements).
    pub config: &'a AlasConfig,
    /// The candidate's clean drag model.
    pub drag: &'a CandidateDrag,
    /// The bound engine-out evidence builder.
    pub engine_out: &'a EngineOutModel,
    /// Reference wing area, m^2.
    pub s_ref_m2: f64,
    /// Take-off mass, kg: the basis of the installed thrust-to-weight.
    pub takeoff_mass_kg: f64,
    /// Maximum landing mass, kg.
    pub landing_mass_kg: f64,
    /// Departure aerodrome, when resolved.
    pub departure: Option<&'a Airport>,
    /// Arrival aerodrome, when resolved.
    pub arrival: Option<&'a Airport>,
}

/// One evaluated check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimbEvaluation {
    /// The clause.
    pub segment: ClimbSegment,
    /// True airspeed of the check, m/s.
    pub speed_m_s: f64,
    /// Lift coefficient of the check.
    pub cl: f64,
    /// Total drag coefficient including every increment of the clause.
    pub cd: f64,
    /// Required gradient.
    pub gradient: f64,
    /// Per-engine thrust ratio (including the spool-up fraction).
    pub thrust_ratio: f64,
    /// Failed-engine drag coefficient included in `cd` (0 with all engines).
    pub windmilling_cd: f64,
    /// Asymmetric trim drag coefficient included in `cd` (0 with all engines).
    pub asymmetric_trim_cd: f64,
    /// Origin of the trim term.
    pub trim_basis: Option<TrimDragBasis>,
    /// Required sea-level-static thrust over take-off weight.
    pub required_sls_tw: f64,
}

/// The result of one check.
#[derive(Debug, Clone, PartialEq)]
pub enum ClimbOutcome {
    /// Evaluated from complete evidence.
    Evaluated(ClimbEvaluation),
    /// The aerodrome or an evidence term is missing or invalid.
    EvidenceGap(String),
}

fn gap(reason: impl Into<String>) -> ClimbOutcome {
    ClimbOutcome::EvidenceGap(reason.into())
}

/// Evaluate the final segment, approach climb and landing climb.
///
/// Returns an empty list for an engine count outside the 2/3/4 schedule: the
/// second-segment assessor already reports that case.
pub fn evaluate_climb_checks(
    inputs: &CertificationInputs<'_>,
) -> Vec<(ClimbSegment, ClimbOutcome)> {
    let n_engines = inputs.engine_out.n_engines() as i64;
    if !far25_engine_count_supported(n_engines) {
        return Vec::new();
    }
    vec![
        (ClimbSegment::FinalSegment, final_segment(inputs, n_engines)),
        (
            ClimbSegment::ApproachClimb,
            approach_climb(inputs, n_engines),
        ),
        (ClimbSegment::LandingClimb, landing_climb(inputs, n_engines)),
    ]
}

/// Speed [m/s] at which `mass_kg` flies at lift coefficient `cl` in air of
/// density `rho`.
fn speed_at_cl(mass_kg: f64, gravity_m_s2: f64, rho: f64, s_ref_m2: f64, cl: f64) -> f64 {
    (2.0 * mass_kg * gravity_m_s2 / (rho * s_ref_m2 * cl)).sqrt()
}

fn final_segment(inputs: &CertificationInputs<'_>, n_engines: i64) -> ClimbOutcome {
    let Some(departure) = inputs.departure else {
        return gap("departure aerodrome unresolved; final segment not scored");
    };
    let perf = &inputs.config.performance;
    let g = inputs.config.requirements.gravity_m_s2;
    let Some(cl) = cl_at_speed_ratio(perf.cl_max_clean, FINAL_TAKEOFF_MIN_OVER_VSR) else {
        return gap("clean CLmax is invalid");
    };
    let altitude_m = departure.elevation_m + FINAL_SEGMENT_HEIGHT_M;
    let atmosphere = us1976_compute_values(altitude_m, departure.isa_deviation_c);
    let speed = speed_at_cl(
        inputs.takeoff_mass_kg,
        g,
        atmosphere.density_kg_m3,
        inputs.s_ref_m2,
        cl,
    );
    let mach = speed / atmosphere.speed_of_sound_m_s;
    let clean =
        inputs
            .drag
            .cd_airport_base_at_atmosphere(cl, mach, altitude_m, departure.isa_deviation_c);
    let cd0 = inputs
        .drag
        .cd_at_atmosphere(0.0, mach, altitude_m, departure.isa_deviation_c);
    engine_out_outcome(
        inputs,
        n_engines,
        ClimbSegment::FinalSegment,
        EngineOutCondition {
            altitude_m,
            isa_deviation_c: departure.isa_deviation_c,
            tas_m_s: speed,
        },
        EngineOutRating::MaximumContinuous,
        cl,
        clean,
        cd0,
        0.0,
        1.0,
    )
}

/// The approach configuration of CS 25.121(d): `(CLmax, flap drag increment)`.
///
/// The clause does not fix the flap setting; it admits any configuration whose
/// `VSR` is at most 1.10 of the landing configuration's `VSR0`
/// ([`APPROACH_CONFIG_MAX_VSR_OVER_LANDING`]). Stall speed goes as
/// `1/sqrt(CLmax)`, so a configuration qualifies when
/// `CLmax >= CLmax_land / 1.10^2`. Two configurations are modelled, take-off
/// flaps (gear up, `oei_climb_delta_cd`) and landing flaps (gear up,
/// `landing_flap_delta_cd`); the lower-drag one that qualifies is flown, as
/// the normal go-around procedure does. A type whose take-off flap `CLmax`
/// is within 21 % of the landing value (ATR 72: the two are equal within the
/// recovered speed records) therefore climbs out at take-off flaps; a type
/// whose landing flap adds much more lift (every jet in the registry) stays in
/// the landing configuration, which is the previous behaviour.
fn approach_configuration(perf: &alas_config::PerformanceConfig) -> (f64, f64) {
    let landing = (perf.cl_max_land, perf.landing_flap_delta_cd);
    let takeoff = (perf.cl_max_to, perf.oei_climb_delta_cd);
    let qualifies = takeoff.0.is_finite()
        && takeoff.0 > 0.0
        && takeoff.1.is_finite()
        && takeoff.0 * APPROACH_CONFIG_MAX_VSR_OVER_LANDING.powi(2) >= perf.cl_max_land;
    if qualifies && takeoff.1 <= landing.1 {
        takeoff
    } else {
        landing
    }
}

fn approach_climb(inputs: &CertificationInputs<'_>, n_engines: i64) -> ClimbOutcome {
    let Some(arrival) = inputs.arrival else {
        return gap("arrival aerodrome unresolved; approach climb not scored");
    };
    let perf = &inputs.config.performance;
    let g = inputs.config.requirements.gravity_m_s2;
    let (cl_max_approach, flap_delta_cd) = approach_configuration(perf);
    let Some(cl) = cl_at_speed_ratio(cl_max_approach, APPROACH_CLIMB_MAX_OVER_VSR) else {
        return gap("approach-configuration CLmax is invalid");
    };
    let atmosphere = us1976_compute_values(arrival.elevation_m, arrival.isa_deviation_c);
    let speed = speed_at_cl(
        inputs.landing_mass_kg,
        g,
        atmosphere.density_kg_m3,
        inputs.s_ref_m2,
        cl,
    );
    let mach = speed / atmosphere.speed_of_sound_m_s;
    let clean = inputs.drag.cd_airport_base_at_atmosphere(
        cl,
        mach,
        arrival.elevation_m,
        arrival.isa_deviation_c,
    );
    let cd0 = inputs
        .drag
        .cd_at_atmosphere(0.0, mach, arrival.elevation_m, arrival.isa_deviation_c);
    engine_out_outcome(
        inputs,
        n_engines,
        ClimbSegment::ApproachClimb,
        EngineOutCondition {
            altitude_m: arrival.elevation_m,
            isa_deviation_c: arrival.isa_deviation_c,
            tas_m_s: speed,
        },
        EngineOutRating::TakeoffGoAround,
        cl,
        clean,
        cd0,
        flap_delta_cd,
        inputs.landing_mass_kg / inputs.takeoff_mass_kg,
    )
}

// One named physical input per term of the climb balance.
#[allow(clippy::too_many_arguments)]
fn engine_out_outcome(
    inputs: &CertificationInputs<'_>,
    n_engines: i64,
    segment: ClimbSegment,
    condition: EngineOutCondition,
    rating: EngineOutRating,
    cl: f64,
    clean_cd: f64,
    clean_cd0: f64,
    configuration_cd: f64,
    weight_over_takeoff: f64,
) -> ClimbOutcome {
    let point = match inputs.engine_out.point(condition, rating, clean_cd0) {
        Ok(point) => point,
        Err(reason) => return gap(format!("{}: {reason}", segment.id())),
    };
    let Some(trim_cd) = point.asymmetric_trim_cd else {
        return gap(format!(
            "{}: asymmetric trim drag unavailable (no fin geometry and no configured value)",
            segment.id()
        ));
    };
    let cd = clean_cd + configuration_cd + point.windmilling_cd + trim_cd;
    let Some(required) = required_sls_thrust_to_weight(ClimbPoint {
        segment,
        n_engines,
        cl,
        cd,
        weight_over_takeoff,
        thrust_ratio: point.thrust_ratio,
    }) else {
        return gap(format!(
            "{}: inputs outside the supported domain",
            segment.id()
        ));
    };
    ClimbOutcome::Evaluated(ClimbEvaluation {
        segment,
        speed_m_s: condition.tas_m_s,
        cl,
        cd,
        gradient: segment.gradient(n_engines).unwrap_or(f64::NAN),
        thrust_ratio: point.thrust_ratio,
        windmilling_cd: point.windmilling_cd,
        asymmetric_trim_cd: trim_cd,
        trim_basis: Some(point.trim_basis),
        required_sls_tw: required,
    })
}

fn landing_climb(inputs: &CertificationInputs<'_>, n_engines: i64) -> ClimbOutcome {
    let Some(arrival) = inputs.arrival else {
        return gap("arrival aerodrome unresolved; landing climb not scored");
    };
    let perf = &inputs.config.performance;
    let g = inputs.config.requirements.gravity_m_s2;
    let Some(cl) = cl_at_speed_ratio(perf.cl_max_land, VREF_MIN_OVER_VSR0) else {
        return gap("landing CLmax is invalid");
    };
    let atmosphere = us1976_compute_values(arrival.elevation_m, arrival.isa_deviation_c);
    let speed = speed_at_cl(
        inputs.landing_mass_kg,
        g,
        atmosphere.density_kg_m3,
        inputs.s_ref_m2,
        cl,
    );
    let mach = speed / atmosphere.speed_of_sound_m_s;
    let clean = inputs.drag.cd_airport_base_at_atmosphere(
        cl,
        mach,
        arrival.elevation_m,
        arrival.isa_deviation_c,
    );
    let thrust = inputs.engine_out.thrust(
        EngineOutCondition {
            altitude_m: arrival.elevation_m,
            isa_deviation_c: arrival.isa_deviation_c,
            tas_m_s: speed,
        },
        EngineOutRating::TakeoffGoAround,
        false,
    );
    let (_, ratio) = match thrust {
        Ok(value) => value,
        Err(reason) => return gap(format!("{}: {reason}", ClimbSegment::LandingClimb.id())),
    };
    let spool = inputs.engine_out.spool_fraction();
    let cd = clean + perf.landing_flap_delta_cd + perf.landing_gear_delta_cd;
    let point = ClimbPoint {
        segment: ClimbSegment::LandingClimb,
        n_engines,
        cl,
        cd,
        weight_over_takeoff: inputs.landing_mass_kg / inputs.takeoff_mass_kg,
        thrust_ratio: ratio * spool,
    };
    match required_sls_thrust_to_weight(point) {
        Some(required) => ClimbOutcome::Evaluated(ClimbEvaluation {
            segment: ClimbSegment::LandingClimb,
            speed_m_s: speed,
            cl,
            cd,
            gradient: ClimbSegment::LandingClimb
                .gradient(n_engines)
                .unwrap_or(f64::NAN),
            thrust_ratio: ratio * spool,
            windmilling_cd: 0.0,
            asymmetric_trim_cd: 0.0,
            trim_basis: None,
            required_sls_tw: required,
        }),
        None => gap("landing_climb: inputs outside the supported domain"),
    }
}

/// Gross-to-net take-off gradient margin, CS 25.115(b): the steady gradient
/// the installed thrust delivers at `V2` with one engine out, minus the
/// 0.8/0.9/1.0 percent reduction. Positive means the net path still climbs.
///
/// `required_gradient` and `required_inflight_tw` are the 25.121(b) gradient
/// and the in-flight engine-equivalent thrust-to-weight that holds it;
/// `available_sls_tw` the installed static thrust over weight and
/// `thrust_ratio` the V2 thrust over static thrust. A Diagnostic only:
/// without an obstacle model this is not a constraint.
pub fn net_path_gradient_margin(
    n_engines: i64,
    required_gradient: f64,
    required_inflight_tw: f64,
    available_sls_tw: f64,
    thrust_ratio: f64,
) -> Option<f64> {
    let reduction = cs25_115_net_path_reduction(n_engines)?;
    let finite = [
        required_gradient,
        required_inflight_tw,
        available_sls_tw,
        thrust_ratio,
    ]
    .iter()
    .all(|value| value.is_finite());
    if !finite || thrust_ratio <= 0.0 {
        return None;
    }
    let live_fraction = (n_engines - 1) as f64 / n_engines as f64;
    let gross = required_gradient
        + live_fraction * (available_sls_tw * thrust_ratio - required_inflight_tw);
    Some(gross - reduction)
}

/// Optimizer residuals for the three climb checks: a thrust-requirement
/// residual per evaluated check, a Diagnostic evidence-gap flag per check
/// that could not be evaluated.
pub(super) fn certification_residuals(
    inputs: &CertificationInputs<'_>,
    available_tw: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    evaluate_climb_checks(inputs)
        .into_iter()
        .flat_map(|(segment, outcome)| match outcome {
            ClimbOutcome::Evaluated(evaluation) => {
                // The approach and landing climbs fly the take-off/go-around
                // rating: the take-off band applies. The final segment is at
                // maximum continuous (the climb chart), not a take-off check.
                let (band, warning_id, what) = match segment {
                    ClimbSegment::ApproachClimb => (
                        takeoff_thrust_scatter(inputs.config),
                        "oei_approach_climb_within_takeoff_band",
                        "CS 25.121(d) approach climb (static T/W)",
                    ),
                    ClimbSegment::LandingClimb => (
                        takeoff_thrust_scatter(inputs.config),
                        "landing_climb_within_takeoff_band",
                        "CS 25.119 landing climb (static T/W)",
                    ),
                    _ => (0.0, "", ""),
                };
                takeoff_banded_rows(
                    segment.id(),
                    warning_id,
                    what,
                    available_tw,
                    evaluation.required_sls_tw,
                    band,
                    role,
                )
            }
            ClimbOutcome::EvidenceGap(_) => vec![ConstraintResidual::direct(
                evidence_gap_id(segment),
                Performance,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                ResidualRole::Diagnostic,
            )],
        })
        .collect()
}

fn evidence_gap_id(segment: ClimbSegment) -> &'static str {
    match segment {
        ClimbSegment::SecondSegment => "oei_second_segment_evidence_gap",
        ClimbSegment::FinalSegment => "oei_final_segment_evidence_gap",
        ClimbSegment::ApproachClimb => "oei_approach_climb_evidence_gap",
        ClimbSegment::LandingClimb => "landing_climb_evidence_gap",
    }
}

#[cfg(test)]
mod tests;
