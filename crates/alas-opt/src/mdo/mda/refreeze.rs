// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Freezing a pass's mission plan, and re-freezing a single-pass closure's
//! plan at the mass it closes on.
//!
//! A single-pass closure (`FixedRequirement`) freezes its plan at the seed
//! mass, the takeoff-mass ceiling, while its dispatch closes far below it. The
//! plan's discrete choices (cruise level, step climbs) are those of the heavy
//! aircraft, so the design mission was flown at a lower level than the same
//! aircraft reaches at the mass it closes on, and a longer route, planned at
//! its own closed mass, burned less trip fuel than the design mission. The
//! plan is re-frozen at the closed mass, as `solve_planned_dispatch` does for
//! a consumer's route, at most [`SINGLE_PASS_REFREEZES`] times (the bound of
//! `planned_mission::PLANNED_DISPATCH_PASSES` less the first solve).

use alas_config::mission::CruiseAltitudePolicy;
use alas_mass::dispatch::{DispatchSolution, DispatchStatus};
use alas_mass::fuel_plan::FuelModelError;

use super::{
    CandidateFailure, FreezeError, SegmentMissionModel, MISSION_STEP_UNCONVERGED,
    SIZING_BUDGET_EXHAUSTED,
};

/// Re-freezes of a single-pass closure's plan at its closed mass.
const SINGLE_PASS_REFREEZES: usize = 2;

/// A pass's mission model and the mass its plan is frozen at.
pub(super) struct Refreeze<'a> {
    /// The pass's un-frozen mission model.
    pub model: &'a SegmentMissionModel,
    /// The cruise-altitude policy the plans are frozen under.
    pub policy: &'a CruiseAltitudePolicy,
    /// Route length, m.
    pub range_m: f64,
    /// The takeoff mass the first plan is frozen at, kg.
    pub frozen_at_kg: f64,
    /// Zero-fuel mass, kg: no plan is tried below it.
    pub zero_fuel_kg: f64,
    /// Mass agreement under which a plan stands, kg.
    pub tolerance_kg: f64,
    /// Whether the closure takes one pass, and so re-freezes at its closed mass.
    pub single_pass: bool,
    /// Whether the sizing work budget is spent.
    pub budget_spent: &'a dyn Fn(&SegmentMissionModel) -> bool,
}

impl Refreeze<'_> {
    /// Freeze the plan at [`Self::frozen_at_kg`] and solve the dispatch on it
    /// with `solve_from(start_kg, model)`, which starts the dispatch iteration
    /// at `start_kg`; a single-pass closure then re-freezes at the mass it
    /// closed on, starting there (from the ceiling the iteration probes masses
    /// the lighter aircraft's plan cannot fly and asks for a re-freeze back
    /// at the heavy mass). Returns the flown model, its solution and the
    /// number of plans frozen.
    ///
    /// # Errors
    ///
    /// A cancelled run, a trip too coarsely resolved to price, or a spent
    /// work budget.
    pub(super) fn solve(
        &self,
        solve_from: impl Fn(f64, &SegmentMissionModel) -> DispatchSolution,
    ) -> Result<(SegmentMissionModel, DispatchSolution, u32), CandidateFailure> {
        let first = self.model.solve_on_descending_plans(
            self.frozen_at_kg,
            self.zero_fuel_kg,
            self.range_m,
            self.policy,
            |model: &SegmentMissionModel, planned_at_kg: f64| solve_from(planned_at_kg, model),
        );
        let (mut pass_model, mut solution, mut freezes) = match first {
            Ok((frozen, solution, _planned_at_kg)) => (frozen, solution, 1),
            Err(FreezeError::Fuel(FuelModelError::Cancelled)) => {
                return Err(CandidateFailure {
                    reason: "cancelled",
                })
            }
            // An under-resolved trip is a numerical failure of the candidate,
            // not a reason to fly the dispatch unfrozen.
            Err(FreezeError::StepUnconverged { .. }) => {
                return Err(CandidateFailure {
                    reason: MISSION_STEP_UNCONVERGED,
                })
            }
            Err(_) if (self.budget_spent)(self.model) => {
                return Err(CandidateFailure {
                    reason: SIZING_BUDGET_EXHAUSTED,
                })
            }
            // No plan can be made at this mass (the trip is not flyable there
            // even with level and climb adaptation): the dispatch brackets its
            // own way to a flyable mass, adapting per flight.
            Err(_) => (
                self.model.clone(),
                solve_from(self.frozen_at_kg, self.model),
                0,
            ),
        };
        for _ in 0..SINGLE_PASS_REFREEZES {
            let settled = matches!(
                solution.status,
                DispatchStatus::Converged
                    | DispatchStatus::MtowLimited { .. }
                    | DispatchStatus::TankLimited { .. }
            );
            let planned_kg = pass_model
                .frozen_plan()
                .map_or(self.frozen_at_kg, |plan| plan.takeoff_mass_kg);
            if !self.single_pass
                || !settled
                || (solution.takeoff_mass_kg - planned_kg).abs() <= self.tolerance_kg
            {
                break;
            }
            let closed_kg = solution.takeoff_mass_kg;
            match self.model.solve_on_frozen_plans(
                closed_kg,
                self.range_m,
                self.policy,
                |model: &SegmentMissionModel| solve_from(closed_kg, model),
            ) {
                Ok((refrozen, resolved)) => {
                    freezes += 1;
                    (pass_model, solution) = (refrozen, resolved);
                }
                Err(FreezeError::Fuel(FuelModelError::Cancelled)) => {
                    return Err(CandidateFailure {
                        reason: "cancelled",
                    })
                }
                Err(FreezeError::StepUnconverged { .. }) => {
                    return Err(CandidateFailure {
                        reason: MISSION_STEP_UNCONVERGED,
                    })
                }
                // Keep the plan frozen at the seed.
                Err(_) => break,
            }
        }
        Ok((pass_model, solution, freezes))
    }
}
