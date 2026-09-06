// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closing the takeoff-mass fixed point analytically, once the candidate's
//! trimmed drag polar is known.
//!
//! `mdo::build` evaluates the geometry, mass and trim passes that do not
//! depend on the takeoff mass. What is left -- the operating empty mass at a
//! given takeoff mass, and the fuel the mission needs at that empty mass --
//! is a fixed point in the takeoff mass itself, closed here by repeatedly
//! re-running the (cheap) mass analysis and [`solve_dispatch`] rather than
//! the aerodynamic solve.

use alas_atmo::Atmosphere;
use alas_config::{airports, AlasConfig, MtowSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassBreakdown, MassCoordinateModel,
    MassCoordinates, PayloadLayoutSummary,
};
use alas_mass::breguet::{BreguetFuelModel, SegmentFractions};
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchSolution, DispatchStatus};
use alas_payload::oew::oew_and_cg;
use alas_units::FOOT;

use super::build::{build_geometry, first_mass_pass, trim_and_polar};
use super::engine::{engine_terms, static_thrust_kn_per_engine};
use super::range::mission_range_m;
use super::tanks::tank_capacity_kg;
use super::types::{CandidateFailure, HistoryFields, SizedCandidate};

/// Horizontal distance credited to climb and descent against the cruise
/// Breguet leg: the same representative narrowbody value
/// `alas_pipeline::fuel_model::breguet_from_report` assumes. The native
/// mission flies its own climb and descent; this only shapes the analytic
/// estimate the sizing loop closes against.
const CLIMB_DESCENT_RANGE_CREDIT_M: f64 = 250_000.0;

/// A failure with the `mass_coordinates` reason, for the sizing loop's own
/// re-runs of the mass analysis and the engine-binding lookups beside it.
fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
}

/// Everything a mission-sized residual table is computed from, beyond the
/// scalar summary in [`SizedCandidate`].
pub(crate) struct SizingOutcome {
    pub plane: Airplane,
    pub masses: MassBreakdown,
    pub coords: MassCoordinates,
    pub cg_x: f64,
    pub x_np: f64,
    pub mac: f64,
    pub cd0: f64,
    pub induced_factor_k: f64,
    pub n_engines: i64,
    pub static_thrust_kn: f64,
    pub departure: Option<&'static airports::Airport>,
    pub arrival: Option<&'static airports::Airport>,
    pub mtow_ceiling: f64,
    pub sized: SizedCandidate,
    pub history: HistoryFields,
}

/// Build, size and trim one candidate design vector.
///
/// # Errors
///
/// [`CandidateFailure`] when the geometry, mass, payload layout or trim
/// solve fails -- the candidate is not a physically evaluable aircraft.
pub(crate) fn run_candidate(
    config: &AlasConfig,
    x: &[f64],
) -> Result<SizingOutcome, CandidateFailure> {
    let (candidate_config, dv, mut plane) = build_geometry(config, x)?;
    let (masses0, coords0, cg0, summary) = first_mass_pass(&candidate_config, &plane)?;
    let polar = trim_and_polar(&candidate_config, &mut plane, cg0[0], &dv)?;

    let req = &candidate_config.requirements;
    let mtow_ceiling = req.mtow_kg;
    let n_engines = candidate_config
        .geometry
        .engine
        .spanwise_positions_m
        .len()
        .max(1);
    let n_engines_f64 = n_engines as f64;

    let cruise_atmo = Atmosphere::new(req.cruise_altitude_m);
    let cruise_tas_m_s = req.cruise_mach * cruise_atmo.speed_of_sound();

    // Propulsion mass already required this same engine binding to resolve
    // successfully inside `first_mass_pass`, so a failure reaching here is
    // bucketed with the mass-coordinate failures it would otherwise cause.
    let terms = engine_terms(
        &candidate_config.geometry.engine,
        cruise_tas_m_s,
        n_engines_f64,
    )
    .map_err(|_| mass_coordinates_failure())?;
    let static_thrust_kn = static_thrust_kn_per_engine(&candidate_config.geometry.engine)
        .map_err(|_| mass_coordinates_failure())?;

    let departure = airports::get(&candidate_config.departure_airport).ok();
    let arrival = airports::get(&candidate_config.arrival_airport).ok();
    let range_m = mission_range_m(
        candidate_config.optimizer.objective.design_range_nmi,
        departure,
        arrival,
    );

    let holding_altitude_m = arrival.map_or(0.0, |airport| airport.elevation_m)
        + candidate_config.fuel_policy.holding_altitude_ft * FOOT;
    let holding_atmo = Atmosphere::new(holding_altitude_m);

    let model = BreguetFuelModel {
        cruise_tas_m_s,
        cruise_density_kg_m3: cruise_atmo.density(),
        holding_density_kg_m3: holding_atmo.density(),
        wing_area_m2: plane.s_ref,
        cd0: polar.cd0,
        induced_factor_k: polar.induced_factor_k,
        tsfc_cruise_kg_per_n_s: terms.tsfc_cruise_kg_per_n_s,
        holding_tsfc_factor: BreguetFuelModel::DEFAULT_HOLDING_TSFC_FACTOR,
        takeoff_fuel_flow_kg_s: terms.takeoff_fuel_flow_kg_s,
        idle_fuel_flow_fraction: BreguetFuelModel::DEFAULT_IDLE_FUEL_FLOW_FRACTION,
        gravity_m_s2: req.gravity_m_s2,
        segment_fractions: SegmentFractions::default(),
        climb_descent_range_credit_m: CLIMB_DESCENT_RANGE_CREDIT_M,
    };
    // The model is built from the candidate's own trimmed cruise point
    // rather than from an `alas-pipeline` report; an invalid polar or engine
    // binding here reflects the same aerodynamic operating point
    // `trim_and_polar` just evaluated, so it is bucketed with `trim_solve`.
    model.validate().map_err(|_| CandidateFailure {
        reason: "trim_solve",
    })?;

    let tank_capacity = tank_capacity_kg(&candidate_config, &plane, &dv);

    let closure = size_takeoff_mass(SizingLoopInputs {
        config: &candidate_config,
        plane: &plane,
        masses0,
        coords0,
        cg0,
        summary: &summary,
        model: &model,
        range_m,
        tank_capacity_kg: tank_capacity,
    })?;

    let (operating_empty_mass_kg, _) = oew_and_cg(&closure.masses, &closure.coords);
    let sized = SizedCandidate {
        takeoff_mass_kg: closure.dispatch.takeoff_mass_kg,
        operating_empty_mass_kg,
        zero_fuel_mass_kg: closure.dispatch.zero_fuel_mass_kg,
        payload_kg: closure.masses.payload,
        block_fuel_kg: closure.dispatch.plan.block_fuel_kg(),
        takeoff_fuel_kg: closure.dispatch.plan.takeoff_fuel_kg(),
        ramp_fuel_kg: closure.dispatch.plan.ramp_fuel_kg(),
        usable_capacity_kg: tank_capacity.unwrap_or(f64::NAN),
        design_range_m: range_m,
        lift_to_drag: polar.lift_to_drag,
        dispatch: closure.dispatch,
        sizing_iterations: closure.sizing_iterations,
        sizing_closed: closure.sizing_closed,
    };
    let history = HistoryFields {
        dv,
        span_m: dv.span_m,
        alpha_deg: polar.alpha_deg,
        area_m2: plane.s_ref,
        trim_ih_deg: polar.incidence_deg,
    };
    let mac = plane.c_ref;
    Ok(SizingOutcome {
        plane,
        masses: closure.masses,
        coords: closure.coords,
        cg_x: closure.cg[0],
        x_np: polar.x_np,
        mac,
        cd0: polar.cd0,
        induced_factor_k: polar.induced_factor_k,
        n_engines: n_engines as i64,
        static_thrust_kn,
        departure,
        arrival,
        mtow_ceiling,
        sized,
        history,
    })
}

/// Grouped inputs to [`size_takeoff_mass`], so the closure loop itself reads
/// as one named bundle rather than nine positional parameters.
struct SizingLoopInputs<'a> {
    config: &'a AlasConfig,
    plane: &'a Airplane,
    masses0: MassBreakdown,
    coords0: MassCoordinates,
    cg0: [f64; 3],
    summary: &'a PayloadLayoutSummary,
    model: &'a BreguetFuelModel,
    range_m: f64,
    tank_capacity_kg: Option<f64>,
}

/// The result of closing the outer takeoff-mass fixed point.
struct SizingClosure {
    dispatch: DispatchSolution,
    masses: MassBreakdown,
    coords: MassCoordinates,
    cg: [f64; 3],
    sizing_iterations: usize,
    sizing_closed: bool,
}

/// Close the outer takeoff-mass fixed point: `MtowSizing::FixedRequirement`
/// runs a single pass at the ceiling; `MtowSizing::SizedByMission` iterates
/// `TOW_{k+1} = dispatch.takeoff_mass_kg` until the change falls below the
/// configured tolerance or the iteration budget is spent.
fn size_takeoff_mass(inputs: SizingLoopInputs<'_>) -> Result<SizingClosure, CandidateFailure> {
    let SizingLoopInputs {
        config,
        plane,
        masses0,
        coords0,
        cg0,
        summary,
        model,
        range_m,
        tank_capacity_kg,
    } = inputs;
    let objective = &config.optimizer.objective;
    let mtow_ceiling = config.requirements.mtow_kg;
    let mlw_kg = mtow_ceiling * config.mass_model.mlw_fraction_mtow;
    let sized_by_mission = objective.mtow_sizing == MtowSizing::SizedByMission;
    let inner_max_iterations = objective.sizing_max_iterations.max(1) as usize;
    let max_passes = if sized_by_mission {
        inner_max_iterations
    } else {
        1
    };

    let mut tow_k = mtow_ceiling;
    let mut masses_k = masses0;
    let mut coords_k = coords0;
    let mut cg_k = cg0;
    let mut sizing_iterations = 0usize;
    let mut outcome: Option<(DispatchSolution, bool)> = None;

    for pass in 0..max_passes {
        sizing_iterations = pass + 1;
        if pass > 0 {
            let mut pass_requirements = config.requirements.clone();
            pass_requirements.mtow_kg = tow_k;
            let pass_result = run_mass_analysis_with_model_checked_product_with_gear(
                plane,
                &pass_requirements,
                &config.geometry,
                &config.cabin,
                &config.control_surfaces,
                Some(&config.mass_model),
                Some(summary),
                MassCoordinateModel::StructuralWingbox(&config.structures),
                &config.landing_gear,
            );
            let (m, c, cg) = pass_result.map_err(|_| mass_coordinates_failure())?;
            masses_k = m;
            coords_k = c;
            cg_k = cg;
        }
        let (oew_k, _) = oew_and_cg(&masses_k, &coords_k);
        let zero_fuel_mass_kg = oew_k + masses_k.payload;
        let limits = DispatchLimits {
            mtow_kg: mtow_ceiling,
            mzfw_kg: None,
            mlw_kg: Some(mlw_kg),
            usable_capacity_kg: tank_capacity_kg,
        };
        let solution = solve_dispatch(
            zero_fuel_mass_kg,
            range_m,
            &config.fuel_policy,
            model,
            &limits,
            inner_max_iterations,
            objective.sizing_tolerance_kg,
        );

        if sized_by_mission {
            let next_tow_kg = solution.takeoff_mass_kg;
            let delta_kg = (next_tow_kg - tow_k).abs();
            let done = delta_kg < objective.sizing_tolerance_kg;
            let closed = done && !is_model_failed(&solution.status);
            tow_k = next_tow_kg;
            outcome = Some((solution, closed));
            if done {
                break;
            }
        } else {
            let closed = !is_model_failed(&solution.status);
            outcome = Some((solution, closed));
            break;
        }
    }

    let Some((dispatch, sizing_closed)) = outcome else {
        // `max_passes` is always at least one, so the loop above always runs
        // and always assigns `outcome`. This arm only keeps the function
        // panic-free rather than describing a reachable state.
        return Err(mass_coordinates_failure());
    };
    Ok(SizingClosure {
        dispatch,
        masses: masses_k,
        coords: coords_k,
        cg: cg_k,
        sizing_iterations,
        sizing_closed,
    })
}

fn is_model_failed(status: &DispatchStatus) -> bool {
    matches!(status, DispatchStatus::ModelFailed(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::design_variables::DesignVector;

    #[test]
    fn the_default_design_vector_sizes_to_a_finite_positive_takeoff_mass() {
        let config = AlasConfig::default();
        let x = DesignVector::default().to_array();
        let outcome =
            run_candidate(&config, &x).unwrap_or_else(|failure| panic!("{}", failure.reason));
        assert!(outcome.sized.takeoff_mass_kg.is_finite());
        assert!(outcome.sized.takeoff_mass_kg > 0.0);
        assert!(outcome.sized.lift_to_drag.is_finite() && outcome.sized.lift_to_drag > 0.0);
    }
}
