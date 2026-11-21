// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The part of the route pricing that the native flight does not change.
//!
//! [`super::price_route`] builds the route's mission model from the report, solves the planned
//! dispatch on it, and then (once the native flight has burned its fuel) solves the dispatch again
//! on the plan with the flown ratios ([`super::NativeScales`]). Everything up to the first solution
//! is a function of the configuration, the report and the request alone, so the sizing passes of
//! one run share it ([`RoutePricing`]) instead of rebuilding the model and the plan on every pass.
//! The values are the ones a rebuild would give: the model and the planned dispatch are
//! deterministic in their inputs.

use alas_config::AlasConfig;
use alas_mass::dispatch::{DispatchLimits, DispatchSolution};
use alas_mission::MissionRequest;
use alas_opt::mdo::solve_planned_dispatch_with_plan;
use alas_opt::SegmentMissionModel;

use super::{aircraft_mtow_kg, FuelLoadingAssessment};
use crate::fuel_model::report_route_mission_model;
use crate::full_analysis::AnalysisReport;

/// The route's mission model, its dispatch limits and the planned dispatch on it before any
/// native-flight scaling.
pub(crate) struct RoutePricing {
    /// The model the dispatch was planned on (it carries the frozen plan).
    pub planned: SegmentMissionModel,
    /// The model before planning (the takeoff-mass ceiling reads the taxi flow from it).
    pub model: SegmentMissionModel,
    pub limits: DispatchLimits,
    /// The dispatch solution before scaling.
    pub solution: DispatchSolution,
    /// The takeoff-mass ceiling, once it was computed.
    pub ceiling_kg: Option<f64>,
}

impl RoutePricing {
    /// Build the model and solve the planned dispatch for `request`: the pricing, or why the
    /// route cannot be priced.
    pub(super) fn solve(
        config: &AlasConfig,
        report: &AnalysisReport,
        fuel_loading: &FuelLoadingAssessment,
        request: &MissionRequest,
    ) -> Result<Self, String> {
        let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
        let model_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::PipelineMissionModel);
        let model = report_route_mission_model(config, report)?;
        drop(model_profile);
        let sized = report.fuel.sized_fuel();
        let limits = DispatchLimits {
            mtow_kg: aircraft_mtow_kg(config, report),
            mzfw_kg: None,
            mlw_kg: Some(crate::feasibility::landing_mass_limit_kg(config, report)),
            usable_capacity_kg: fuel_loading.usable_capacity.capacity_kg,
        };
        // The requested route, which the native flight flies too. For a sized aircraft it is an
        // off-design mission: pricing the design range instead let an airway longer than it
        // consume reserves while the dispatch still reported convergence.
        let range_m = request.route_distance_m;
        let max_iterations = config.optimizer.objective.sizing_max_iterations.max(1) as usize;
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        // The dispatch recovers internally from a model failure anywhere in its own search by
        // bisecting toward the zero-fuel mass (`alas_mass::dispatch`); `limits` is never adjusted
        // here, so a `ModelFailed` means the model could not be evaluated anywhere between the
        // zero-fuel mass and its search ceiling. The trip is flown on a plan frozen for this
        // mission (`alas_opt::mdo::solve_planned_dispatch`), planned first at the closure's
        // takeoff mass for a sized report, as the closure's own last pass did, and at the
        // analysed takeoff mass otherwise.
        let planning_mass_kg = sized.map_or(fuel_loading.analyzed_takeoff_mass_kg, |sized| {
            sized.takeoff_mass_kg
        });
        // An under-resolved trip (or a cancelled run) leaves the policy unpriced rather than
        // flown unfrozen.
        let _dispatch_profile = alas_math::stage_profile::scope(
            alas_math::stage_profile::Stage::PipelinePlannedDispatch,
        );
        let (solution, planned) = solve_planned_dispatch_with_plan(
            &model,
            zero_fuel_mass_kg,
            planning_mass_kg,
            range_m,
            &config.fuel_policy,
            &limits,
            max_iterations,
            tolerance_kg,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            planned,
            model,
            limits,
            solution,
            ceiling_kg: None,
        })
    }
}
