// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise speeds and the take-off acceleration the native flight is given
//! by the priced route.
//!
//! The cruise levels and the number of cruise legs are not given: the native
//! flight flies the profile's own ladder (the leg count the user sets, the
//! levels and the distance shares of the profile), so that the planned mission
//! the Mission page draws is the mission that is simulated. The priced route
//! (`alas-opt` `fly_trip`, which chooses its own levels by weight burn) still
//! supplies the speed a declared cruise Mach resolves to at each level, the
//! acceleration from V2 it flies in front of its take-off band, and the idle
//! fuel flow that prices the taxi.

use alas_config::AlasConfig;
use alas_mission::MissionRequest;

use super::schedule::speed_change::CruiseSpeedRule;
use super::schedule::takeoff::TakeoffAcceleration;
use super::schedule::Reach;
use crate::full_analysis::AnalysisReport;
use alas_mass::fuel_plan::FuelBurnModel;

/// What the native flight takes from the priced route.
#[derive(Default)]
pub(super) struct PricedCruise {
    /// The rule resolving a declared cruise speed to a level, when the priced
    /// model is available.
    pub rule: Option<CruiseSpeedRule>,
    /// The acceleration from the take-off safety speed the priced route flew in
    /// front of its take-off band, when it flew one.
    pub takeoff: Option<TakeoffAcceleration>,
    /// Fuel flow at ground idle with every engine running, kg/s: the flow that
    /// prices the taxi-out and the taxi-in.
    pub taxi_fuel_flow_kg_s: Option<f64>,
    /// Why the flow is missing: the priced model's or the engine's own error.
    pub taxi_unavailable: Option<String>,
    /// The highest level and the speeds the priced route holds at the dispatched mass.
    pub reach: Option<Reach>,
}

/// One cruise policy for both models of the mission.
///
/// *Speed.* A declared cruise speed is the one for the configured level; at
/// another level the native legs fly the speed the priced route flies there
/// (constant Mach up to VC for a jet, constant equivalent airspeed for a
/// turboprop), returned as the rule the schedule applies at the level each cruise
/// leg ends up at.
pub(super) fn priced_cruise_policy(
    config: &AlasConfig,
    report: &AnalysisReport,
    request: &MissionRequest,
    takeoff_mass_kg: f64,
    built: Option<&alas_opt::SegmentMissionModel>,
) -> PricedCruise {
    // `built` is the route model a caller already made from this report (the dispatch's): the
    // same model, not rebuilt for every pass of a run.
    let model = match built.cloned().map_or_else(
        || crate::fuel_model::report_route_mission_model(config, report),
        Ok,
    ) {
        Ok(model) => model,
        Err(error) => {
            tracing::warn!(error, "the priced mission model is unavailable; the native flight keeps the profile speeds");
            return PricedCruise {
                taxi_unavailable: Some(format!("the route cannot be priced: {error}")),
                ..PricedCruise::default()
            };
        }
    };
    let mut takeoff = None;
    let mut reach = None;
    match model.fly_trip(takeoff_mass_kg, request.route_distance_m) {
        Ok(flown) => {
            takeoff = takeoff_acceleration(&model, &flown);
            let runs: Vec<(f64, f64)> = flown
                .cruise_ladder()
                .runs
                .iter()
                .map(|run| (run.altitude_m, run.tas_m_s))
                .collect();
            // The level held at the dispatched mass is the first cruise run, the initial cruise
            // level the priced climb levels off at (300 ft/min residual at maximum-climb
            // thrust, `alas_perf::performance::TOC_RESIDUAL_CLIMB_M_S`). A higher run comes
            // after a step at a lighter mass: climbing straight to it at the top-of-climb mass
            // is refused for thrust.
            let top_m = runs.first().map_or(f64::NEG_INFINITY, |run| run.0);
            if top_m.is_finite() {
                reach = Some(Reach { top_m, runs });
            }
        }
        Err(error) => tracing::warn!(
            %error,
            "the priced route is unavailable; the native flight keeps the profile take-off"
        ),
    }
    let (taxi_fuel_flow_kg_s, taxi_unavailable) = match model.taxi_fuel_flow_kg_s() {
        Ok(flow) => (Some(flow), None),
        Err(error) => (
            None,
            Some(format!(
                "the taxi cannot be priced: ground-idle fuel flow: {error}"
            )),
        ),
    };
    PricedCruise {
        rule: Some(CruiseSpeedRule::new(move |level_m, declared_m_s| {
            model.cruise_speed_at_level(level_m, declared_m_s).ok()
        })),
        takeoff,
        taxi_fuel_flow_kg_s,
        taxi_unavailable,
        reach,
    }
}

/// The acceleration from V2 the priced route flew (its first level step), with the
/// take-off over maximum-climb thrust ratio of the deck at its mean condition.
fn takeoff_acceleration(
    model: &alas_opt::SegmentMissionModel,
    flown: &alas_opt::mdo::mission_model::FlownLeg,
) -> Option<TakeoffAcceleration> {
    use alas_prop::system::PropulsionRating::{MaximumClimb, TakeoffGoAround};
    let step = flown.steps.iter().find(|step| {
        step.phase == alas_opt::mdo::mission_model::StepPhase::Takeoff
            && step.tas_m_s[1] > step.tas_m_s[0]
    })?;
    let mean_m_s = 0.5 * (step.tas_m_s[0] + step.tas_m_s[1]);
    let altitude_m = step.altitude_m[0];
    let flight = model
        .propulsion
        .flight_condition(
            altitude_m,
            mean_m_s,
            model.gravity_m_s2,
            model.isa_deviation_c,
        )
        .ok()?;
    let rated = |rating| {
        model
            .propulsion
            .rated_point(flight, rating)
            .ok()
            .map(|point| point.thrust_n)
    };
    let ratio = rated(TakeoffGoAround)? / rated(MaximumClimb)?;
    ratio.is_finite().then_some(TakeoffAcceleration {
        v2_m_s: step.tas_m_s[0],
        thrust_ratio: ratio,
    })
}
