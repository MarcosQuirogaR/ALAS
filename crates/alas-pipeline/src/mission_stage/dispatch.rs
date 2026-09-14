// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Choosing the takeoff mass the route is flown at.
//!
//! The frozen mission burned whatever fuel the takeoff-mass closure left
//! over, which answers "can the tanks reach the destination" and nothing
//! else. An operator flies the fuel the policy requires for the route --
//! taxi, trip, contingency, alternate and final reserve -- and no more, and
//! that requirement depends on the takeoff mass it produces. This module
//! solves that fixed point against the native mission: the analytic model
//! supplies a first closure cheaply, the native mission then re-flies the
//! route at the estimated takeoff mass and the policy is re-priced with the
//! flown trip until the mass stops moving. The result is bounded by the
//! takeoff-mass limit and by the usable tank capacity, and a route that
//! cannot carry its policy fuel is reported as such rather than flown short.

use alas_config::AlasConfig;
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchSolution, DispatchStatus};
use alas_mass::fuel_plan::{FuelBurnModel, FuelPlan, LegEstimate};
use alas_mass::fuel_policy::plan_fuel;
use alas_mission::segments::SegmentSpec;
use alas_mission::MissionRequest;

use crate::feasibility::FuelLoadingAssessment;
use crate::fuel_model::{breguet_from_report, FlownTripModel};
use crate::full_analysis::AnalysisReport;

use super::flight::fly_with_guidance;
use super::MissionAnalyses;

/// Native refinements after the analytic closure. The fixed point contracts
/// by the trip fuel's sensitivity to the takeoff mass, about a quarter per
/// flight on the registered presets, so the analytic seed's error of a few
/// percent is inside the tolerance after four to six flights.
const MAX_NATIVE_REFINEMENTS: usize = 8;

/// Absolute floor of the takeoff-mass change below which the native closure
/// is taken as settled.
const NATIVE_TOLERANCE_KG: f64 = 5.0;

/// Relative part of the settling tolerance: one part in ten thousand of the
/// takeoff mass, an order of magnitude below the fuel model's own fidelity
/// and, at the contraction above, a residual error under a third of it.
const NATIVE_TOLERANCE_FRACTION: f64 = 1.0e-4;

/// The policy closure that selected the flown load case.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyClosureCase {
    /// The plan at the selected mass, with the flown trip.
    pub plan: FuelPlan,
    /// The analytic closure the native refinement started from.
    pub analytic: DispatchSolution,
    /// Native flights used by the refinement.
    pub native_flights: usize,
    /// Whether the native refinement settled within tolerance.
    pub converged: bool,
    /// Fuel the route needs beyond what the limits allow, kg.
    pub shortfall_kg: f64,
}

/// How the load case the mission flies was selected.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadCaseSelection {
    /// The maximum-available-fuel case of the frozen mission; the policy is
    /// disabled or cannot be priced.
    MaximumAvailableFuel {
        /// Why the policy closure was not used, when it was attempted.
        reason: Option<String>,
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
}

/// Select the load case for a route.
///
/// `fuel_loading` is the takeoff-mass-closure case the frozen mission flew;
/// it remains the answer when the policy is switched off, when the scheme
/// carries no reserves, or when the analytic model cannot be built. The
/// native refinement mutates `analyses` masses as it flies and leaves them
/// at the selected case.
pub(super) fn select_load_case(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    analyses: &mut MissionAnalyses,
    request: &MissionRequest,
    schedule: &[SegmentSpec],
) -> Result<SelectedLoadCase, String> {
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    let maximum = |reason: Option<String>| SelectedLoadCase {
        takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
        zero_fuel_mass_kg,
        selection: LoadCaseSelection::MaximumAvailableFuel { reason },
    };
    if !config.fuel_policy.fly_policy_load_case {
        return Ok(maximum(None));
    }
    let analytic = match breguet_from_report(config, report) {
        Ok(model) => model,
        Err(error) => return Ok(maximum(Some(error))),
    };
    let limits = DispatchLimits {
        mtow_kg: config.requirements.mtow_kg,
        mzfw_kg: None,
        mlw_kg: Some(config.landing_mass_limit_kg(config.requirements.mtow_kg)),
        usable_capacity_kg: fuel_loading.usable_capacity.capacity_kg,
    };
    let range_m = request.route_distance_m;
    // `alas_mass::dispatch::solve_dispatch` recovers internally from a
    // model failure encountered anywhere in its own search (an overshoot
    // during the Picard iteration, or at its own MTOW boundary check) by
    // bisecting toward the zero-fuel mass; see
    // `alas-mass/src/dispatch.rs::evaluate_bracketed`. `limits` is passed
    // straight through and is never adjusted here, so a `ModelFailed` below
    // means the model could not be evaluated anywhere between the zero-fuel
    // mass and its search ceiling, not merely at one starting guess.
    let analytic_solution = solve_dispatch(
        zero_fuel_mass_kg,
        range_m,
        &config.fuel_policy,
        &analytic,
        &limits,
        config.optimizer.objective.sizing_max_iterations.max(1) as usize,
        config.optimizer.objective.sizing_tolerance_kg,
    );
    if let DispatchStatus::ModelFailed(reason) = &analytic_solution.status {
        return Ok(maximum(Some(reason.clone())));
    }

    // Fuel above what the limits admit is a finding, not a load: the flight
    // is flown at the admissible mass and the shortfall is reported.
    let ceiling_kg =
        admissible_takeoff_mass_ceiling(config, zero_fuel_mass_kg, &limits, &analytic)?;
    let mut takeoff_mass_kg = analytic_solution.takeoff_mass_kg.min(ceiling_kg);
    let mut plan = analytic_solution.plan;
    let mut converged = false;
    let mut native_flights = 0;
    let mut required_kg = takeoff_mass_kg;
    for _ in 0..MAX_NATIVE_REFINEMENTS {
        analyses.takeoff_mass_kg = takeoff_mass_kg;
        analyses.minimum_mass_kg = None;
        native_flights += 1;
        let flown = match fly_with_guidance(schedule.to_vec(), request, analyses) {
            Ok(result) => result,
            Err(error) => {
                // A policy closure is an optional refinement of the load
                // case. If the native mission cannot be evaluated at its
                // trial mass, return the last analytically admissible case
                // with an explicit partial-mission reason. The caller still
                // runs the final flight and reports its stopping condition;
                // this is a mission result, never a mass-method fallback.
                analyses.takeoff_mass_kg = fuel_loading.analyzed_takeoff_mass_kg;
                analyses.minimum_mass_kg = Some(zero_fuel_mass_kg);
                return Ok(maximum(Some(format!(
                    "native mission could not evaluate policy closure at {takeoff_mass_kg:.1} kg: {error}"
                ))));
            }
        };
        let Some(summary) = flown.completed_summary() else {
            analyses.takeoff_mass_kg = fuel_loading.analyzed_takeoff_mass_kg;
            analyses.minimum_mass_kg = Some(zero_fuel_mass_kg);
            return Ok(maximum(Some(format!(
                "the route could not be flown at {takeoff_mass_kg:.1} kg during the fuel-policy closure"
            ))));
        };
        let leg = LegEstimate {
            fuel_kg: summary.trip_fuel_kg,
            time_s: flown.block_time_s(),
        };
        let model = FlownTripModel {
            leg,
            analytic: &analytic,
        };
        plan =
            plan_fuel(&config.fuel_policy, &model, takeoff_mass_kg, range_m).map_err(|error| {
                format!("fuel policy could not be priced on the flown trip: {error}")
            })?;
        required_kg = zero_fuel_mass_kg + plan.takeoff_fuel_kg();
        let next_kg = required_kg.min(ceiling_kg);
        let change_kg = next_kg - takeoff_mass_kg;
        takeoff_mass_kg = next_kg;
        let tolerance_kg = NATIVE_TOLERANCE_KG.max(NATIVE_TOLERANCE_FRACTION * next_kg);
        if change_kg.abs() < tolerance_kg {
            converged = true;
            break;
        }
    }
    analyses.takeoff_mass_kg = takeoff_mass_kg;
    analyses.minimum_mass_kg = Some(zero_fuel_mass_kg);
    Ok(SelectedLoadCase {
        takeoff_mass_kg,
        zero_fuel_mass_kg,
        selection: LoadCaseSelection::PolicyClosure(Box::new(PolicyClosureCase {
            plan,
            analytic: analytic_solution,
            native_flights,
            converged,
            shortfall_kg: (required_kg - ceiling_kg).max(0.0),
        })),
    })
}

/// The largest takeoff mass the limits admit: the takeoff-mass limit, or the
/// zero-fuel mass plus the usable capacity less the taxi fuel when the tanks
/// are the tighter bound. Taxi fuel is loaded but burned before brake
/// release, so it occupies tank volume without reaching the takeoff mass.
fn admissible_takeoff_mass_ceiling(
    config: &AlasConfig,
    zero_fuel_mass_kg: f64,
    limits: &DispatchLimits,
    analytic: &dyn FuelBurnModel,
) -> Result<f64, String> {
    let mut ceiling_kg = limits.mtow_kg;
    if let Some(capacity_kg) = limits.usable_capacity_kg {
        let taxi_kg = analytic
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
