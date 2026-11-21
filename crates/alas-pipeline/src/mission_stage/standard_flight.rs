// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The standard mission (no mission plan): one list of phases that is both
//! planned for the Mission page and flown by the run.
//!
//! [`planned_standard_mission`] and [`evaluate`] build their schedules with the
//! same [`build_schedules`], so the phases the page draws are the phases the
//! pipeline flies:
//!
//! - taxi-out and taxi-in: ground time (`taxi_time_min`, and the 7 min LTO
//!   taxi-in of `alas_mass::fuel_plan::TAXI_IN_TIME_MIN`) at the engine's
//!   ground-idle fuel flow. There is no ground-roll segment in the segment
//!   network, so the fuel is the product of that time and flow; the flight
//!   starts at the mass after the taxi-out (brake release), as the dispatch
//!   closure defines it;
//! - take-off, initial climb, cruise legs and step climbs (the leg count and
//!   levels of the profile), descent, approach and landing: flown segments;
//! - the reserve program ([`super::reserve`]): flown segments after the
//!   destination, from the landing mass.

use std::sync::atomic::{AtomicBool, Ordering};

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_mass::fuel_plan::TAXI_IN_TIME_MIN;
use alas_mission::build_mission_request;
use alas_mission::segments::SegmentSpec;
use alas_mission::{MissionRequest, MissionResult};

use super::flight::fly_with_guidance;
use super::reserve::{self, FlownReserves, ReserveProgram};
use super::schedule::speed_change::{CruiseSpeedRule, SpeedMemory};
use super::schedule::takeoff::TakeoffAcceleration;
use super::schedule::Reach;
use super::schedule::{build_schedule_with_memory, configuration::apply_configuration_drag};
mod sizing;
mod supplement;
use super::sequence::{flown_phases, planned_phases, warnings, Phase, PlannedMission, Scope};
use super::{build_analyses, cruise_level, dispatch, plan, SelectedLoadCase};
use crate::full_analysis::AnalysisReport;
pub use sizing::SizingOutcome;
use sizing::{flown_policy, required_fuel_kg};
use supplement::supplement;

/// What the native flight adds to the trip: the ground phases and the reserve
/// program as flown.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeSupplement {
    /// Taxi-out (time and fuel), before brake release.
    pub taxi_out: Phase,
    /// Taxi-in (time and fuel), after landing.
    pub taxi_in: Phase,
    /// The trip as flown (taxi excluded), in flight order: what the Mission page draws after a run.
    pub trip_phases: Vec<Phase>,
    /// Why the top level is below the configured one, when it is: the aircraft cannot hold the
    /// configured level at the dispatched mass. The levels of the ladder were fitted under it.
    pub reach_note: Option<ReachNote>,
    /// The reserve program as flown, from the landing mass; `None` when the
    /// scheme carries none or it could not be flown.
    pub reserves: Option<FlownReserves>,
    /// Why the reserve program was not flown, when it was not.
    pub reserve_error: Option<String>,
    /// How the sizing of the takeoff mass on the flown mission ended (standard mission only).
    pub sizing: Option<SizingOutcome>,
}

/// A configured top level lowered to what the aircraft holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachNote {
    /// Configured top level, m.
    pub configured_m: f64,
    /// Level flown, m.
    pub flown_m: f64,
    /// Why the level was lowered.
    pub cause: ReachCause,
}

/// Why a top level is flown below the configured one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachCause {
    /// The aircraft cannot hold the configured level at the dispatched mass.
    Mass,
    /// The altitudes were fitted under the footprint of the climbs and descents on the route.
    RouteFit,
}

/// The schedules of the standard mission on a route.
pub(super) struct Schedules {
    /// The trip, take-off to landing.
    pub trip: Vec<SegmentSpec>,
    /// What the planner remembered about the trip's speed changes.
    pub memory: SpeedMemory,
    /// The reserve program, or why it could not be built.
    pub reserve: Result<Option<ReserveProgram>, String>,
}

/// The one construction of the standard mission's schedules, used by the plan
/// and by the run.
pub(super) fn build_schedules(
    request: &MissionRequest,
    config: &AlasConfig,
    rule: Option<CruiseSpeedRule>,
    takeoff: Option<&TakeoffAcceleration>,
    reach: Option<&Reach>,
) -> Result<Schedules, String> {
    let (trip, memory) = build_schedule_with_memory(request, rule.clone(), takeoff, reach)?;
    let reserve = reserve::build(request, &config.fuel_policy, rule);
    Ok(Schedules {
        trip,
        memory,
        reserve,
    })
}

/// The number of cruise legs the profile asks for: its active distance shares.
fn configured_cruise_legs(request: &MissionRequest) -> usize {
    let p = &request.profile;
    [
        p.cruise_1_distance_fraction,
        p.cruise_2_distance_fraction,
        p.cruise_3_distance_fraction,
    ]
    .iter()
    .filter(|share| **share > 0.0)
    .count()
}

/// The phases the standard mission is flown as on a route, before it is
/// flown: the list the Mission page draws and the run follows.
///
/// # Errors
///
/// The profile or route is rejected by the schedule construction, as text.
pub fn planned_standard_mission(
    config: &AlasConfig,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<PlannedMission, String> {
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let schedules = build_schedules(&request, config, None, None, None)?;
    let (program, reserve_note) = match &schedules.reserve {
        Ok(program) => (program.as_ref(), None),
        Err(error) => (
            None,
            Some(format!("the reserve program is not planned: {error}")),
        ),
    };
    let phases = planned_phases(
        &schedules.trip,
        request.departure_elevation_m,
        request.arrival_elevation_m,
        config.fuel_policy.taxi_time_min * 60.0,
        TAXI_IN_TIME_MIN * 60.0,
        program,
    );
    let mut notes = warnings(
        &phases,
        configured_cruise_legs(&request),
        request.cruise_altitude_m,
    );
    notes.extend(reserve_note);
    Ok(PlannedMission {
        phases,
        warnings: notes,
    })
}

/// The segment sequence the native flight is built from for the standard
/// mission on a route: the trip schedule of [`planned_standard_mission`]'s own
/// construction.
///
/// # Errors
///
/// As [`planned_standard_mission`].
pub fn planned_standard_schedule(
    config: &AlasConfig,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<Vec<SegmentSpec>, String> {
    let request = build_mission_request(config, origin, destination, route_distance_m);
    build_schedules(&request, config, None, None, None).map(|schedules| schedules.trip)
}

/// Passes of the sizing on the flown mission: the priced dispatch, then the dispatch solved on the
/// fuel the native flight burned, until the takeoff mass stops moving.
const SIZING_PASSES: usize = 6;

/// Run the native mission for the selected report and route (the standard
/// mission; a plan with stages is flown by [`plan::evaluate`]).
///
/// The takeoff mass is the policy mass of the dispatch on the segment model, then re-sized on the
/// flown mission: the trip, the diversion and the hold the native flight burned scale the priced
/// ones ([`dispatch::NativeScales`]) and the dispatch is solved again, so the fuel on board covers
/// what is flown, reserves included. Contingency stays the regulation's share of the trip fuel.
pub(super) fn evaluate(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<(Option<MissionResult>, SelectedLoadCase), String> {
    evaluate_cancellable(
        config,
        report,
        origin,
        destination,
        route_distance_m,
        &AtomicBool::new(false),
    )
}

/// [`evaluate`], stopping with an error between passes once `cancel` is set.
pub(in crate::mission_stage) fn evaluate_cancellable(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
    cancel: &AtomicBool,
) -> Result<(Option<MissionResult>, SelectedLoadCase), String> {
    // The route pricing the native flight does not change, built once for the passes below.
    let mut pricing = None;
    let dispatch_profile =
        alas_math::stage_profile::scope(alas_math::stage_profile::Stage::PipelineRouteDispatch);
    let (request, mut load_case) = dispatch::select_route_load_case_scaled(
        config,
        report,
        origin,
        destination,
        route_distance_m,
        None,
        &mut pricing,
    )?;
    drop(dispatch_profile);
    let _flight_profile =
        alas_math::stage_profile::scope(alas_math::stage_profile::Stage::PipelineTelemetryFlight);
    let mass_tolerance = |mass_kg: f64| dispatch::closure_mass_tolerance_kg(config, mass_kg);
    let mut scales = dispatch::NativeScales::default();
    let mut result = None;
    let mut outcome: Option<SizingOutcome> = None;
    for pass in 0..SIZING_PASSES {
        if cancel.load(Ordering::Relaxed) {
            return Err("the flight was cancelled".to_owned());
        }
        let priced = {
            let _policy_profile = alas_math::stage_profile::scope(
                alas_math::stage_profile::Stage::FlightCruisePolicy,
            );
            cruise_level::priced_cruise_policy(
                config,
                report,
                &request,
                load_case.takeoff_mass_kg,
                pricing.as_ref().map(|p| &p.model),
            )
        };
        result = fly_once(config, report, &request, &mut load_case, &priced, cancel);
        // Only a policy closure flies the mass the policy requires.
        let Some((trip_kg, plan)) = flown_policy(&load_case) else {
            break;
        };
        // A flight that did not complete, or whose reserves were not flown, is not a converged
        // sizing: it is reported, and the next pass flies again.
        let flown = load_case.native_supplement.as_deref();
        let reserves = flown.and_then(|supplement| supplement.reserves.as_ref());
        let reserves_expected = flown.is_some_and(|supplement| {
            supplement.reserves.is_some() || supplement.reserve_error.is_some()
        });
        let next = scales.after_flight(&plan, trip_kg, reserves);
        let repricing_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::FlightRepricing);
        let (_, repriced) = dispatch::select_route_load_case_scaled(
            config,
            report,
            origin,
            destination,
            route_distance_m,
            Some(next),
            &mut pricing,
        )?;
        drop(repricing_profile);
        let moved_kg = (repriced.takeoff_mass_kg - load_case.takeoff_mass_kg).abs();
        let reserves_flown = !reserves_expected || reserves.is_some();
        outcome = Some(SizingOutcome {
            converged: false,
            passes: pass + 1,
            mass_residual_kg: moved_kg,
            fuel_residual_kg: plan.takeoff_fuel_kg() - required_fuel_kg(&plan, trip_kg, reserves),
            clamped: next.clamped,
            reserves_flown,
        });
        scales = next;
        let settled = moved_kg <= mass_tolerance(load_case.takeoff_mass_kg) && reserves_flown;
        load_case = repriced;
        if settled || pass + 1 == SIZING_PASSES {
            // The flight reported is the one flown at the mass the plan was sized to.
            let priced = {
                let _policy_profile = alas_math::stage_profile::scope(
                    alas_math::stage_profile::Stage::FlightCruisePolicy,
                );
                cruise_level::priced_cruise_policy(
                    config,
                    report,
                    &request,
                    load_case.takeoff_mass_kg,
                    pricing.as_ref().map(|p| &p.model),
                )
            };
            result = fly_once(config, report, &request, &mut load_case, &priced, cancel);
            if let (Some((trip_kg, plan)), Some(sizing)) =
                (flown_policy(&load_case), outcome.as_mut())
            {
                let reserves = load_case
                    .native_supplement
                    .as_deref()
                    .and_then(|supplement| supplement.reserves.as_ref());
                sizing.fuel_residual_kg =
                    plan.takeoff_fuel_kg() - required_fuel_kg(&plan, trip_kg, reserves);
                sizing.reserves_flown = reserves.is_some()
                    || load_case
                        .native_supplement
                        .as_deref()
                        .is_some_and(|supplement| supplement.reserve_error.is_none());
                sizing.converged = settled
                    && sizing.reserves_flown
                    && sizing.fuel_residual_kg >= -mass_tolerance(load_case.takeoff_mass_kg);
            }
            break;
        }
    }
    if let (Some(sizing), Some(supplement)) = (outcome, load_case.native_supplement.as_mut()) {
        if !sizing.converged || sizing.clamped {
            tracing::warn!(?sizing, "the fuel sizing on the flown mission is not clean");
        }
        supplement.sizing = Some(sizing);
    }
    Ok((result, load_case))
}

/// Fly the standard mission once at the load case's mass, recording the flight on the load case.
fn fly_once(
    config: &AlasConfig,
    report: &AnalysisReport,
    request: &MissionRequest,
    load_case: &mut SelectedLoadCase,
    priced: &cruise_level::PricedCruise,
    cancel: &AtomicBool,
) -> Option<MissionResult> {
    load_case.native_error = None;
    load_case.native_supplement = None;
    if cancel.load(Ordering::Relaxed) {
        load_case.native_error = Some("the flight was cancelled".to_owned());
        return None;
    }
    // The taxi is priced at the engine's ground-idle flow; without it there is no taxi to fly.
    let Some(taxi_flow_kg_s) = priced.taxi_fuel_flow_kg_s.filter(|flow| flow.is_finite()) else {
        // The message is the cause: the priced model missing, or the flow itself.
        load_case.native_error = Some(priced.taxi_unavailable.clone().unwrap_or_else(|| {
            "the taxi cannot be priced: the engine's ground-idle fuel flow is not finite".to_owned()
        }));
        return None;
    };
    // The one native flight, at the selected mass; its failure never stops the stage. The
    // configured ladder is flown first; only when the aircraft cannot fly it at this mass is the
    // ladder fitted under the level the priced route holds (and the run says so).
    let attempt = |reach: Option<&Reach>| {
        let build_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::FlightBuildAnalyses);
        let mut analyses = build_analyses(config, report)?;
        drop(build_profile);
        analyses.takeoff_mass_kg = load_case.takeoff_mass_kg;
        analyses.minimum_mass_kg = Some(load_case.zero_fuel_mass_kg);
        let schedule_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::FlightSchedules);
        let schedules = build_schedules(
            request,
            config,
            priced.rule.clone(),
            priced.takeoff.as_ref(),
            reach,
        )?;
        let mut schedule = schedules.trip;
        apply_configuration_drag(&mut schedule, &config.performance);
        plan::add_carried_drag(&mut schedule, report, analyses.reference_area_m2);
        drop(schedule_profile);
        let _guidance_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::FlightGuidance);
        let mut flown_level = Vec::new();
        let result = fly_with_guidance(
            schedule,
            request,
            &analyses,
            schedules.memory,
            false,
            &mut flown_level,
        )?;
        Ok::<_, String>((result, schedules.reserve, analyses, flown_level))
    };
    // The initial cruise level is the one the priced route plans: the highest the aircraft holds
    // at the top-of-climb mass with 300 ft/min residual climb at maximum-climb thrust, under the
    // planner's ceilings (the maximum-altitude convention and its source on
    // `alas_perf::performance::TOC_RESIDUAL_CLIMB_M_S`). The ladder is fitted under it from the
    // first flight, so a configured level above it is not climbed to and refused for thrust piece
    // by piece. Only when that flight does not complete is the configured ladder flown as it is.
    let mut reach = priced.reach.as_ref();
    let mut native = attempt(reach);
    let completed = |native: &Result<(MissionResult, _, _, _), String>| {
        native
            .as_ref()
            .is_ok_and(|(result, _, _, _)| result.completed_summary().is_some())
    };
    if !completed(&native) && reach.is_some() {
        reach = None;
        native = attempt(None);
    }
    load_case.native_error = None;
    load_case.native_supplement = None;
    load_case.flown_level_path_changes.clear();
    load_case.propulsion_validity = Default::default();
    match native {
        Ok((mut result, reserve, mut analyses, flown_level)) => {
            load_case.flown_level_path_changes = flown_level;
            load_case.propulsion_validity =
                super::deck_validity::PropulsionDeckValidity::of_flight(&mut result, &analyses);
            if let Some(summary) = result.completed_summary() {
                load_case.record_native_trip(summary.trip_fuel_kg);
                let _supplement_profile = alas_math::stage_profile::scope(
                    alas_math::stage_profile::Stage::FlightSupplement,
                );
                load_case.native_supplement = Some(Box::new(supplement(
                    config,
                    report,
                    &mut analyses,
                    priced,
                    taxi_flow_kg_s,
                    reach,
                    reserve,
                    summary.landing_mass_kg,
                    request,
                    flown_phases(&result, Scope::Trip),
                )));
                let reserve_notes = load_case
                    .native_supplement
                    .as_ref()
                    .and_then(|supplement| supplement.reserves.as_ref())
                    .map(|reserves| reserves.flown_level.clone())
                    .unwrap_or_default();
                load_case.flown_level_path_changes.extend(reserve_notes);
            } else {
                // The configured profile cannot be flown on this route at this mass: the
                // run says so; it does not fly a different profile instead.
                load_case.native_error = result.completion_refusal();
            }
            Some(result)
        }
        Err(error) => {
            load_case.native_error = Some(error);
            None
        }
    }
}
