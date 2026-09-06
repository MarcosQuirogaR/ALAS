// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building, trimming and sizing one candidate: the disciplines that do not
//! depend on the takeoff mass run once, then `mdo::mda` closes the coupled
//! mass, centre-of-gravity, trim and mission-fuel fixed point.

use alas_atmo::Atmosphere;
use alas_config::{airports, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::breguet::{BreguetFuelModel, SegmentFractions};
use alas_payload::oew::oew_and_cg;
use alas_units::FOOT;

use super::build::{build_geometry, first_mass_pass, trim_and_polar, TrimmedPolar};
use super::engine::{engine_terms, static_thrust_kn_per_engine};
use super::mda::{converge, MdaContext, MdaState};
use super::range::mission_range_m;
use super::tanks::tank_capacity_kg;
use super::types::{CandidateFailure, ExternalPolar, HistoryFields, SizedCandidate};

/// Horizontal distance credited to climb and descent against the cruise
/// Breguet leg: the same representative narrowbody value
/// `alas_pipeline::fuel_model::breguet_from_report` assumes. The native
/// mission flies its own climb and descent; this only shapes the analytic
/// estimate the sizing loop closes against.
const CLIMB_DESCENT_RANGE_CREDIT_M: f64 = 250_000.0;

/// A failure with the `mass_coordinates` reason, for the engine-binding
/// lookups beside the mass analysis.
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
    run_candidate_with_polar(config, x, None)
}

/// [`run_candidate`] with an optional externally supplied cruise polar,
/// which replaces the native trim and is held fixed through the sizing loop.
pub(crate) fn run_candidate_with_polar(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&ExternalPolar>,
) -> Result<SizingOutcome, CandidateFailure> {
    let (candidate_config, dv, mut plane) = build_geometry(config, x)?;
    let (masses0, coords0, cg0, summary) = first_mass_pass(&candidate_config, &plane)?;

    let req = &candidate_config.requirements;
    let mtow_ceiling = req.mtow_kg;
    let polar0 = match external {
        Some(polar) if polar.is_valid() => TrimmedPolar {
            cd0: polar.cd0,
            induced_factor_k: polar.induced_factor_k,
            lift_to_drag: polar.lift_to_drag,
            alpha_deg: polar.alpha_deg,
            incidence_deg: polar.incidence_deg,
            x_np: polar.x_np,
        },
        Some(_) => {
            return Err(CandidateFailure {
                reason: "trim_solve",
            })
        }
        None => trim_and_polar(&candidate_config, &mut plane, cg0[0], &dv, mtow_ceiling)?,
    };
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
        cd0: polar0.cd0,
        induced_factor_k: polar0.induced_factor_k,
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

    let context = MdaContext {
        config: &candidate_config,
        dv: &dv,
        summary: &summary,
        model,
        range_m,
        tank_capacity_kg: tank_capacity,
        retrim_allowed: external.is_none(),
    };
    let closure = converge(
        &context,
        &mut plane,
        MdaState {
            masses: masses0,
            coords: coords0,
            cg: cg0,
            polar: polar0,
        },
    )?;
    let state = closure.state;
    let polar = state.polar;

    let (operating_empty_mass_kg, _) = oew_and_cg(&state.masses, &state.coords);
    let sized = SizedCandidate {
        takeoff_mass_kg: closure.dispatch.takeoff_mass_kg,
        operating_empty_mass_kg,
        zero_fuel_mass_kg: closure.dispatch.zero_fuel_mass_kg,
        payload_kg: state.masses.payload,
        block_fuel_kg: closure.dispatch.plan.block_fuel_kg(),
        takeoff_fuel_kg: closure.dispatch.plan.takeoff_fuel_kg(),
        ramp_fuel_kg: closure.dispatch.plan.ramp_fuel_kg(),
        usable_capacity_kg: tank_capacity.unwrap_or(f64::NAN),
        design_range_m: range_m,
        lift_to_drag: polar.lift_to_drag,
        dispatch: closure.dispatch,
        sizing_iterations: closure.sizing_iterations,
        sizing_closed: closure.sizing_closed,
        retrim_count: closure.retrim_count,
        cg_shift_pct_mac: closure.cg_shift_pct_mac,
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
        masses: state.masses,
        coords: state.coords,
        cg_x: state.cg[0],
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
        assert_eq!(outcome.sized.retrim_count, 0);
    }
}
