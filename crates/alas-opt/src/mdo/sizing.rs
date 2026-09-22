// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building, trimming and sizing one candidate: the disciplines that do not
//! depend on the takeoff mass run once, then `mdo::mda` closes the coupled
//! mass, centre-of-gravity, trim and mission-fuel fixed point.

use alas_config::{airport_dataset, airports, AlasConfig, MtowSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_payload::oew::oew_and_cg;

use super::build::first_mass_pass;
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
///
/// This is deliberately **not** the seam for a station-placement failure. A
/// candidate whose main-gear station cannot be placed never reaches here: the
/// mass analysis it passes through first
/// (`build::mass_analysis_with_structural_feedback`) already classifies that
/// cause as `main_gear_station_not_measured` and propagates it with `?`, so
/// a search log can separate a missing gear datum from a degenerate geometry.
/// `a_missing_main_gear_datum_survives_the_mdo_sizing_entry_point` pins that.
/// What is bucketed under `mass_coordinates` here is the engine binding
/// beside the mass analysis, for the reason stated at each call site.
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
    /// Geometric aircraft-body angle at the cruise trim used by the mission
    /// model, before the presentation-only compressibility correction.
    pub geometric_body_alpha_deg: f64,
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
/// solve fails: the candidate is not a physically evaluable aircraft.
#[cfg(test)]
pub(crate) fn run_candidate(
    config: &AlasConfig,
    x: &[f64],
) -> Result<SizingOutcome, CandidateFailure> {
    run_candidate_with_polar_and_fuselage_policy(config, x, None, false)
}

/// Run a candidate while preserving a caller-pinned clean-sheet fuselage
/// coordinate.  This is used by fixed desktop/reference reviews; ordinary
/// product optimization keeps the cabin-derived sizing behavior.
pub(crate) fn run_candidate_with_fuselage_policy(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<SizingOutcome, CandidateFailure> {
    run_candidate_with_polar_and_fuselage_policy(config, x, None, preserve_explicit_fuselage_length)
}

/// [`run_candidate_with_fuselage_policy`] with an optional externally
/// supplied cruise polar, which replaces the native trim and is held fixed
/// through the sizing loop.
pub(crate) fn run_candidate_with_polar_and_fuselage_policy(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
) -> Result<SizingOutcome, CandidateFailure> {
    let (candidate_config, dv, mut plane) = super::build::build_geometry_with_fuselage_policy(
        config,
        x,
        preserve_explicit_fuselage_length,
    )?;
    // The ceiling-mass cabin layout is consumed inside `first_mass_pass`;
    // the closure re-places the cabin at every closed mass itself.
    let (masses0, coords0, cg0, _summary, capacity, structural_reference) =
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
    // The altitude the configured route is actually flown at, resolved by the
    // same rule the published mission uses
    // (`alas_mission::route_cruise_altitude_m`). `req.cruise_altitude_m` is
    // the *sizing* cruise altitude - the design point the wing, the engine
    // deck and the drag polar are built at - and it stays that everywhere
    // else in this function, including the propulsion deck's reference point
    // above. It is not the flight level a dispatcher files for a short
    // declared sector, and using it as one is what made the A320-200 and the
    // A220-300 reject every candidate on `mission_profile_range`: the
    // climb-cruise-descent ladder to 11 278 m needs 743 km of still air and
    // the declared LEMD-LEPA sector is 546 km. The published mission was
    // corrected to fly the preset's own declared operational altitude; this
    // is the same correction on the optimizer's side, so the two models size
    // and fly one mission instead of two.
    let flown_cruise_altitude_m = match (departure, arrival) {
        (Some(origin), Some(destination)) => {
            alas_mission::route_cruise_altitude_m(&candidate_config, origin, destination)
        }
        _ => req.cruise_altitude_m,
    };
    let model = SegmentMissionModel::new(
        candidate_config.mission.profile.clone(),
        req.cruise_mach,
        flown_cruise_altitude_m,
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
    // The route has to clear the ladder the aircraft can actually fly, which
    // is the one at the lowest cruise level the geometry admits; the planner
    // lowers the level until it fits. See
    // `SegmentMissionModel::minimum_flyable_profile_range_m`.
    let minimum_profile_range_m = model.minimum_flyable_profile_range_m();

    let tank_capacity = tank_capacity_kg(&candidate_config, &plane, &dv);

    let context = MdaContext {
        config: &candidate_config,
        dv: &dv,
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
    // A fixed-requirement run evaluates the aircraft at its declared MTOW;
    // the dispatch solution is the mission's required takeoff mass and is
    // compared against that ceiling by the mass residuals.  Keeping those
    // quantities separate prevents geometry/thrust/CG checks from using the
    // lower mission-required mass while the component ledger is closed at
    // the declared MTOW.  Both mission-sized modes (`SizedByMission`,
    // bounded above by the declared MTOW, and `Unconstrained`, which only
    // seeds its first pass from it) use the converged dispatch mass for
    // both purposes instead, since there each candidate's own closure, not
    // the declared requirement, is what the analysis mass answers to.
    let analysis_takeoff_mass_kg =
        if candidate_config.optimizer.objective.mtow_sizing == MtowSizing::FixedRequirement {
            mtow_ceiling
        } else {
            closure.dispatch.takeoff_mass_kg
        };
    // The design-weight basis the ledger was closed on, made explicit so a
    // report can say whether the components belong to the declared aircraft
    // or to the sized one (`alas_config::MassSizingBasis`).
    let basis = candidate_config.mass_sizing_basis();
    let (design_gross_mass_kg, design_landing_mass_kg) = match basis {
        alas_config::MassSizingBasis::FixedAircraft {
            design_gross_mass_kg,
            design_landing_mass_kg,
        } => (design_gross_mass_kg, design_landing_mass_kg),
        alas_config::MassSizingBasis::Coupled => {
            // The component ledger is closed on this candidate's own
            // dispatched mass, which is what "coupled" means and is left
            // alone. The *landing* limit is a different quantity and must
            // not follow it.
            //
            // A maximum landing mass is a structural design weight: a
            // fraction of the design gross weight the airframe and gear are
            // built for. Referring it to the mass this particular sector
            // happens to close at makes the `landing_mass` residual say
            // "burn at least (1 - mlw_fraction) of your own take-off mass on
            // this flight", which is a statement about the mission with no
            // aircraft property in it, and it is unsatisfiable by
            // construction on a short sector: measured on the shipped
            // clean-sheet path it rejected 434 of 462 A320-200 candidates,
            // 438 of 460 A220-300 and 603 of 605 A340-300.
            //
            // Under `MtowSizing::SizedByMission` - the product default - the
            // closure is explicitly bounded above by the declared MTOW, so
            // that declared mass *is* the design gross weight the structure
            // must support and the closure is only this mission's dispatch.
            // The limit is therefore taken against the ceiling there.
            // `Unconstrained` declares no ceiling at all (see the
            // `mtow_ceiling` residual above), so it keeps the closed mass,
            // which is the only design weight that mode has.
            let limit_basis_kg = if candidate_config.optimizer.objective.mtow_sizing
                == MtowSizing::SizedByMission
                && mtow_ceiling.is_finite()
                && mtow_ceiling > 0.0
            {
                mtow_ceiling
            } else {
                analysis_takeoff_mass_kg
            };
            (
                analysis_takeoff_mass_kg,
                candidate_config.landing_mass_limit_kg(limit_basis_kg),
            )
        }
    };
    let sized = SizedCandidate {
        takeoff_mass_kg: analysis_takeoff_mass_kg,
        sizing_basis: basis.as_str(),
        design_gross_mass_kg,
        design_landing_mass_kg,
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
        geometric_body_alpha_deg: polar.geometric_body_alpha_deg,
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

    #[test]
    fn a_missing_main_gear_datum_survives_the_mdo_sizing_entry_point() {
        // The MDA-sizing entry point must not relabel a refused main-gear
        // station as a generic coordinate failure on its way out: the two
        // lead to opposite actions, and only this reason tells a reviewer to
        // register the aircraft's published gear stations.
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("ATR configuration: {error}"));
        // This test owns an explicitly unmeasured fixture; the registered
        // ATR preset itself now has its published gear anchors.
        config.landing_gear.reference_station_fuselage_length_m = None;
        config.landing_gear.reference_nlg_x_fraction = None;
        config.landing_gear.reference_mlg_x_fractions = None;
        let preset = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("registered ATR preset: {error}"));
        let failure = run_candidate(&config, &preset.design_vector.to_array())
            .err()
            .unwrap_or_else(|| panic!("an ATR-like candidate has no main-gear station to size"));
        assert_eq!(failure.reason, "main_gear_station_not_measured");
    }

    #[test]
    fn a_resolvable_candidate_is_not_labelled_a_missing_gear_datum() {
        // The clean-sheet default keeps the wing-mounted fallback and must
        // still size, so the gate cannot be what stops an in-domain layout.
        let config = AlasConfig::default();
        let x = DesignVector::default().to_array();
        assert!(run_candidate(&config, &x).is_ok());
    }
}
