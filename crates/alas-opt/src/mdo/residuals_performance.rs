// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airworthiness performance family: engine-out second-segment climb,
//! cruise and takeoff thrust margin, landing field length and approach speed.

use crate::mdo::ResidualRole;

use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignRequirements, ObjectiveConfig, PerformanceConfig};
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, density_ratio, far25_oei_gradient, oei_cl_at_v2,
    tw_takeoff_constraint, ws_landing_limit, OeiClimbStatus, OeiV2Condition,
};
use alas_units::KNOT;

use super::sizing::SizingOutcome;
use super::types::CandidateDrag;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// The engine-out climb, cruise and takeoff thrust-margin residuals, plus
/// (when the arrival aerodrome resolves) landing field length and approach
/// speed.
pub(super) fn performance_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let req = &config.requirements;
    let perf = &config.performance;
    let objective = &config.optimizer.objective;
    let sized = &outcome.sized;
    let available_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0
        / (sized.takeoff_mass_kg * req.gravity_m_s2);
    let s_ref = outcome.plane.s_ref.max(1e-9);

    let mut residuals = Vec::new();

    let certified_oei_gradient = far25_oei_gradient(outcome.n_engines);
    let departure_speeds = outcome.departure.map(|departure| {
        compute_v_speeds_at_masses(
            sized.takeoff_mass_kg,
            sized.takeoff_mass_kg,
            s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        )
    });
    let oei_condition = outcome
        .departure
        .zip(departure_speeds)
        .map(|(departure, speeds)| OeiV2Condition {
            departure_elevation_m: departure.elevation_m,
            departure_isa_deviation_c: departure.isa_deviation_c,
            v2_over_vstall: speeds.v2_ms / speeds.v_stall_to_ms,
            condition_to_sls_thrust_ratio: perf.oei_condition_to_sls_thrust_ratio,
            asymmetric_trim_cd: perf.oei_asymmetric_trim_cd,
            windmilling_cd: perf.oei_windmilling_cd,
        });
    // The engine-out climb is flown at the V2 lift coefficient
    // `CL_V2 = CLmax_TO / (V2/VS)^2` (14 CFR 25.107(c): V2 >= 1.13 VSR, so
    // CL_V2 <= CLmax_TO / 1.13^2; the assessor uses the same relation,
    // `oei_cl_at_v2`), at the airport elevation and the V2 Mach there. The
    // drag table is read at exactly that point, not at the cruise design CL
    // and reference altitude. Without a resolved departure the legacy
    // `oei_climb_cl` is the lift coefficient and the field is sea level.
    // The table is clean: the takeoff flap/slat increment enters through
    // `oei_climb_delta_cd`. The projection is used only at this selected CL.
    let drag = &sized.fuel_artifacts.drag;
    let oei_cl = oei_condition
        .and_then(|condition| oei_cl_at_v2(perf.cl_max_to, condition.v2_over_vstall))
        .unwrap_or(perf.oei_climb_cl);
    let (oei_altitude_m, oei_isa_deviation_c) = outcome.departure.map_or((0.0, 0.0), |airport| {
        (airport.elevation_m, airport.isa_deviation_c)
    });
    let departure_atmosphere =
        alas_atmo::us1976_compute_values(oei_altitude_m, oei_isa_deviation_c);
    let oei_speed_m_s = departure_speeds.map_or_else(
        || {
            (2.0 * sized.takeoff_mass_kg * req.gravity_m_s2
                / (departure_atmosphere.density_kg_m3 * s_ref * oei_cl))
                .sqrt()
        },
        |speeds| speeds.v2_ms,
    );
    let oei_mach = oei_speed_m_s / departure_atmosphere.speed_of_sound_m_s;
    let (oei_cd0, oei_k) =
        departure_polar(drag, oei_cl, oei_mach, oei_altitude_m, oei_isa_deviation_c);
    let oei_assessment = assess_oei_climb(
        oei_cd0,
        oei_k,
        outcome.n_engines,
        certified_oei_gradient.unwrap_or(perf.oei_gradient),
        oei_cl,
        perf.oei_climb_delta_cd,
        perf.cl_max_to,
        oei_condition,
    );
    // A conceptual in-flight estimate remains useful as a soft ranking
    // signal, but only the shared assessor's SLS-equivalent result may become
    // a hard residual. In particular, unsupported engine counts never fall
    // back to a made-up Part 25 requirement.
    if let Some(required_oei_tw) = oei_assessment
        .required_sls_tw
        .or(oei_assessment.required_inflight_tw)
    {
        let oei_policy = if oei_assessment.status == OeiClimbStatus::SlsEquivalent {
            role
        } else {
            ResidualRole::Preference
        };
        residuals.push(ConstraintResidual::scaled(
            "oei_second_segment",
            Performance,
            available_tw,
            required_oei_tw,
            "T/W",
            required_oei_tw - available_tw,
            oei_policy,
        ));
    }
    match oei_assessment.status {
        OeiClimbStatus::NotApplicable | OeiClimbStatus::SlsEquivalent => {}
        OeiClimbStatus::UnsupportedEngineCount => residuals.push(ConstraintResidual::direct(
            "oei_part25_engine_count_unsupported",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ResidualRole::Diagnostic,
        )),
        OeiClimbStatus::ConceptualInflight | OeiClimbStatus::EvidenceGap => {
            residuals.push(ConstraintResidual::direct(
                "oei_second_segment_evidence_gap",
                Performance,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                ResidualRole::Diagnostic,
            ));
        }
    }

    let cruise_ws_pa = sized.takeoff_mass_kg * req.gravity_m_s2 / s_ref;
    let cruise_atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let cruise_speed_m_s = req.cruise_mach * cruise_atmosphere.speed_of_sound();
    let cruise_q_pa = 0.5 * cruise_atmosphere.density() * cruise_speed_m_s * cruise_speed_m_s;
    let cruise_cl = cruise_ws_pa / cruise_q_pa;
    // Mass closure changes CL without rebuilding a CG-compatible table.
    // Evaluate its drag at that CL; a tangent at the original design lift
    // does not retain a shifted induced polar or nonlinear wave drag.
    let required_cruise_tw =
        drag.cd(cruise_cl, req.cruise_mach, req.cruise_altitude_m) / cruise_cl / perf.thrust_lapse;
    residuals.push(ConstraintResidual::scaled(
        "cruise_thrust",
        Performance,
        available_tw,
        required_cruise_tw,
        "T/W",
        required_cruise_tw - available_tw,
        role,
    ));

    if !outcome.airport_records_resolved {
        residuals.push(ConstraintResidual::direct(
            "airport_unknown",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    } else if !outcome.declared_airport_data_complete {
        residuals.push(ConstraintResidual::direct(
            "airport_declared_distance_unavailable",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    }

    if !outcome.mission_distance_known {
        residuals.push(ConstraintResidual::direct(
            "mission_distance_unavailable",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    }

    if outcome.minimum_profile_range_m.is_finite() && outcome.minimum_profile_range_m > 0.0 {
        residuals.push(ConstraintResidual::scaled(
            "mission_profile_range",
            Performance,
            sized.design_range_m,
            outcome.minimum_profile_range_m,
            "m",
            outcome.minimum_profile_range_m - sized.design_range_m,
            role,
        ));
    }

    // The field lengths a runway has to offer are certified at the maximum
    // masses, not at the mass a short city pair happens to dispatch: takeoff
    // at the design gross mass (MTOW) and landing at the design landing mass
    // (MLW). Both are the sizing basis's own values
    // (`alas_config::MassSizingBasis`): the declared weights of a fixed
    // aircraft, the closure of a coupled design.
    if let Some(departure) = outcome.departure {
        residuals.push(takeoff_field_residual(
            sized.design_gross_mass_kg,
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0,
            s_ref,
            req.gravity_m_s2,
            departure,
            perf.cl_max_to,
            role,
        ));
    }

    if let Some(arrival) = outcome.arrival {
        residuals.extend(landing_and_approach_residuals(
            outcome, req, perf, objective, arrival, s_ref, role,
        ));
    }

    residuals
}

/// The takeoff field-length residual at `mass_kg`, as a thrust-to-weight
/// requirement.
///
/// Raymer's empirical takeoff relation (`alas_perf::performance::
/// tw_takeoff_constraint`) gives the required `T/W` from the wing loading at
/// `mass_kg`; the available `T/W` is the installed static thrust over the same
/// weight. Both sides move with the mass, so a field that a light dispatch
/// clears can be missed at the maximum takeoff mass.
fn takeoff_field_residual(
    mass_kg: f64,
    static_thrust_n: f64,
    s_ref: f64,
    gravity_m_s2: f64,
    departure: &Airport,
    cl_max_to: f64,
    role: ResidualRole,
) -> ConstraintResidual {
    let weight_n = mass_kg * gravity_m_s2;
    let available_tw = static_thrust_n / weight_n;
    let sigma = density_ratio(departure.elevation_m, departure.isa_deviation_c);
    let required_tw =
        tw_takeoff_constraint(&[weight_n / s_ref], departure.toda_m, sigma, cl_max_to)[0];
    ConstraintResidual::scaled(
        "takeoff_field",
        Performance,
        available_tw,
        required_tw,
        "T/W",
        required_tw - available_tw,
        role,
    )
}

/// Single-point clean-drag projection for the engine-out assessor. Both
/// scoring branches evaluate this selected CL; `(CD_at_CL, 0)` preserves
/// the drag with its actual atmospheric Reynolds number exactly.
fn departure_polar(
    drag: &CandidateDrag,
    cl: f64,
    mach: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> (f64, f64) {
    (
        drag.cd_at_atmosphere(cl, mach, altitude_m, isa_deviation_c),
        0.0,
    )
}

/// The landing field-length residual, plus the approach-speed residual when
/// a limit is configured. Both are evaluated at the design landing mass
/// (MLW), the mass the landing distance and approach speed are certified at.
#[allow(clippy::too_many_arguments)] // one named physical input per residual; a struct would only rename them once
fn landing_and_approach_residuals(
    outcome: &SizingOutcome,
    req: &DesignRequirements,
    perf: &PerformanceConfig,
    objective: &ObjectiveConfig,
    arrival: &Airport,
    s_ref: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let sized = &outcome.sized;
    let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
    let ws_land_limit_pa = ws_landing_limit(arrival.lda_m, sigma, perf.cl_max_land, perf.k_land);
    let landing_ws_pa = sized.design_landing_mass_kg * req.gravity_m_s2 / s_ref;

    let mut residuals = vec![ConstraintResidual::scaled(
        "landing_field",
        Performance,
        landing_ws_pa,
        ws_land_limit_pa,
        "Pa",
        landing_ws_pa - ws_land_limit_pa,
        role,
    )];

    if objective.max_approach_speed_kt > 0.0 {
        let v_speeds = compute_v_speeds_at_masses(
            sized.design_gross_mass_kg,
            sized.design_landing_mass_kg,
            s_ref,
            arrival,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let v_app_kt = v_speeds.v_app_ms / KNOT;
        residuals.push(ConstraintResidual::scaled(
            "approach_speed",
            Performance,
            v_app_kt,
            objective.max_approach_speed_kt,
            "kt",
            v_app_kt - objective.max_approach_speed_kt,
            role,
        ));
    }
    residuals
}

#[cfg(test)]
mod tests;
