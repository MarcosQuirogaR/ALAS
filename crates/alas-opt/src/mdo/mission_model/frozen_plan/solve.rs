// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Solving a dispatch on frozen plans: the re-freeze loop and the descent to
//! a lighter plan where none can be made at the takeoff-mass ceiling.

use alas_config::mission::CruiseAltitudePolicy;
use alas_mass::fuel_plan::FuelModelError;

use super::{FreezeError, SegmentMissionModel, MAX_REFREEZES};

/// Ratio between successive masses [`SegmentMissionModel::solve_on_descending_plans`]
/// tries when no plan can be made at the one before (engineering choice: a
/// step of 7 % of the takeoff mass reaches the closed mass of a wide-body
/// whose seed is its takeoff-mass ceiling in about five trials, each a
/// failed adaptation that costs a few dozen flights, where the unfrozen
/// dispatch it replaces spends thousands).
const FREEZE_DESCENT_FACTOR: f64 = 0.93;

/// Most masses [`SegmentMissionModel::solve_on_descending_plans`] tries below
/// the first before it gives the caller the first mass's error.
const MAX_FREEZE_DESCENTS: usize = 10;

impl SegmentMissionModel {
    /// Run `solve` on a plan frozen at `takeoff_mass_kg` over `range_m`
    /// under `policy`. When a trip of that plan asked for a re-freeze
    /// ([`Self::refreeze_request_kg`]), `solve` restarts from nothing on a
    /// new plan frozen at the requested mass, at most [`MAX_REFREEZES`]
    /// times, so every answer `solve` saw, and any memo it kept by mass,
    /// comes from the one plan returned with its result. A re-freeze that
    /// cannot be made keeps the last result, which is consistent with its
    /// own plan.
    ///
    /// # Errors
    ///
    /// The first freeze's error; a re-freeze's cancellation or
    /// [`FreezeError::StepUnconverged`].
    pub fn solve_on_frozen_plans<T>(
        &self,
        takeoff_mass_kg: f64,
        range_m: f64,
        policy: &CruiseAltitudePolicy,
        mut solve: impl FnMut(&Self) -> T,
    ) -> Result<(Self, T), FreezeError> {
        let plan = self.freeze_plan(takeoff_mass_kg, range_m, policy)?;
        let mut model = self.clone().with_frozen_plan(plan);
        // A solve that will be repeated on a re-frozen plan, if its plan asks
        // for one, stops at that request: its answer is discarded, so
        // adapting every later probe of it is wasted work. The solve that
        // follows the last permitted re-freeze is never stopped.
        model.stop_solve_at_refreeze_request(MAX_REFREEZES > 0);
        let mut solution = solve(&model);
        for refreeze in 0..MAX_REFREEZES {
            let Some(mass_kg) = model.refreeze_request_kg() else {
                break;
            };
            let plan = match self.freeze_plan(mass_kg, range_m, policy) {
                Ok(plan) => plan,
                Err(
                    error @ (FreezeError::StepUnconverged { .. }
                    | FreezeError::Fuel(FuelModelError::Cancelled)),
                ) => return Err(error),
                Err(FreezeError::Fuel(_)) => {
                    // No re-freeze can be made: this solve's answer is the
                    // answer, so it must be the whole one.
                    model.stop_solve_at_refreeze_request(false);
                    solution = solve(&model);
                    break;
                }
            };
            model = self.clone().with_frozen_plan(plan);
            model.stop_solve_at_refreeze_request(refreeze + 1 < MAX_REFREEZES);
            solution = solve(&model);
        }
        model.stop_solve_at_refreeze_request(false);
        Ok((model, solution))
    }

    /// As [`Self::solve_on_frozen_plans`], but where no plan can be made at
    /// `takeoff_mass_kg` the solve is run on a plan made at a lighter mass
    /// instead of on no plan at all.
    ///
    /// A closure that starts at the takeoff-mass ceiling meets a heavy
    /// aircraft there that cannot fly the configured climb schedule even at
    /// the gentlest commanded rate, so no plan exists at the seed although
    /// one exists at the mass the closure settles on. Flown unfrozen, the
    /// dispatch then re-adapts every trip it probes (a rate revision and a
    /// level search of tens of flights each), and because a discrete choice
    /// flips with mass the trip fuel is not a function of mass: the
    /// iteration cycles between two masses and never closes, at thousands of
    /// flights per candidate. Here the plan is made at
    /// [`FREEZE_DESCENT_FACTOR`] times the previous mass, at most
    /// [`MAX_FREEZE_DESCENTS`] times and not below `floor_kg` (the zero-fuel
    /// mass), and `solve(model, planned_at_kg)` starts its dispatch at the
    /// mass the plan was made at; a trip the plan cannot fly at a heavier
    /// mass asks for a re-freeze there, as for any plan.
    ///
    /// Returns the model, the result and the mass the first plan was made at,
    /// which is `takeoff_mass_kg` whenever a plan can be made there (the
    /// answer is then exactly that of [`Self::solve_on_frozen_plans`]).
    ///
    /// # Errors
    ///
    /// As [`Self::solve_on_frozen_plans`]; when no mass admits a plan, the
    /// error at `takeoff_mass_kg`.
    pub fn solve_on_descending_plans<T>(
        &self,
        takeoff_mass_kg: f64,
        floor_kg: f64,
        range_m: f64,
        policy: &CruiseAltitudePolicy,
        mut solve: impl FnMut(&Self, f64) -> T,
    ) -> Result<(Self, T, f64), FreezeError> {
        let first_error =
            match self.solve_on_frozen_plans(takeoff_mass_kg, range_m, policy, |model| {
                solve(model, takeoff_mass_kg)
            }) {
                Ok((model, solution)) => return Ok((model, solution, takeoff_mass_kg)),
                // A cancelled run, or a trip too coarsely resolved to price at the
                // caller's own mass, is the caller's to handle, not a reason to
                // fly another mass.
                Err(
                    error @ (FreezeError::StepUnconverged { .. }
                    | FreezeError::Fuel(FuelModelError::Cancelled)),
                ) => return Err(error),
                Err(error) => error,
            };
        let mut planned_at_kg = takeoff_mass_kg;
        for _ in 0..MAX_FREEZE_DESCENTS {
            planned_at_kg *= FREEZE_DESCENT_FACTOR;
            if planned_at_kg <= floor_kg || planned_at_kg.is_nan() {
                break;
            }
            match self.solve_on_frozen_plans(planned_at_kg, range_m, policy, |model| {
                solve(model, planned_at_kg)
            }) {
                Ok((model, solution)) => return Ok((model, solution, planned_at_kg)),
                Err(error @ FreezeError::Fuel(FuelModelError::Cancelled)) => return Err(error),
                // This lighter mass has no usable plan either; try the next.
                Err(_) => {}
            }
        }
        Err(first_error)
    }
}
