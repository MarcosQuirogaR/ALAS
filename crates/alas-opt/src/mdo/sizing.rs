// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building, trimming and sizing one candidate: the disciplines that do not
//! depend on the takeoff mass run once, then `mdo::mda` closes the coupled
//! mass, centre-of-gravity, trim and mission-fuel fixed point.

use alas_config::{airport_dataset, airports, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_payload::oew::oew_and_cg;

use super::build::{build_geometry, first_mass_pass};
use super::engine::static_thrust_kn_per_engine;
use super::mda::{converge, MdaContext, MdaState};
use super::mission_model::{PhaseAeroLimits, SegmentMissionModel};
use super::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use super::range::mission_range_from_coordinates;
use super::tanks::tank_capacity_kg;
use super::trim::{trim_and_polar, TrimmedPolar};
use super::types::{
    CandidateFailure, ExternalPolar, HistoryFields, PayloadCapacity, PolarConditionTolerance,
    SizedCandidate,
};

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
    /// Source-resolved records used for routing/elevation. A record may be
    /// present while its runway values remain physical-only and therefore
    /// unusable by field-performance constraints.
    #[expect(
        dead_code,
        reason = "retained for finalist airport provenance reporting"
    )]
    pub departure_record: Option<airport_dataset::ProvenancedAirport>,
    #[expect(
        dead_code,
        reason = "retained for finalist airport provenance reporting"
    )]
    pub arrival_record: Option<airport_dataset::ProvenancedAirport>,
    /// Whether both configured aerodrome identifiers resolved to records.
    pub airport_records_resolved: bool,
    /// Whether both records carry declared operational runway distances.
    pub declared_airport_data_complete: bool,
    /// Whether the mission distance was explicit or could be computed from
    /// two finite source-resolved coordinates.
    pub mission_distance_known: bool,
    /// Minimum still-air distance for the configured climb/descent profile.
    pub minimum_profile_range_m: f64,
    pub mtow_ceiling: f64,
    pub sized: SizedCandidate,
    pub history: HistoryFields,
    #[expect(dead_code, reason = "retained for finalist load-case reporting")]
    pub capacity: PayloadCapacity,
    /// Whether the wing total represents a complete primary plus secondary
    /// inventory. Clean-sheet movable correlations remain partial.
    pub structural_inventory_complete: bool,
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
    let (masses0, coords0, cg0, summary, capacity, structural_reference) =
        first_mass_pass(&candidate_config, &dv, &plane)?;

    let req = &candidate_config.requirements;
    let mtow_ceiling = req.mtow_kg;
    let polar0 = match external {
        // An external polar must describe this candidate at this cruise
        // point: a polar evaluated at another Mach, altitude or reference
        // area is a mismatched analysis, not a candidate.
        Some(polar)
            if polar.is_valid()
                && polar
                    .matches_condition(
                        req.cruise_mach,
                        req.cruise_altitude_m,
                        plane.s_ref,
                        PolarConditionTolerance::default(),
                    )
                    .is_ok() =>
        {
            TrimmedPolar::from_external(polar)
        }
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

    // Propulsion mass already required this same engine binding to resolve
    // successfully inside `first_mass_pass`, so a failure reaching here is
    // bucketed with the mass-coordinate failures it would otherwise cause.
    let propulsion = PropulsionDeck::from_engine(
        &candidate_config.geometry.engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_rate_ft_min(candidate_config.mission.profile.initial_climb_rate_m_s),
    )
    .map_err(|_| mass_coordinates_failure())?;
    let static_thrust_kn = static_thrust_kn_per_engine(&candidate_config.geometry.engine)
        .map_err(|_| mass_coordinates_failure())?;

    let departure_record = airport_dataset::resolve(&candidate_config.departure_airport).ok();
    let arrival_record = airport_dataset::resolve(&candidate_config.arrival_airport).ok();
    let departure = airports::get(&candidate_config.departure_airport).ok();
    let arrival = airports::get(&candidate_config.arrival_airport).ok();
    let explicit_range = candidate_config.optimizer.objective.design_range_nmi > 0.0;
    let departure_coordinates = departure_record
        .as_ref()
        .and_then(|airport| Some((airport.latitude_deg.value?, airport.longitude_deg.value?)));
    let arrival_coordinates = arrival_record
        .as_ref()
        .and_then(|airport| Some((airport.latitude_deg.value?, airport.longitude_deg.value?)));
    let coordinate_range = departure_coordinates.is_some() && arrival_coordinates.is_some();
    let mission_distance_known = explicit_range || coordinate_range;
    let range_m = mission_range_from_coordinates(
        candidate_config.optimizer.objective.design_range_nmi,
        departure_coordinates,
        arrival_coordinates,
    );

    let departure_elevation_m = departure_record
        .as_ref()
        .and_then(|airport| airport.elevation_m.value)
        .or_else(|| departure.map(|airport| airport.elevation_m))
        .unwrap_or(0.0);
    let arrival_elevation_m = arrival_record
        .as_ref()
        .and_then(|airport| airport.elevation_m.value)
        .or_else(|| arrival.map(|airport| airport.elevation_m))
        .unwrap_or(0.0);
    let holding_altitude_m =
        arrival_elevation_m + candidate_config.fuel_policy.holding_altitude_ft * alas_units::FOOT;
    let model = SegmentMissionModel::new(
        candidate_config.mission.profile.clone(),
        req.cruise_mach,
        req.cruise_altitude_m,
        departure_elevation_m,
        arrival_elevation_m,
        plane.s_ref,
        polar0.cd0,
        polar0.induced_factor_k,
        polar0.wave_drag_cd,
        req.gravity_m_s2,
        holding_altitude_m,
        PhaseAeroLimits::from_config(&candidate_config),
        propulsion,
    )
    .map_err(|_| CandidateFailure {
        reason: "trim_solve",
    })?
    // The same departure ISA deviation the native mission applies to every
    // segment, so both paths fly one ambient convention.
    .with_isa_deviation_c(
        departure
            .map(|airport| airport.isa_deviation_c)
            .unwrap_or(0.0),
    );
    // The model is built from the candidate's own trimmed cruise point
    // rather than from an `alas-pipeline` report; an invalid polar or engine
    // binding here reflects the same aerodynamic operating point
    // `trim_and_polar` just evaluated, so it is bucketed with `trim_solve`.
    model.validate().map_err(|_| CandidateFailure {
        reason: "trim_solve",
    })?;
    let minimum_profile_range_m = model.minimum_profile_range_m();

    let tank_capacity = tank_capacity_kg(&candidate_config, &plane, &dv);

    let context = MdaContext {
        config: &candidate_config,
        dv: &dv,
        summary: &summary,
        model,
        range_m,
        tank_capacity_kg: tank_capacity,
        retrim_allowed: external.is_none(),
        structural_reference: structural_reference.reference,
        structural_feedback: structural_reference.feedback,
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
        passenger_capacity: capacity.passenger_capacity,
        carried_passengers: capacity
            .carried_passengers
            .min(candidate_config.requirements.num_passengers.max(0)),
        cargo_capacity_kg: capacity.cargo_capacity_kg,
        carried_cargo_payload_kg: capacity.carried_cargo_payload_kg,
        block_fuel_kg: closure.dispatch.plan.block_fuel_kg(),
        takeoff_fuel_kg: closure.dispatch.plan.takeoff_fuel_kg(),
        ramp_fuel_kg: closure.dispatch.plan.ramp_fuel_kg(),
        usable_capacity_kg: tank_capacity.unwrap_or(f64::NAN),
        design_range_m: range_m,
        mission_distance_known,
        airport_records_resolved: departure_record.is_some() && arrival_record.is_some(),
        declared_airport_data_complete: departure_record
            .as_ref()
            .is_some_and(airport_dataset::ProvenancedAirport::is_complete_for_declared_performance)
            && arrival_record.as_ref().is_some_and(
                airport_dataset::ProvenancedAirport::is_complete_for_declared_performance,
            ),
        minimum_profile_range_m,
        lift_to_drag: polar.lift_to_drag,
        dispatch: closure.dispatch,
        sizing_iterations: closure.sizing_iterations,
        sizing_closed: closure.sizing_closed,
        retrim_count: closure.retrim_count,
        cg_shift_pct_mac: closure.cg_shift_pct_mac,
        structural_inventory_complete: structural_reference.inventory_complete,
        structural_primary_mass_kg: closure.structural_feedback.primary_mass_kg,
        structural_secondary_mass_kg: closure.structural_feedback.secondary_mass_kg,
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
        declared_airport_data_complete: departure_record
            .as_ref()
            .is_some_and(airport_dataset::ProvenancedAirport::is_complete_for_declared_performance)
            && arrival_record.as_ref().is_some_and(
                airport_dataset::ProvenancedAirport::is_complete_for_declared_performance,
            ),
        airport_records_resolved: departure_record.is_some() && arrival_record.is_some(),
        departure_record,
        arrival_record,
        mission_distance_known,
        minimum_profile_range_m,
        mtow_ceiling,
        sized,
        history,
        capacity,
        structural_inventory_complete: structural_reference.inventory_complete,
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
        assert!(
            outcome.sized.retrim_count > 0,
            "mission-sized closure must refresh the polar after mass/CG updates"
        );
    }
}
