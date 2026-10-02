// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A frozen trip plan for every consumer's own mission.
//!
//! The sizing closure flies its trips on a plan frozen at its own takeoff
//! mass and range ([`SegmentMissionModel::solve_on_frozen_plans`]): the
//! cruise level the [`alas_config::mission::CruiseAltitudePolicy`] picks, the
//! climb revisions, the step-climb positions and a Richardson-verified step
//! count. A trip no frozen plan covers holds its initial level, because a
//! step position chosen anew at every mass would make trip fuel jump inside
//! a dispatch closure. A consumer that priced another mission on the carried
//! model (a route other than the design range, a payload-range corner, the
//! dispatch of a report with no sized candidate) therefore flew a schedule
//! without the step climbs the closure flies, under the same policy and drag.
//!
//! Here each such mission gets a plan of its own, made outside its mass
//! closure exactly as the closure makes its own: [`solve_planned_dispatch`]
//! for a dispatch (the plan is frozen at a planning mass and re-frozen at
//! the closed mass until it settles), and [`PlannedTrips`] for a fixed-mass range search
//! (every probed trip is flown on the plan frozen for that takeoff mass and
//! range).

use alas_config::FuelPolicyConfig;
use alas_mass::dispatch::{
    solve_dispatch_with_initial_guess, DispatchLimits, DispatchSolution, DispatchStatus,
};
use alas_mass::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};

use crate::mdo::mission_model::{FreezeError, SegmentMissionModel};

/// Largest number of dispatch solves per planned mission: one on the plan
/// frozen at the planning mass and up to two on plans re-frozen at the
/// previous solve's closed mass. Two re-freezes price the dispatch within
/// 0.1 % of fuel of the closure re-frozen until its mass stops moving on the
/// A320-200, B787-9 and ATR72-600, even from the unstepped closure
/// (`mission_model::physics_tests::
/// a_refrozen_plan_prices_dispatch_within_a_tenth_of_a_percent`).
const PLANNED_DISPATCH_PASSES: usize = 3;

/// `model` with a plan frozen for the trip of `takeoff_mass_kg` over
/// `range_m` under the model's own cruise-altitude policy. A plan the model
/// already carries for that range (the sizing closure's design-mission plan)
/// is kept, so the design mission is priced on the closure's own plan.
///
/// # Errors
///
/// As [`SegmentMissionModel::freeze_plan`]: the trip cannot be flown at that
/// mass, its step count misses the Richardson bound, or the run was
/// cancelled.
pub fn plan_for_mission(
    model: &SegmentMissionModel,
    takeoff_mass_kg: f64,
    range_m: f64,
) -> Result<SegmentMissionModel, FreezeError> {
    if covers(model, range_m) {
        return Ok(model.clone());
    }
    let policy = model.profile.cruise_altitude_policy;
    let plan = model.freeze_plan(takeoff_mass_kg, range_m, &policy)?;
    Ok(model.clone().with_frozen_plan(plan))
}

/// Whether `model` carries a frozen plan for exactly `range_m`, the range
/// test the frozen trip itself applies.
fn covers(model: &SegmentMissionModel, range_m: f64) -> bool {
    model
        .frozen_plan()
        .is_some_and(|plan| plan.range_m.to_bits() == range_m.to_bits())
}

/// The dispatch of `zero_fuel_mass_kg` over `range_m` on `model`, each pass
/// solved on plans frozen at the pass's planning mass
/// ([`SegmentMissionModel::solve_on_frozen_plans`], which restarts the solve
/// on a re-frozen plan where a trip asks for one): `initial_guess_kg`
/// first, then the closed mass of the previous pass while it moves by more
/// than `tolerance_kg`. A plan cannot be made where the trip is not flyable
/// even with level and climb adaptation; that pass then flies unfrozen and
/// the dispatch brackets toward a flyable mass, as a sizing pass does. A
/// plan the model carries for `range_m` is flown as it is.
///
/// The arguments are those of [`solve_dispatch_with_initial_guess`].
///
/// # Errors
///
/// [`FreezeError::StepUnconverged`] when a plan's step count misses the
/// Richardson bound, and the cancellation of the run; an under-resolved trip
/// is not flown unfrozen instead.
#[allow(clippy::too_many_arguments)] // the dispatch solver's own inputs
pub fn solve_planned_dispatch(
    model: &SegmentMissionModel,
    zero_fuel_mass_kg: f64,
    initial_guess_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> Result<DispatchSolution, FreezeError> {
    let carried = covers(model, range_m);
    let altitude_policy = model.profile.cruise_altitude_policy;
    let mut planning_mass_kg = initial_guess_kg;
    let mut pass = 0;
    loop {
        pass += 1;
        let solve = |planned: &SegmentMissionModel| {
            solve_dispatch_with_initial_guess(
                zero_fuel_mass_kg,
                planning_mass_kg,
                range_m,
                policy,
                planned,
                limits,
                max_iterations,
                tolerance_kg,
            )
        };
        let solution = if carried {
            solve(model)
        } else {
            match model.solve_on_frozen_plans(planning_mass_kg, range_m, &altitude_policy, solve) {
                Ok((_, solution)) => solution,
                Err(
                    error @ (FreezeError::StepUnconverged { .. }
                    | FreezeError::Fuel(FuelModelError::Cancelled)),
                ) => return Err(error),
                Err(FreezeError::Fuel(_)) => solve(model),
            }
        };
        let settled = matches!(
            solution.status,
            DispatchStatus::Converged
                | DispatchStatus::MtowLimited { .. }
                | DispatchStatus::TankLimited { .. }
        );
        if carried
            || !settled
            || pass >= PLANNED_DISPATCH_PASSES
            || (solution.takeoff_mass_kg - planning_mass_kg).abs() <= tolerance_kg
        {
            return Ok(solution);
        }
        planning_mass_kg = solution.takeoff_mass_kg;
    }
}

/// `model` as a burn model whose every trip is flown on the plan frozen for
/// that trip's own takeoff mass and range ([`plan_for_mission`]): the
/// mission a fixed-mass range search probes at each distance. Diversions,
/// holding and taxi are the model's own, as in the sizing closure.
#[derive(Debug, Clone, Copy)]
pub struct PlannedTrips<'a>(pub &'a SegmentMissionModel);

impl FuelBurnModel for PlannedTrips<'_> {
    fn deterministic_for_dispatch(&self) -> bool {
        // A plan is a function of the trip's mass and range alone.
        self.0.deterministic_for_dispatch()
    }

    fn check_cancellation(&self) -> Result<(), FuelModelError> {
        self.0.check_cancellation()
    }

    fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
        let planned =
            plan_for_mission(self.0, takeoff_mass_kg, range_m).map_err(|error| match error {
                FreezeError::Fuel(error) => error,
                unconverged @ FreezeError::StepUnconverged { .. } => {
                    FuelModelError::NotConverged(unconverged.to_string())
                }
            })?;
        planned.trip(takeoff_mass_kg, range_m)
    }

    fn diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<LegEstimate, FuelModelError> {
        self.0.diversion(start_mass_kg, distance_m)
    }

    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError> {
        self.0.holding_fuel_flow_kg_s(mass_kg, altitude_m)
    }

    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
        self.0.cruise_fuel_flow_kg_s(mass_kg)
    }

    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
        self.0.taxi_fuel_flow_kg_s()
    }
}

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::candidate_mission_model;
    use alas_config::AlasConfig;

    /// The sized A340-300 prices its design mission on the closure's own
    /// plan, and a route half as long again on a plan frozen for that route,
    /// which climbs in steps under `OptimumStep` as the closure's would: the
    /// unfrozen trip held its initial level for 4,770 nmi.
    #[test]
    fn every_mission_is_flown_on_a_plan_of_its_own() {
        let name = "A340-300";
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = crate::mdo::assess_product_candidate(&config, &design).unwrap();
        let sized = &assessment.sized;
        let model = candidate_mission_model(&config, &sized.fuel_artifacts).unwrap();
        let limits = DispatchLimits {
            mtow_kg: config.requirements.mtow_kg,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        let dispatch = |range_m: f64| {
            solve_planned_dispatch(
                &model,
                sized.zero_fuel_mass_kg,
                sized.dispatch.takeoff_mass_kg,
                range_m,
                &config.fuel_policy,
                &limits,
                50,
                tolerance_kg,
            )
            .unwrap()
        };
        let design_mission = dispatch(sized.design_range_m);
        assert!(
            (design_mission.takeoff_mass_kg - sized.dispatch.takeoff_mass_kg).abs() < tolerance_kg,
            "design mission {} kg against the closure's {} kg",
            design_mission.takeoff_mass_kg,
            sized.dispatch.takeoff_mass_kg
        );
        let range_m = 1.5 * sized.design_range_m;
        let route = dispatch(range_m);
        assert_eq!(route.status, DispatchStatus::Converged);
        let planned = plan_for_mission(&model, route.takeoff_mass_kg, range_m).unwrap();
        let plan = planned.frozen_plan().unwrap();
        assert_eq!(plan.range_m, range_m);
        assert!(
            !plan.step_positions.is_empty(),
            "the route plan climbs in steps: {plan:?}"
        );
        assert!(route.plan.trip.kg > design_mission.plan.trip.kg);
    }
}
