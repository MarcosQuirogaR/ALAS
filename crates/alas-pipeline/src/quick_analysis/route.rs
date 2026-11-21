// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planned route, flown off-design as the full analysis flies it.
//!
//! The aircraft is sized on its design mission, the still-air great circle
//! between the declared airports. The route it is then flown over is the
//! one the full analysis' mission stage plans ([`plan_mission_route`]), an
//! airway route where the navigation data is installed (1.6 to 19.1 % longer
//! than the great circle on the registered presets). Its fuel is priced by the
//! mission stage's own call ([`select_route_load_case`]) on the full
//! baseline analysis' report: the unified segment mission model with the
//! closure's trimmed drag table, deck and frozen-plan policy, the same
//! takeoff-mass and tank limits. The sandbox therefore reports the route
//! fuel the full analysis reports, to the dispatch settling tolerance.
//!
//! The one tier the sandbox does not use is the live dispatch service
//! (SimBrief), a network request the in-process sandbox never makes; with a
//! SimBrief account configured, the full analysis may fly that plan instead.

use std::sync::atomic::AtomicBool;

use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignVector};

use crate::feasibility::{
    assess_physical_feasibility, assess_physical_feasibility_on_route, FeasibilityReport,
};
use crate::full_analysis::AnalysisReport;
use crate::mission_route::{great_circle_m, plan_mission_route, route_endpoints};
use crate::mission_stage::dispatch::select_route_load_case;
use crate::mission_stage::{fly_standard_mission, NativeFlight};

use super::types::{QuickOutcome, QuickRouteFuel};

/// The route as the full analysis flew it, kept so the Feasibility metric judges the same flight:
/// the flown mission and its load case are what the pipeline hands its own feasibility
/// assessment, with the route's two aerodromes.
pub(super) struct FlownRoute {
    pub(super) flight: NativeFlight,
    pub(super) origin: Airport,
    pub(super) destination: Airport,
}

/// The physical assessment of the sandbox aircraft, on the flight the route metric flew, exactly
/// as the pipeline hands its own assessment the mission, its load case and the route's two
/// aerodromes: the loading states judged (the dispatched fuel, not the takeoff-mass closure
/// remainder) and the findings raised are the Full Analysis'. Without that flight (not completed,
/// cancelled, no route) the aircraft is assessed with no mission requested of it: a mission
/// requested and never handed to the assessment is reported as "the route was not flown" on every
/// aircraft, which blocked them all whatever their physics.
pub(super) fn assess_on_flown_route(
    sandbox: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    flown: Option<&FlownRoute>,
) -> FeasibilityReport {
    match flown {
        Some(flown) => assess_physical_feasibility_on_route(
            sandbox,
            design,
            report,
            flown.flight.mission.as_ref(),
            Some(&flown.flight.load_case),
            Some((&flown.origin, &flown.destination)),
        ),
        None => {
            let mut assessed = sandbox.clone();
            assessed.mission.enabled = false;
            assess_physical_feasibility(&assessed, design, report, None)
        }
    }
}

/// The planned route's fuel on `report`, or why it is not available.
/// The priced route fuel, immediately: an estimate until [`route_fuel_flown`] replaces it.
pub(super) fn route_fuel(config: &AlasConfig, report: &AnalysisReport) -> QuickOutcome {
    route_fuel_with(config, report, None).0
}

/// The route fuel of the native flight the full analysis flies (same dispatch, same sizing passes
/// on the flown fuel) with the flight itself, or `None` when the flight does not complete or
/// `cancel` is set: the estimate then stands.
pub(super) fn route_fuel_flown(
    config: &AlasConfig,
    report: &AnalysisReport,
    cancel: &AtomicBool,
) -> Option<(QuickOutcome, FlownRoute)> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    match route_fuel_with(config, report, Some(cancel)) {
        (outcome @ QuickOutcome::Route(_), Some(flown)) => Some((outcome, flown)),
        _ => None,
    }
}

fn route_fuel_with(
    config: &AlasConfig,
    report: &AnalysisReport,
    flown: Option<&AtomicBool>,
) -> (QuickOutcome, Option<FlownRoute>) {
    if !config.mission.enabled {
        return (
            QuickOutcome::Unsupported("the mission is disabled, so no route is flown".to_owned()),
            None,
        );
    }
    let Some(route) = plan_mission_route(config, None) else {
        return (
            QuickOutcome::Unsupported(
                "the declared airports are not in the registry, so no route is planned".to_owned(),
            ),
            None,
        );
    };
    let mut flight = None;
    let priced = route_endpoints(config, &route).and_then(|(origin, destination)| {
        let distance_m = route.total_distance_m();
        let load_case = match flown {
            None => select_route_load_case(config, report, origin, destination, distance_m)
                .map(|(_, load_case)| load_case)?,
            Some(cancel) => {
                let flown =
                    fly_standard_mission(config, report, origin, destination, distance_m, cancel)?;
                let load_case = flown.load_case.clone();
                flight = Some((flown, origin.clone(), destination.clone()));
                load_case
            }
        };
        Ok((load_case, great_circle_m(origin, destination)))
    });
    let (load_case, great_circle_m) = match priced {
        Ok(priced) => priced,
        Err(error) => {
            return (
                QuickOutcome::Failed(format!("route pricing failed: {error}")),
                None,
            )
        }
    };
    let flown_route = flight.map(|(flight, origin, destination)| FlownRoute {
        flight,
        origin,
        destination,
    });
    let Some(case) = load_case.route_case() else {
        let message = match &load_case.selection {
            crate::LoadCaseSelection::MaximumAvailableFuel {
                reason: Some(reason),
                ..
            } => format!("the route could not be priced on the segment mission model: {reason}"),
            _ => match &load_case.native_error {
                Some(reason) => format!("the route could not be flown: {reason}"),
                None => "the route could not be priced on the segment mission model".to_owned(),
            },
        };
        return (QuickOutcome::Failed(message), None);
    };
    let outcome = QuickOutcome::Route(QuickRouteFuel {
        block_fuel_kg: case.plan.block_fuel_kg(),
        takeoff_fuel_kg: case.plan.takeoff_fuel_kg(),
        takeoff_mass_kg: load_case.takeoff_mass_kg,
        shortfall_kg: case.shortfall_kg,
        route_distance_m: case.route_distance_m,
        great_circle_m,
        estimate: flown.is_none(),
        source: route.source.as_str().to_owned(),
        note: format!(
            "{} route of {:.0} km flown off-design by the full analysis' mission stage: segment mission model on the closure's trimmed drag table and frozen-plan policy, reserves under scheme {}{}",
            route.source.as_str(),
            case.route_distance_m / 1000.0,
            config.fuel_policy.scheme.as_str(),
            if case.shortfall_kg > 0.0 {
                format!("; {:.0} kg short of the route's policy fuel", case.shortfall_kg)
            } else {
                String::new()
            }
        ),
    });
    (outcome, flown_route)
}

/// The standard mission of `config` on the aircraft `design` draws, flown exactly as the full
/// analysis' mission stage flies it (same report, route tier, dispatch and sizing passes): the
/// planned flight the Mission page draws before any run. Heavy (a full baseline analysis and a
/// few native flights): run it off the interface thread.
///
/// # Errors
///
/// The baseline analysis fails, the airports do not resolve, the flight does not complete (the
/// refusal is the error), or `cancel` is set (checked between the stages and the flights).
///
/// `dispatched` is the route the run flies when a dispatch service supplied one (SimBrief): the
/// run resolves its route with [`plan_mission_route`] on that route first, and so does this, so the
/// page and the run fly one route.
pub fn planned_flight(
    config: &AlasConfig,
    design: &alas_config::design_variables::DesignVector,
    dispatched: Option<alas_route::route::Route>,
    cancel: &AtomicBool,
) -> Result<crate::mission_stage::NativeFlight, String> {
    if !config.mission.enabled {
        return Err("the mission is disabled, so no route is flown".to_owned());
    }
    let cancelled = || cancel.load(std::sync::atomic::Ordering::Relaxed);
    let analysis = crate::full_analysis::FullAnalysis::new(config.clone());
    let airplane = analysis
        .build_airplane(design, true)
        .map_err(|error| format!("full baseline analysis failed: {error}"))?;
    if cancelled() {
        return Err("the flight was cancelled".to_owned());
    }
    // The fuel artifacts the mission stage needs are built beside the analysis, not after it.
    let report =
        crate::fuel_model::run_on_airplane_with_baseline_artifacts(&analysis, design, airplane)
            .map_err(|error| format!("full baseline analysis failed: {error}"))?;
    if cancelled() {
        return Err("the flight was cancelled".to_owned());
    }
    let route = plan_mission_route(config, dispatched)
        .ok_or_else(|| "the declared airports are not in the registry".to_owned())?;
    let (origin, destination) = route_endpoints(config, &route)?;
    fly_standard_mission(
        config,
        &report,
        origin,
        destination,
        route.total_distance_m(),
        cancel,
    )
}
