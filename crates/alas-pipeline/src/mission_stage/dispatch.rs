// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Choosing the takeoff mass the route is flown at.
//!
//! An operator flies the fuel the policy requires for the route: taxi, trip,
//! contingency, alternate and final reserve, and no more, and that
//! requirement depends on the takeoff mass it produces. This module solves
//! that fixed point on the report's fuel model
//! ([`crate::fuel_model::report_mission_model`]), the segment mission model
//! the sizing closure flew, over the requested route.
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
//! (`mission_stage::evaluate`); its trip is telemetry ([`NativeTripCheck`]),
//! never a fuel source and not a regression bound on the segment model: the
//! two do not share aerodynamics (the native flight trims a vortex-lattice
//! lift surrogate with its own drag build-up, the model flies the closure's
//! trimmed drag table) nor a profile (the native schedule has no frozen
//! cruise levels or step climbs). Measured native-minus-model trip at the charted
//! routes after the fuel model unification: A320 +2.2 %, A220 +6.9 %, A340
//! +11.5 %, A380 +4.0 %, B787 +12.0 %, DC-10 +2.1 %, ATR +5.1 %.

use alas_config::AlasConfig;
use alas_mass::dispatch::{DispatchLimits, DispatchSolution, DispatchStatus};
use alas_mass::fuel_plan::{FuelBurnModel, FuelPlan};
use alas_mission::MissionRequest;
use alas_opt::mdo::solve_planned_dispatch;

use crate::feasibility::FuelLoadingAssessment;
use crate::fuel_model::report_mission_model;
use crate::full_analysis::AnalysisReport;

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
}

impl SelectedLoadCase {
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

/// Select the load case for a route.
///
/// The route is always priced on the segment mission model, so its trip
/// plus reserves are checked against the loadable fuel whichever load is
/// flown. `fuel_loading` is the load case before the policy is applied; it
/// is flown when the policy load case is switched off (carrying the route
/// check beside it) or cannot be priced (carrying the reason).
pub(super) fn select_load_case(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    request: &MissionRequest,
) -> Result<SelectedLoadCase, String> {
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    let maximum =
        |reason: Option<String>, route_check: Option<Box<PolicyClosureCase>>| SelectedLoadCase {
            takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
            zero_fuel_mass_kg,
            native_error: None,
            selection: LoadCaseSelection::MaximumAvailableFuel {
                reason,
                route_check,
            },
        };
    Ok(match price_route(config, report, fuel_loading, request)? {
        Ok((takeoff_mass_kg, case)) if config.fuel_policy.fly_policy_load_case => {
            SelectedLoadCase {
                takeoff_mass_kg,
                zero_fuel_mass_kg,
                native_error: None,
                selection: LoadCaseSelection::PolicyClosure(case),
            }
        }
        Ok((_, case)) => maximum(None, Some(case)),
        Err(reason) => maximum(Some(reason), None),
    })
}

/// The route's dispatch on the segment mission model: the admissible
/// takeoff mass and the policy case, or why it could not be priced.
fn price_route(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    request: &MissionRequest,
) -> Result<Result<(f64, Box<PolicyClosureCase>), String>, String> {
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    let model = match report_mission_model(config, report) {
        Ok(model) => model,
        Err(error) => return Ok(Err(error)),
    };
    let sized = report.fuel.sized_fuel();
    let limits = DispatchLimits {
        mtow_kg: aircraft_mtow_kg(config, report),
        mzfw_kg: None,
        mlw_kg: Some(crate::feasibility::landing_mass_limit_kg(config, report)),
        usable_capacity_kg: fuel_loading.usable_capacity.capacity_kg,
    };
    // The requested route, which the native flight flies too. For a sized
    // aircraft it is an off-design mission: pricing the design range instead
    // let an airway longer than it consume reserves while the dispatch still
    // reported convergence.
    let range_m = request.route_distance_m;
    let max_iterations = config.optimizer.objective.sizing_max_iterations.max(1) as usize;
    let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
    // The dispatch recovers internally from a model failure anywhere in its
    // own search by bisecting toward the zero-fuel mass
    // (`alas_mass::dispatch`); `limits` is never adjusted here, so a
    // `ModelFailed` means the model could not be evaluated anywhere between
    // the zero-fuel mass and its search ceiling. The trip is flown on a plan
    // frozen for this mission (`alas_opt::mdo::solve_planned_dispatch`),
    // planned first at the closure's takeoff mass for a sized report, as the
    // closure's own last pass did, and at the analysed takeoff mass
    // otherwise.
    let planning_mass_kg = sized.map_or(fuel_loading.analyzed_takeoff_mass_kg, |sized| {
        sized.takeoff_mass_kg
    });
    // An under-resolved trip (or a cancelled run) leaves the policy unpriced
    // rather than flown unfrozen.
    let solution = match solve_planned_dispatch(
        &model,
        zero_fuel_mass_kg,
        planning_mass_kg,
        range_m,
        &config.fuel_policy,
        &limits,
        max_iterations,
        tolerance_kg,
    ) {
        Ok(solution) => solution,
        Err(error) => return Ok(Err(error.to_string())),
    };
    if let DispatchStatus::ModelFailed(reason) = &solution.status {
        return Ok(Err(reason.clone()));
    }
    // Fuel above what the limits admit is a finding, not a load: the flight
    // is flown at the admissible mass and the shortfall is reported. The
    // margin is the route's trip plus reserves against the loadable fuel; a
    // deficit within the dispatch's own settling tolerance is the fixed
    // point's resolution, not a shortfall (the solver reports
    // `MtowLimited` on the same bound).
    let ceiling_kg = admissible_takeoff_mass_ceiling(config, zero_fuel_mass_kg, &limits, &model)?;
    let reserve_margin_kg = ceiling_kg - zero_fuel_mass_kg - solution.plan.takeoff_fuel_kg();
    let shortfall_kg = if -reserve_margin_kg > tolerance_kg {
        -reserve_margin_kg
    } else {
        0.0
    };
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
        }),
    )))
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
    use crate::feasibility::plan_fuel_loading;
    use crate::full_analysis::FullAnalysis;
    use alas_config::airports::get as get_airport;
    use alas_mission::build_mission_request;

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
            let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;

            let (on_design, design_case) = fly(sized.design_range_m);
            assert!(
                (on_design.takeoff_mass_kg - sized.dispatch.takeoff_mass_kg).abs() < tolerance_kg,
                "{name}: design-range takeoff mass {} kg vs sized {} kg",
                on_design.takeoff_mass_kg,
                sized.dispatch.takeoff_mass_kg
            );

            let (longer, longer_case) = fly(1.04 * sized.design_range_m);
            assert!(
                longer_case.plan.trip.kg > design_case.plan.trip.kg,
                "{name}: trip {} kg over the longer airway vs {} kg",
                longer_case.plan.trip.kg,
                design_case.plan.trip.kg
            );
            assert!(longer.takeoff_mass_kg >= on_design.takeoff_mass_kg);
            assert!(longer_case.reserve_margin_kg < design_case.reserve_margin_kg);

            let taxi_kg = report_mission_model(&config, &report)
                .unwrap()
                .taxi_fuel_flow_kg_s()
                .unwrap()
                * config.fuel_policy.taxi_time_min
                * 60.0;
            let mtow_kg = aircraft_mtow_kg(&config, &report);
            let capacity_kg = loading.usable_capacity.capacity_kg.unwrap_or(f64::INFINITY);
            let fuel_kg = (mtow_kg - loading.zero_fuel_mass_kg).min(capacity_kg - taxi_kg);
            let model = report_mission_model(&config, &report).unwrap();
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
