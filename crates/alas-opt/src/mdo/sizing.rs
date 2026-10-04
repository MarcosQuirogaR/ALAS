// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building, trimming and sizing one candidate: the disciplines that do not
//! depend on the takeoff mass run once, then `mdo::mda` closes the coupled
//! mass, centre-of-gravity, trim and mission-fuel fixed point.

use std::sync::Arc;

use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, TailSizing};
use alas_payload::oew::oew_and_cg;

pub(crate) mod candidate_model;
mod entry;
mod outcome;
pub(crate) mod planned_mission;
mod route;
use entry::mass_coordinates_failure;
#[cfg(test)]
pub(crate) use entry::run_candidate;
pub(crate) use entry::run_candidate_with_fuselage_policy;
pub(crate) use outcome::{SizingOutcome, TrimReuse};

use super::build::first_mass_pass;
use super::engine::static_thrust_kn_per_engine;
use super::mda::{converge, MdaContext, MdaState};
use super::mission_model::SizingBudget;
use super::tanks::usable_fuel_capacity;
use super::trim::{trim_and_polar_with_cache, TrimmedPolar};
use super::types::{
    CandidateFailure, CandidateFuelArtifacts, ExternalPolar, HistoryFields,
    PolarConditionTolerance, SizedCandidate, SizingControls, SizingWork,
};

/// A budget that never binds: the closure always carries one so its work
/// counters are kept (`SegmentMissionModel::work`).
const UNLIMITED_BUDGET: SizingBudget = SizingBudget {
    max_trip_flights: u32::MAX,
    max_deck_evals: u64::MAX,
    max_outer_passes: u32::MAX,
};

/// Build, size and trim one candidate, with an optional externally supplied
/// cruise polar (which replaces the native trim and is held fixed through
/// the sizing loop), deep cancellation and caller [`SizingControls`].
pub(crate) fn run_candidate_cancellable(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<crate::cancellation::EvaluationCancellation>,
    controls: SizingControls,
) -> Result<SizingOutcome, CandidateFailure> {
    run_candidate_reusing(
        config,
        x,
        external,
        preserve_explicit_fuselage_length,
        cancellation,
        controls,
        None,
    )
}

/// [`run_candidate_cancellable`], starting from `reuse`'s trim when it
/// stands for this candidate ([`TrimReuse`]) and from its lattice cache in
/// any case.
pub(crate) fn run_candidate_reusing(
    config: &AlasConfig,
    x: &[f64],
    external: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<crate::cancellation::EvaluationCancellation>,
    controls: SizingControls,
    reuse: Option<&TrimReuse>,
) -> Result<SizingOutcome, CandidateFailure> {
    if cancellation.as_ref().is_some_and(|token| token.requested()) {
        return Err(CandidateFailure {
            reason: "cancelled",
        });
    }
    let (candidate_config, dv, mut plane) = super::build::build_geometry_with_fuselage_policy(
        config,
        x,
        preserve_explicit_fuselage_length,
    )?;
    // The seed-mass cabin layout is consumed inside `first_mass_pass`; the
    // closure re-places the cabin at every closed mass itself. The two
    // design modes evaluate the first pass at their seed with the structure
    // designed there; every other mode's seed is the declared requirement.
    let mut plan = candidate_config.mtow_plan();
    // A warm start replaces the seed of an iterating plan only, inside its
    // dispatch ceiling; a fixed requirement is evaluated at the requirement.
    if let Some(warm_kg) = controls
        .initial_takeoff_mass_kg
        .filter(|kg| plan.iterates && kg.is_finite() && *kg > 0.0)
    {
        plan.seed_kg = warm_kg.min(plan.dispatch_ceiling_kg);
    }
    let seeded = plan
        .requires_mission_sized_evaluation()
        .then(|| candidate_config.at_sized_closure_mass(plan.seed_kg));
    let (mut masses0, mut coords0, mut cg0, _summary, capacity, structural_reference) =
        first_mass_pass(seeded.as_ref().unwrap_or(&candidate_config), &dv, &plane)?;

    // The structural pass stays at the declared design weights. Only the
    // usable fuel and its physical moment follow the loadable takeoff case.
    let takeoff_loading = if plan.mode == alas_config::MtowSizing::FixedRequirement {
        let (loading, cg) = alas_mass::loading::apply_mtow_fuel_loading(
            &candidate_config,
            &dv,
            &plane,
            &mut masses0,
            &mut coords0,
        )
        .map_err(|_| CandidateFailure {
            reason: "mass_coordinates",
        })?;
        cg0 = cg;
        Some(loading)
    } else {
        None
    };
    let initial_analysis_mass_kg =
        takeoff_loading.map_or(plan.seed_kg, |load| load.takeoff_mass_kg);

    let req = &candidate_config.requirements;
    let declared_mtow_kg = req.mtow_kg;
    let mtow_ceiling = plan.upper_bound_kg.unwrap_or(declared_mtow_kg);
    let untrimmed = plane.clone();
    let vlm_cache: super::trim::CandidateVlmCache = reuse
        .map(|reuse| reuse.vlm_cache.clone())
        .unwrap_or_default();
    let reused = reuse.filter(|reuse| {
        external.is_none()
            && reuse.stands_for(
                &candidate_config,
                &dv,
                &plane,
                cg0[0],
                controls.screening_drag_table,
            )
    });
    let initial_trim_cg_x_m = reused.map_or(cg0[0], |reuse| reuse.trim_cg_x_m);
    let polar0 = match (external, reused) {
        // An external polar must describe this candidate at this cruise
        // point: a polar evaluated at another Mach, altitude or reference
        // area is a mismatched analysis, not a candidate.
        (Some(polar), _)
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
        (Some(_), _) => {
            return Err(CandidateFailure {
                reason: "trim_solve",
            })
        }
        (None, Some(reuse)) => {
            plane = reuse.trimmed.clone();
            reuse.polar.clone()
        }
        (None, None) => trim_and_polar_with_cache(
            &candidate_config,
            &mut plane,
            cg0[0],
            &dv,
            initial_analysis_mass_kg,
            &vlm_cache,
            controls.screening_drag_table,
        )?,
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
    let (propulsion, deck_key) =
        candidate_model::sizing_deck(&candidate_config).map_err(|_| mass_coordinates_failure())?;
    let static_thrust_kn = static_thrust_kn_per_engine(&candidate_config.geometry.engine)
        .map_err(|_| mass_coordinates_failure())?;

    let route = route::resolve(&candidate_config);
    let (departure, range_m, route_distance_m) =
        (route.departure, route.range_m, route.route_distance_m);
    // The same builder a consumer of the carried artifacts rebuilds this
    // model with (`candidate_model::candidate_mission_model`).
    let mut model = candidate_model::route_mission_model(
        &candidate_config,
        &route,
        plane.s_ref,
        polar0.drag.cruise_drag(),
        propulsion,
    )
    .map_err(|_| CandidateFailure {
        reason: "trim_solve",
    })?;
    if let Some(steps) = controls.steps_per_segment.filter(|&steps| steps > 0) {
        model = model.with_steps_per_segment(steps);
    }
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

    // The one capacity rule the full analysis applies too: published for an
    // unchanged preset, else the resolved layout.
    let capacity_assessment = usable_fuel_capacity(&candidate_config, &dv, &plane);
    let tank_capacity = capacity_assessment.map(|capacity| capacity.kg);

    model.cancellation = cancellation;
    let design_mission = super::mtow_modes::closure_mission(
        &plan,
        &candidate_config,
        &model,
        range_m,
        (masses0.payload, capacity.carried_passengers),
    )?;
    // The route is flown off-design by the closed aircraft whenever it is
    // not the mission the closure flew: beside a design mission, and beside
    // a great-circle sizing mission when the run planned a different route.
    // Its dispatch is the flown load the reporting verdict checks.
    let closes_on_design_mission = design_mission.is_some();
    let flies_route_off_design =
        closes_on_design_mission || (route_distance_m > 0.0 && route_distance_m != range_m);
    let route_model = flies_route_off_design.then(|| model.clone());
    let (model, range_m, dispatch_payload_kg, mission_distance_known, minimum_profile_range_m) =
        match design_mission {
            Some(mission) => {
                let minimum_m = mission.model.minimum_flyable_profile_range_m();
                let known = mission.distance_known;
                (
                    mission.model,
                    mission.range_m,
                    mission.payload_kg,
                    known,
                    minimum_m,
                )
            }
            None => (
                model,
                range_m,
                None,
                route.mission_distance_known,
                minimum_profile_range_m,
            ),
        };
    let context = MdaContext {
        config: &candidate_config,
        dv: &dv,
        model: model.with_budget(controls.budget.unwrap_or(UNLIMITED_BUDGET)),
        range_m,
        plan,
        dispatch_payload_kg,
        tank_capacity_kg: tank_capacity,
        retrim_allowed: external.is_none(),
        screening_drag_table: controls.screening_drag_table,
        structural_reference: structural_reference.reference,
        structural_feedback: structural_reference.feedback,
        structural_inventory_complete: structural_reference.inventory_complete,
        wing_box: Default::default(),
        vlm_cache,
        initial_trim_cg_x_m,
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
    let trim_reuse = external.is_none().then(|| {
        let mut without_gear = candidate_config.clone();
        without_gear.landing_gear.derived_main_gear = None;
        TrimReuse {
            config: without_gear,
            dv,
            untrimmed,
            trimmed: plane.clone(),
            polar: state.polar.clone(),
            trim_cg_x_m: closure.trim_cg_x_m,
            screening_drag_table: controls.screening_drag_table,
            vlm_cache: context.vlm_cache.clone(),
        }
    });
    let work = SizingWork {
        plan_freezes: closure.plan_freezes,
        trip_flights: context.model.work().map_or(0, |(flights, _)| flights),
        deck_evals: 0,
    };

    let (operating_empty_mass_kg, _) = oew_and_cg(&state.masses, &state.coords);
    // A fixed-requirement run evaluates the aircraft at its declared MTOW;
    // the dispatch solution is the mission's required takeoff mass and is
    // compared against that ceiling by the mass residuals.  Keeping those
    // quantities separate prevents geometry/thrust/CG checks from using the
    // lower mission-required mass while the component ledger is closed at
    // the declared MTOW.  Every mission-closed mode (`SizedByMission`,
    // `Unconstrained`, `MtowBand`, `PayloadAdjusted`) uses the converged
    // dispatch mass for both purposes instead, since there each candidate's
    // own closure, not the declared requirement, is what the analysis mass
    // answers to.
    let analysis_takeoff_mass_kg = if plan.analyses_at_closure() {
        closure.dispatch.takeoff_mass_kg
    } else {
        initial_analysis_mass_kg
    };
    // The last trim moved to the cruise lift of the analysis mass: the drag
    // table spans the lift range, so the reported lift-to-drag ratio and
    // attitude belong to the mass the residuals are evaluated at.
    let polar = {
        let atmosphere = Atmosphere::new(req.cruise_altitude_m);
        let speed_m_s = req.cruise_mach * atmosphere.speed_of_sound();
        let dynamic_pressure_pa = 0.5 * atmosphere.density() * speed_m_s * speed_m_s;
        state.polar.at_cruise_cl(
            analysis_takeoff_mass_kg * req.gravity_m_s2 / (dynamic_pressure_pa * plane.s_ref),
        )
    };
    // The design-weight basis the ledger was closed on, made explicit so a
    // report can say whether the components belong to the declared aircraft
    // or to the sized one (`alas_config::MassSizingBasis`).
    let (sizing_basis, design_gross_mass_kg, design_landing_mass_kg) =
        super::mtow_modes::design_weights(
            &candidate_config,
            &plan,
            analysis_takeoff_mass_kg,
            declared_mtow_kg,
            super::mtow_modes::landing_floor_kg(&closure.dispatch),
        );
    // The payload the closure flew: the design payload of a design mission,
    // otherwise the laid-out load case. The derived design MZFW adds it to
    // the operating empty mass.
    let design_payload_kg = dispatch_payload_kg.unwrap_or(state.masses.payload);
    let offdesign = match route_model.as_ref() {
        Some(route) if route_distance_m > 0.0 => Some(super::offdesign::fly_route(
            &candidate_config,
            route,
            route_distance_m,
            &super::offdesign::ClosedAircraft {
                polar: &polar,
                operating_empty_mass_kg,
                payload_kg: state.masses.payload,
                design_payload_kg,
                // The takeoff-mass limit the route is flown under, as the
                // reporting dispatch takes it: the mass a design mission
                // closed on, else the declared MTOW that bounds the closure.
                mtow_kg: if closes_on_design_mission {
                    analysis_takeoff_mass_kg
                } else {
                    declared_mtow_kg
                },
                planning_mass_kg: analysis_takeoff_mass_kg,
                usable_capacity_kg: tank_capacity,
            },
        )?),
        _ => None,
    };
    // MDA updates the candidate box and current FLOPS total. An initially
    // complete frozen-reference diagnostic cannot verify the final inventory.
    let structural_inventory_complete = closure.structural_inventory_complete
        && alas_mass::wing_reconciliation::primary_fits_complete_wing(
            closure.structural_feedback.primary_mass_kg,
            state.masses.wing,
        );
    let sized = SizedCandidate {
        takeoff_mass_kg: analysis_takeoff_mass_kg,
        takeoff_loading,
        sizing_basis,
        design_gross_mass_kg,
        design_landing_mass_kg,
        mtow: super::mtow_modes::MtowPlanOutcome {
            structural_basis: plan.structural_basis.as_str(),
            design_payload_kg,
            derived_design_mzfw_kg: operating_empty_mass_kg + design_payload_kg,
            offdesign,
        },
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
        takeoff_fuel_kg: takeoff_loading.map_or_else(
            || closure.dispatch.plan.takeoff_fuel_kg(),
            |load| load.carried_usable_fuel_kg,
        ),
        ramp_fuel_kg: closure.dispatch.plan.ramp_fuel_kg(),
        usable_capacity_kg: tank_capacity.unwrap_or(f64::NAN),
        design_range_m: range_m,
        mission_distance_known,
        airport_records_resolved: route.records_resolved(),
        declared_airport_data_complete: route.declared_data_complete(),
        minimum_profile_range_m,
        lift_to_drag: polar.lift_to_drag,
        design_mission_fuel_kg: closure.dispatch.plan.takeoff_fuel_kg(),
        design_mission_trip_fuel_kg: closure.dispatch.plan.trip.kg,
        fuel_artifacts: Arc::new(CandidateFuelArtifacts {
            drag: polar.drag.clone(),
            reference_area_m2: plane.s_ref,
            deck: deck_key,
            frozen_plan: closure.frozen_plan,
            tail_sizing: TailSizing::of(&candidate_config.geometry.empennage, &dv),
        }),
        // The off-design route flies the same deck, so its evaluations are
        // on the same counter.
        work: SizingWork {
            deck_evals: context.model.propulsion.evaluation_count(),
            ..work
        },
        dispatch: closure.dispatch,
        sizing_iterations: closure.sizing_iterations,
        sizing_closed: closure.sizing_closed,
        takeoff_mass_settled: closure.takeoff_mass_settled,
        retrim_count: closure.retrim_count,
        cg_shift_pct_mac: closure.cg_shift_pct_mac,
        structural_inventory_complete,
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
        geometric_body_alpha_deg: polar.geometric_body_alpha_deg,
        n_engines: n_engines as i64,
        static_thrust_kn,
        departure,
        arrival: route.arrival,
        declared_airport_data_complete: route.declared_data_complete(),
        airport_records_resolved: route.records_resolved(),
        departure_record: route.departure_record,
        arrival_record: route.arrival_record,
        mission_distance_known,
        minimum_profile_range_m,
        mtow_ceiling,
        plan,
        sized,
        history,
        capacity,
        structural_inventory_complete,
        trim_reuse,
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
        // ATR preset itself has its published gear anchors.
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

#[cfg(test)]
mod cancellation_tests {
    #[test]
    fn pre_cancelled_candidate_is_not_a_numerical_or_physical_rejection() {
        let token = crate::cancellation::EvaluationCancellation::new();
        token.request();
        let result = super::run_candidate_cancellable(
            &alas_config::AlasConfig::default(),
            &[],
            None,
            false,
            Some(token),
            crate::mdo::types::SizingControls::default(),
        );
        assert!(matches!(
            result,
            Err(super::CandidateFailure {
                reason: "cancelled"
            })
        ));
    }
}
