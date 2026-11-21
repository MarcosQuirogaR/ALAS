// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Choosing the takeoff mass the route is flown at.
//!
//! An operator flies the fuel the policy requires for the route: taxi, trip,
//! contingency, alternate and final reserve, and no more, and that
//! requirement depends on the takeoff mass it produces. This module solves
//! that fixed point on the route model of the report's aircraft
//! ([`crate::fuel_model::report_route_mission_model`]): the segment mission model
//! of the sizing closure's drag, deck and frozen plan at the requested route's
//! own level and range. In `fixed_requirement` that is the model the closure
//! flew; in `mtow_band` and `payload_adjusted` the closure flew the design
//! mission (design range and design cruise altitude), and the route the user
//! asked for is priced at its own level, the level the native flight flies
//! (A320-200 on 645 km: 28,000 ft against the design 36,445 ft).
//!
//! An aircraft is sized on its design mission; the route the user requested
//! is an off-design mission of that aircraft. A report bound to a sized
//! candidate therefore flies the route at the sized aircraft (its carried
//! drag, deck and plan policy, on a plan frozen for the route), and reports
//! the design mission it was sized on beside it ([`DesignMissionCase`]). The
//! result is bounded by the aircraft's takeoff-mass limit and by the usable
//! tank capacity: route trip plus that route's reserves above the loadable
//! fuel is a hard shortfall, reported rather than flown short.
//!
//! The native pseudospectral mission is then flown once at the selected mass
//! (`mission_stage::evaluate`) and the dispatch is solved again on the fuel it burned
//! ([`NativeScales`]: trip, diversion and hold), so the flown mass carries the fuel the flown
//! mission needs; the first pass above is the priced dispatch (the sandbox's quick route
//! fuel stops there). The native trip is recorded in [`NativeTripCheck`]; it is not a bound on the segment model: the
//! two do not share aerodynamics (the native flight trims a vortex-lattice
//! lift surrogate with its own drag build-up, the model flies the closure's
//! trimmed drag table) nor a profile (the native schedule flies the levels and
//! step climbs of the configured profile, the model its own priced ones).
//! Measured native-minus-model trip at the charted routes after the fuel
//! model unification (priced ladder handed to the native flight): A320
//! +2.2 %, A220 +6.9 %, A340 +11.5 %, A380 +4.0 %, B787 +12.0 %, DC-10
//! +2.1 %, ATR +5.1 %; the configured ladder is flown now and the
//! difference is recorded per run in [`NativeTripCheck`].

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_mass::dispatch::{DispatchLimits, DispatchSolution, DispatchStatus};
use alas_mass::fuel_plan::{FuelBurnModel, FuelPlan};
use alas_mission::{build_mission_request, MissionRequest};

use crate::feasibility::{plan_fuel_loading, FuelLoadingAssessment};

use crate::full_analysis::AnalysisReport;

pub(crate) mod field_climb;
pub(crate) mod landing_limit;
mod native_scales;
mod route_pricing;
pub use native_scales::NativeScales;
pub(crate) use route_pricing::RoutePricing;

/// The native mission's trip at the selected takeoff mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeTripCheck {
    /// Trip fuel of the completed native mission, kg.
    pub native_trip_fuel_kg: f64,
}

/// The design mission a sized aircraft was closed on, as its sizing closure
/// priced it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignMissionCase {
    /// Still-air design range, m.
    pub range_m: f64,
    /// Takeoff mass of the closure's dispatch solution, kg.
    pub takeoff_mass_kg: f64,
    /// Fuel at brake release (trip plus reserves) of the closure's plan, kg.
    pub takeoff_fuel_kg: f64,
    /// Trip fuel of the closure's plan, kg.
    pub trip_fuel_kg: f64,
}

/// A declared (or design) cruise level and the level the dispatched trip flew at the dispatched
/// take-off mass: the priced route flies the highest level the aircraft can climb to, and the
/// user is told when that is lower than declared.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredLevelCheck {
    /// The level the policy declares, m.
    pub declared_m: f64,
    /// The highest cruise level of the trip flown at the dispatched mass, m, or why the trip
    /// could not be flown.
    pub flown_m: Result<f64, String>,
}

/// The policy closure that selected the flown load case.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyClosureCase {
    /// The route's plan at the selected mass.
    pub plan: FuelPlan,
    /// The dispatch solution on the segment model over the route.
    pub analytic: DispatchSolution,
    /// The native mission's trip at the selected mass, once it was flown to
    /// completion.
    pub native_check: Option<NativeTripCheck>,
    /// Whether the dispatch closure converged.
    pub converged: bool,
    /// Still-air distance of the route the plan was priced over, m.
    pub route_distance_m: f64,
    /// Loadable takeoff fuel (the takeoff-mass limit or the tanks less taxi,
    /// whichever binds) less the route's trip plus reserves, kg; negative
    /// when the route cannot carry its policy fuel.
    pub reserve_margin_kg: f64,
    /// Fuel the route needs beyond what the limits allow, kg: the negative
    /// margin once it exceeds the dispatch settling tolerance, else zero.
    pub shortfall_kg: f64,
    /// The design mission the aircraft was sized on, for a report bound to a
    /// sized candidate.
    pub design_mission: Option<DesignMissionCase>,
    /// The declared cruise level against the level flown, when the policy declares one.
    pub declared_level: Option<DeclaredLevelCheck>,
    /// Payload of the report the route leaves behind, kg: zero unless the
    /// flown policy case is landing-weight limited (`landing_limit`).
    pub offloaded_payload_kg: f64,
}

/// How the load case the mission flies was selected.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadCaseSelection {
    /// The maximum-available-fuel case of the frozen mission; the policy
    /// load case is switched off or cannot be priced.
    MaximumAvailableFuel {
        /// Why the route could not be priced on the segment mission model.
        reason: Option<String>,
        /// The route's policy case on the segment mission model, priced
        /// beside the flown load so its trip plus reserves are still checked
        /// against the loadable fuel; `None` exactly when `reason` is set.
        route_check: Option<Box<PolicyClosureCase>>,
    },
    /// The takeoff mass the fuel policy requires for the route. The case is
    /// boxed because it carries a full plan and an iterate history beside
    /// the reason-only variant, and is built once per run.
    PolicyClosure(Box<PolicyClosureCase>),
    /// A user-defined mission plan, flown stage by stage with the fuel closed
    /// to the plan (see [`super::plan`]).
    Plan(Box<super::plan::PlanLoadCase>),
}

/// The masses the mission is flown at.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedLoadCase {
    /// Mass at brake release, kg.
    pub takeoff_mass_kg: f64,
    /// Operating empty mass plus payload, kg: the floor the mission may not
    /// burn below.
    pub zero_fuel_mass_kg: f64,
    /// How the case was chosen.
    pub selection: LoadCaseSelection,
    /// Why the native pseudospectral flight at this load could not be
    /// flown, when it failed: telemetry, never a gate.
    pub native_error: Option<String>,
    /// The ground phases and the reserve program as the native flight flew
    /// them (standard mission only).
    pub native_supplement: Option<Box<super::standard_flight::NativeSupplement>>,
    /// The path changes of the trip the native flight flew level because no
    /// constant descent rate flies them, in words; reported as a warning.
    pub flown_level_path_changes: Vec<String>,
    /// Whether the propulsion deck answered the points of the native flight
    /// inside its evidence domain; reported as a warning when it did not.
    pub propulsion_validity: super::deck_validity::PropulsionDeckValidity,
}

impl SelectedLoadCase {
    /// The route's policy case, whichever load is flown: the flown policy
    /// closure, or the route check priced beside a maximum-available-fuel
    /// flight. `None` when the route could not be priced.
    pub fn route_case(&self) -> Option<&PolicyClosureCase> {
        match &self.selection {
            LoadCaseSelection::PolicyClosure(case) => Some(case),
            LoadCaseSelection::MaximumAvailableFuel { route_check, .. } => route_check.as_deref(),
            LoadCaseSelection::Plan(_) => None,
        }
    }

    /// Record the native trip flown at this load case.
    pub(super) fn record_native_trip(&mut self, native_trip_fuel_kg: f64) {
        // Only a policy closure flies the mass its case was priced at; a
        // route check beside a maximum-available-fuel flight was not.
        if let LoadCaseSelection::PolicyClosure(case) = &mut self.selection {
            case.native_check = Some(NativeTripCheck {
                native_trip_fuel_kg,
            });
        }
    }
}

/// The mission request for the route between `origin` and `destination`
/// over `route_distance_m` of still air, and the load case it is flown at
/// ([`select_load_case`] on the report's own fuel loading).
///
/// This is the whole route pricing of the full analysis' mission stage, and
/// the sandbox prices the route by the same call on the same report, so the
/// two report one route fuel.
pub(crate) fn select_route_load_case(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<(MissionRequest, SelectedLoadCase), String> {
    select_route_load_case_scaled(
        config,
        report,
        origin,
        destination,
        route_distance_m,
        None,
        &mut None,
    )
}

/// [`select_route_load_case`], with the trip, diversion and hold fuel scaled to what the native
/// flight burned (see [`NativeScales`]). `cache` keeps the route pricing the scales do not touch:
/// the passes of one run (same configuration, report and route) pass the same one.
pub(crate) fn select_route_load_case_scaled(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
    scales: Option<NativeScales>,
    cache: &mut Option<RoutePricing>,
) -> Result<(MissionRequest, SelectedLoadCase), String> {
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let loading_profile =
        alas_math::stage_profile::scope(alas_math::stage_profile::Stage::PipelineFuelLoading);
    let fuel_loading = plan_fuel_loading(config, &report.design, report);
    drop(loading_profile);
    let load_case =
        select_load_case_scaled(config, report, &fuel_loading, &request, scales, cache)?;
    Ok((request, load_case))
}

/// Select the load case for a route.
///
/// The route is always priced on the segment mission model, so its trip
/// plus reserves are checked against the loadable fuel whichever load is
/// flown. `fuel_loading` is the load case before the policy is applied; it
/// is flown when the policy load case is switched off (carrying the route
/// check beside it) or cannot be priced (carrying the reason).
#[cfg(test)]
pub(super) fn select_load_case(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    request: &MissionRequest,
) -> Result<SelectedLoadCase, String> {
    select_load_case_scaled(config, report, fuel_loading, request, None, &mut None)
}

fn select_load_case_scaled(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    request: &MissionRequest,
    scales: Option<NativeScales>,
    cache: &mut Option<RoutePricing>,
) -> Result<SelectedLoadCase, String> {
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    let maximum =
        |reason: Option<String>, route_check: Option<Box<PolicyClosureCase>>| SelectedLoadCase {
            takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
            zero_fuel_mass_kg,
            native_error: None,
            native_supplement: None,
            flown_level_path_changes: Vec::new(),
            propulsion_validity: Default::default(),
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason,
                route_check,
            },
        };
    Ok(
        match price_route(config, report, fuel_loading, request, scales, cache)? {
            Ok((takeoff_mass_kg, case)) if config.fuel_policy.fly_policy_load_case => {
                SelectedLoadCase {
                    takeoff_mass_kg,
                    // The dispatch's own: below the report's when the route is
                    // landing-weight limited.
                    zero_fuel_mass_kg: case.analytic.zero_fuel_mass_kg,
                    native_error: None,
                    native_supplement: None,
                    flown_level_path_changes: Vec::new(),
                    propulsion_validity: Default::default(),
                    selection: LoadCaseSelection::PolicyClosure(case),
                }
            }
            Ok((_, case)) => maximum(None, Some(case)),
            Err(reason) => maximum(Some(reason), None),
        },
    )
}

/// The route's dispatch on the segment mission model: the admissible
/// takeoff mass and the policy case, or why it could not be priced. `cache`
/// keeps the part of the pricing that the native flight's scales do not touch,
/// so the sizing passes of a run build it once.
fn price_route(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    request: &MissionRequest,
    scales: Option<NativeScales>,
    cache: &mut Option<RoutePricing>,
) -> Result<Result<(f64, Box<PolicyClosureCase>), String>, String> {
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    if cache.is_none() {
        match RoutePricing::solve(config, report, fuel_loading, request) {
            Ok(pricing) => *cache = Some(pricing),
            Err(reason) => return Ok(Err(reason)),
        }
    }
    let Some(pricing) = cache.as_mut() else {
        return Ok(Err("the route pricing is unavailable".to_owned()));
    };
    let sized = report.fuel.sized_fuel();
    let limits = pricing.limits;
    let range_m = request.route_distance_m;
    let max_iterations = config.optimizer.objective.sizing_max_iterations.max(1) as usize;
    let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
    // The policy sized on the flown mission: the same dispatch, solved again on the plan with the
    // trip, diversion and hold fuel the native flight burned.
    let scaled = scales.map(|scales| native_scales::Scaled {
        inner: &pricing.planned,
        scales,
    });
    let model: &dyn FuelBurnModel = match &scaled {
        Some(scaled) => scaled,
        None => &pricing.planned,
    };
    let solution = match scaled {
        Some(_) => alas_mass::dispatch::solve_dispatch_with_initial_guess(
            zero_fuel_mass_kg,
            pricing.solution.takeoff_mass_kg,
            range_m,
            &config.fuel_policy,
            model,
            &limits,
            max_iterations,
            tolerance_kg,
        ),
        None => pricing.solution.clone(),
    };
    // A load that would land above the maximum landing mass is offloaded to
    // it, as a dispatcher limits the traffic load (`landing_limit`).
    let solution =
        landing_limit::limit_route_dispatch(config, report, solution, range_m, model, &limits);
    if let DispatchStatus::ModelFailed(reason) = &solution.status {
        return Ok(Err(reason.clone()));
    }
    let offloaded_payload_kg = (zero_fuel_mass_kg - solution.zero_fuel_mass_kg).max(0.0);
    let zero_fuel_mass_kg = solution.zero_fuel_mass_kg;
    // Fuel above what the limits admit is a finding, not a load: the flight
    // is flown at the admissible mass and the shortfall is reported. The
    // margin is the route's trip plus reserves against the loadable fuel; a
    // deficit within the dispatch's own settling tolerance is the fixed
    // point's resolution, not a shortfall (the solver reports
    // `MtowLimited` on the same bound).
    let ceiling_kg = match pricing.ceiling_kg {
        // The tank bound moves with an offloaded zero-fuel mass; it is not cached.
        _ if offloaded_payload_kg > 0.0 => {
            admissible_takeoff_mass_ceiling(config, zero_fuel_mass_kg, &limits, &pricing.model)?
        }
        Some(ceiling_kg) => ceiling_kg,
        None => {
            let ceiling_kg = admissible_takeoff_mass_ceiling(
                config,
                zero_fuel_mass_kg,
                &limits,
                &pricing.model,
            )?;
            pricing.ceiling_kg = Some(ceiling_kg);
            ceiling_kg
        }
    };
    let reserve_margin_kg = ceiling_kg - zero_fuel_mass_kg - solution.plan.takeoff_fuel_kg();
    let shortfall_kg = if -reserve_margin_kg > tolerance_kg {
        -reserve_margin_kg
    } else {
        0.0
    };
    // The declared-level check flies the plan the dispatch settled on (`planned`), not a second
    // plan frozen at the closed mass.
    let declared_level = declared_level_check(
        &pricing.planned,
        solution.takeoff_mass_kg.min(ceiling_kg),
        range_m,
    );
    Ok(Ok((
        solution.takeoff_mass_kg.min(ceiling_kg),
        Box::new(PolicyClosureCase {
            plan: solution.plan,
            converged: matches!(
                solution.status,
                DispatchStatus::Converged
                    | DispatchStatus::MtowLimited { .. }
                    | DispatchStatus::TankLimited { .. }
            ),
            analytic: solution,
            native_check: None,
            route_distance_m: range_m,
            reserve_margin_kg,
            shortfall_kg,
            design_mission: sized.map(|sized| DesignMissionCase {
                range_m: sized.design_range_m,
                takeoff_mass_kg: sized.takeoff_mass_kg,
                takeoff_fuel_kg: sized.takeoff_fuel_kg,
                trip_fuel_kg: sized.trip_fuel_kg,
            }),
            declared_level,
            offloaded_payload_kg,
        }),
    )))
}

/// The mass resolution of the flown route closure at a takeoff mass of
/// `mass_kg`: the sizing tolerance, or 0.02 % of the mass when that is
/// larger. The closure settles when the repriced takeoff mass moves by no
/// more than this, so the mass the reported flight lands with may differ from
/// the dispatch's own landing mass by as much.
pub(crate) fn closure_mass_tolerance_kg(config: &AlasConfig, mass_kg: f64) -> f64 {
    config
        .optimizer
        .objective
        .sizing_tolerance_kg
        .max(2.0e-4 * mass_kg)
}

/// The declared level of the model's policy against the level its trip flies at `mass_kg` over
/// `range_m` (nothing for the optimum-step policy, which declares none).
fn declared_level_check(
    model: &alas_opt::SegmentMissionModel,
    mass_kg: f64,
    range_m: f64,
) -> Option<DeclaredLevelCheck> {
    use alas_config::mission::CruiseAltitudePolicy;
    let declared_m = match model.profile.cruise_altitude_policy {
        CruiseAltitudePolicy::OptimumStep => return None,
        CruiseAltitudePolicy::Design => model.design_cruise_altitude_m,
        CruiseAltitudePolicy::Declared => model.cruise_altitude_m,
    };
    // `model` is the dispatch's own planned model (`solve_planned_dispatch_with_plan`): it carries
    // the plan the dispatch settled on for this route, so `plan_for_mission` keeps that plan and
    // freezes nothing new. Only a model without a plan for `range_m` is frozen here, at `mass_kg`.
    let flown_m = alas_opt::mdo::plan_for_mission(model, mass_kg, range_m)
        .map_err(|error| error.to_string())
        .and_then(|planned| {
            planned
                .fly_trip(mass_kg, range_m)
                .map_err(|error| error.to_string())
        })
        .and_then(|leg| {
            leg.cruise_ladder()
                .runs
                .iter()
                .map(|run| run.altitude_m)
                .fold(None, |top: Option<f64>, level| {
                    Some(top.map_or(level, |t| t.max(level)))
                })
                .ok_or_else(|| "the trip has no cruise run".to_owned())
        });
    Some(DeclaredLevelCheck {
        declared_m,
        flown_m,
    })
}

/// The takeoff-mass limit of the aircraft `report` describes, kg. A
/// candidate closed on a design mission (`MtowPlan::design_mission`) has the
/// mass that mission closed on as its MTOW, and flies every other route
/// under it; every other aircraft flies under the declared
/// `requirements.mtow_kg`, which bounds its closure.
fn aircraft_mtow_kg(config: &AlasConfig, report: &AnalysisReport) -> f64 {
    match (
        config.mtow_plan().design_mission,
        report.fuel.sized_fuel(),
        report.sized_takeoff_mass_kg(),
    ) {
        (Some(_), Some(_), Some(closed_kg)) => closed_kg,
        _ => config.requirements.mtow_kg,
    }
}

/// The largest takeoff mass the limits admit: the takeoff-mass limit, or the
/// zero-fuel mass plus the usable capacity less the taxi fuel when the tanks
/// are the tighter bound. Taxi fuel is loaded but burned before brake
/// release, so it occupies tank volume without reaching the takeoff mass.
fn admissible_takeoff_mass_ceiling(
    config: &AlasConfig,
    zero_fuel_mass_kg: f64,
    limits: &DispatchLimits,
    model: &dyn FuelBurnModel,
) -> Result<f64, String> {
    let mut ceiling_kg = limits.mtow_kg;
    if let Some(capacity_kg) = limits.usable_capacity_kg {
        let taxi_kg = model
            .taxi_fuel_flow_kg_s()
            .map_err(|error| format!("taxi fuel flow is unavailable: {error}"))?
            * config.fuel_policy.taxi_time_min
            * 60.0;
        ceiling_kg = ceiling_kg.min(zero_fuel_mass_kg + capacity_kg - taxi_kg);
    }
    if !ceiling_kg.is_finite() || ceiling_kg <= zero_fuel_mass_kg {
        return Err(format!(
            "no fuel can be carried: ceiling {ceiling_kg:.1} kg against zero-fuel mass {zero_fuel_mass_kg:.1} kg"
        ));
    }
    Ok(ceiling_kg)
}

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_model::report_route_mission_model;
    use crate::full_analysis::FullAnalysis;
    use alas_config::airports::get as get_airport;

    use alas_mass::payload_range::max_range_with_reserves;
    use alas_opt::mdo::PlannedTrips;

    /// Every registered aircraft, bound to its own sized candidate, flies the
    /// requested route as an off-design mission and reports the design
    /// mission it was sized on beside it:
    /// - over the design range the route is the design mission, so the flown
    ///   mass is the closure's to the dispatch's settling tolerance;
    /// - an airway 4 % longer burns more trip fuel and leaves less reserve
    ///   margin, instead of being priced as the design mission;
    /// - a route a tenth longer than the aircraft's reserve-inclusive range
    ///   at its takeoff-mass limit with full payload is a hard shortfall.
    #[test]
    fn every_preset_flies_its_route_off_design_and_reports_the_design_mission() {
        for name in alas_config::presets::available() {
            let preset = alas_config::presets::get(name).unwrap();
            let mut config =
                AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
            config.structures.enabled = false;
            let assessment = alas_opt::assess_product_candidate(&config, &preset.design_vector)
                .unwrap_or_else(|error| panic!("{name}: assessment: {error}"));
            let sized = &assessment.sized;
            let report = FullAnalysis::new(config.clone())
                .run_sized_candidate(&assessment.resolved.design, sized)
                .unwrap_or_else(|error| panic!("{name}: report: {error}"));
            let origin = get_airport(&config.departure_airport).unwrap();
            let destination = get_airport(&config.arrival_airport).unwrap();
            let loading = plan_fuel_loading(&config, &report.design, &report);
            let fly = |range_m: f64| {
                let request = build_mission_request(&config, origin, destination, range_m);
                let case = select_load_case(&config, &report, &loading, &request)
                    .unwrap_or_else(|error| panic!("{name}: load case: {error}"));
                let LoadCaseSelection::PolicyClosure(closure) = &case.selection else {
                    panic!("{name}: the policy load case is flown: {case:?}");
                };
                assert_eq!(closure.route_distance_m, range_m);
                let design = closure.design_mission.expect("a sized report");
                assert_eq!(design.range_m, sized.design_range_m);
                assert_eq!(design.takeoff_mass_kg, sized.dispatch.takeoff_mass_kg);
                assert_eq!(design.trip_fuel_kg, sized.design_mission_trip_fuel_kg);
                (case.clone(), closure.as_ref().clone())
            };
            // old: the sizing tolerance for every preset -> 0.1 % of the sized
            // takeoff mass for the B747-400 only: the B747-400 generic cabin seats 330 of its 400 passengers (42
            // business, 288 economy; the 24-first/74-business upper-deck layout
            // is not representable), so the report re-prices furnishings and
            // services for the seated cabin (-163.7 kg of 195.6 t OEW, 0.08 %)
            // while the one-pass fixed-requirement sizing keeps the 400-seat
            // percent-mix terms
            // (234 kg of 344 t takeoff mass through the fuel fraction).
            let tolerance_kg = if name == "B747-400" {
                1.0e-3 * sized.dispatch.takeoff_mass_kg
            } else {
                config.optimizer.objective.sizing_tolerance_kg
            };

            let (on_design, design_case) = fly(sized.design_range_m);
            if design_case.offloaded_payload_kg > 0.0 {
                // A design mission that lands above the maximum landing mass
                // (a freighter at its MZFW) is flown offloaded onto it
                // (`landing_limit`), lighter than the closure.
                assert!(
                    !design_case.analytic.landing_mass_exceeds_mlw
                        && on_design.takeoff_mass_kg < sized.dispatch.takeoff_mass_kg,
                    "{name}: offloaded design-range dispatch {design_case:?}"
                );
            } else {
                assert!(
                    (on_design.takeoff_mass_kg - sized.dispatch.takeoff_mass_kg).abs()
                        < tolerance_kg,
                    "{name}: design-range takeoff mass {} kg vs sized {} kg",
                    on_design.takeoff_mass_kg,
                    sized.dispatch.takeoff_mass_kg
                );
            }

            let (longer, longer_case) = fly(1.04 * sized.design_range_m);
            assert!(
                longer_case.plan.trip.kg > design_case.plan.trip.kg,
                "{name}: trip {} kg over the longer airway vs {} kg",
                longer_case.plan.trip.kg,
                design_case.plan.trip.kg
            );
            assert!(longer.takeoff_mass_kg >= on_design.takeoff_mass_kg);
            assert!(longer_case.reserve_margin_kg < design_case.reserve_margin_kg);

            let taxi_kg = report_route_mission_model(&config, &report)
                .unwrap()
                .taxi_fuel_flow_kg_s()
                .unwrap()
                * config.fuel_policy.taxi_time_min
                * 60.0;
            let mtow_kg = aircraft_mtow_kg(&config, &report);
            let capacity_kg = loading.usable_capacity.capacity_kg.unwrap_or(f64::INFINITY);
            let fuel_kg = (mtow_kg - loading.zero_fuel_mass_kg).min(capacity_kg - taxi_kg);
            let model = report_route_mission_model(&config, &report).unwrap();
            let capability = max_range_with_reserves(
                loading.zero_fuel_mass_kg + fuel_kg,
                fuel_kg,
                &config.fuel_policy,
                &PlannedTrips(&model),
                500.0,
            );
            let (_, beyond_case) = fly(1.1 * capability.range_m);
            assert!(
                beyond_case.shortfall_kg > 0.0 && beyond_case.reserve_margin_kg < 0.0,
                "{name}: {:.0} nmi beyond the {:.0} nmi capability: {beyond_case:?}",
                1.1 * capability.range_m / 1_852.0,
                capability.range_m / 1_852.0
            );
            // A positive shortfall is the hard `ReserveFuelShortfall`
            // finding (`feasibility::dispatch::tests`).
        }
    }
}
