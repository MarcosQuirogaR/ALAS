// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airworthiness performance family: engine-out second-segment climb,
//! cruise and takeoff thrust margin, landing field length and approach speed.

use alas_config::airports::Airport;
use alas_config::{
    AlasConfig, ConstraintPolicy, DesignRequirements, ObjectiveConfig, PerformanceConfig,
};
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, far25_oei_gradient, tw_cruise_constraint,
    tw_oei_climb_constraint, tw_takeoff_constraint, ws_landing_limit,
};
use alas_units::KNOT;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// The engine-out climb, cruise and takeoff thrust-margin residuals, plus
/// (when the arrival aerodrome resolves) landing field length and approach
/// speed.
pub(super) fn performance_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let req = &config.requirements;
    let perf = &config.performance;
    let objective = &config.optimizer.objective;
    let sized = &outcome.sized;
    let available_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0
        / (sized.takeoff_mass_kg * req.gravity_m_s2);
    let s_ref = outcome.plane.s_ref.max(1e-9);

    let mut residuals = Vec::new();

    let oei_gradient = far25_oei_gradient(outcome.n_engines).unwrap_or(perf.oei_gradient);
    let required_oei_tw = tw_oei_climb_constraint(
        outcome.cd0,
        outcome.induced_factor_k,
        outcome.n_engines,
        oei_gradient,
        perf.oei_climb_cl,
        perf.oei_climb_delta_cd,
    );
    residuals.push(ConstraintResidual::scaled(
        "oei_second_segment",
        Performance,
        available_tw,
        required_oei_tw,
        "T/W",
        required_oei_tw - available_tw,
        policy,
    ));

    let cruise_ws_pa = sized.takeoff_mass_kg * req.gravity_m_s2 / s_ref;
    let required_cruise_tw = tw_cruise_constraint(
        &[cruise_ws_pa],
        outcome.cd0,
        outcome.induced_factor_k,
        req.cruise_mach,
        req.cruise_altitude_m,
        perf.thrust_lapse,
    )[0];
    residuals.push(ConstraintResidual::scaled(
        "cruise_thrust",
        Performance,
        available_tw,
        required_cruise_tw,
        "T/W",
        required_cruise_tw - available_tw,
        policy,
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
            policy,
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
            policy,
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
            policy,
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
            policy,
        ));
    }

    if let Some(departure) = outcome.departure {
        let sigma = density_ratio(departure.elevation_m, departure.isa_deviation_c);
        let required_takeoff_tw =
            tw_takeoff_constraint(&[cruise_ws_pa], departure.toda_m, sigma, perf.cl_max_to)[0];
        residuals.push(ConstraintResidual::scaled(
            "takeoff_field",
            Performance,
            available_tw,
            required_takeoff_tw,
            "T/W",
            required_takeoff_tw - available_tw,
            policy,
        ));
    }

    if let Some(arrival) = outcome.arrival {
        residuals.extend(landing_and_approach_residuals(
            outcome, req, perf, objective, arrival, s_ref, policy,
        ));
    }

    residuals
}

/// The landing field-length residual, plus the approach-speed residual when
/// a limit is configured.
#[allow(clippy::too_many_arguments)] // one named physical input per residual; a struct would only rename them once
fn landing_and_approach_residuals(
    outcome: &SizingOutcome,
    req: &DesignRequirements,
    perf: &PerformanceConfig,
    objective: &ObjectiveConfig,
    arrival: &Airport,
    s_ref: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let sized = &outcome.sized;
    let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
    let ws_land_limit_pa = ws_landing_limit(arrival.lda_m, sigma, perf.cl_max_land, perf.k_land);
    let landing_ws_pa = sized.dispatch.destination_landing_mass_kg * req.gravity_m_s2 / s_ref;

    let mut residuals = vec![ConstraintResidual::scaled(
        "landing_field",
        Performance,
        landing_ws_pa,
        ws_land_limit_pa,
        "Pa",
        landing_ws_pa - ws_land_limit_pa,
        policy,
    )];

    if objective.max_approach_speed_kt > 0.0 {
        let v_speeds = compute_v_speeds_at_masses(
            sized.takeoff_mass_kg,
            sized.dispatch.destination_landing_mass_kg,
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
            policy,
        ));
    }
    residuals
}
