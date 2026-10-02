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

use alas_config::AlasConfig;

use crate::full_analysis::AnalysisReport;
use crate::mission_route::{great_circle_m, plan_mission_route, route_endpoints};
use crate::mission_stage::dispatch::select_route_load_case;

use super::types::{QuickOutcome, QuickRouteFuel};

/// The planned route's fuel on `report`, or why it is not available.
pub(super) fn route_fuel(config: &AlasConfig, report: &AnalysisReport) -> QuickOutcome {
    if !config.mission.enabled {
        return QuickOutcome::Unsupported(
            "the mission is disabled, so no route is flown".to_owned(),
        );
    }
    let Some(route) = plan_mission_route(config, None) else {
        return QuickOutcome::Unsupported(
            "the declared airports are not in the registry, so no route is planned".to_owned(),
        );
    };
    let priced = route_endpoints(config, &route).and_then(|(origin, destination)| {
        let distance_m = route.total_distance_m();
        select_route_load_case(config, report, origin, destination, distance_m)
            .map(|(_, load_case)| (load_case, great_circle_m(origin, destination)))
    });
    let (load_case, great_circle_m) = match priced {
        Ok(priced) => priced,
        Err(error) => return QuickOutcome::Failed(format!("route pricing failed: {error}")),
    };
    let Some(case) = load_case.route_case() else {
        return QuickOutcome::Failed(
            "the route could not be priced on the segment mission model".to_owned(),
        );
    };
    QuickOutcome::Route(QuickRouteFuel {
        block_fuel_kg: case.plan.block_fuel_kg(),
        takeoff_fuel_kg: case.plan.takeoff_fuel_kg(),
        takeoff_mass_kg: load_case.takeoff_mass_kg,
        shortfall_kg: case.shortfall_kg,
        route_distance_m: case.route_distance_m,
        great_circle_m,
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
    })
}
