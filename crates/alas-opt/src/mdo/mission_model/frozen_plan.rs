// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Frozen trip plans and the work budget.
//!
//! A dispatch closure flies the same route at a sequence of masses that
//! differ by a few hundred kilograms. Re-running the level search, the climb
//! revision and the step-climb decisions at every one of them costs most of
//! a candidate's mission time and makes trip fuel jump wherever a discrete
//! choice flips. [`SegmentMissionModel::freeze_plan`] makes those choices
//! once, at one mass, together with an integration step count whose
//! Richardson error estimate meets [`RICHARDSON_TOLERANCE`];
//! [`SegmentMissionModel::with_frozen_plan`] then flies every trip of that
//! range with them, re-pricing fuel at the actual mass. A plan is never
//! changed once made, so a model answers every mass the same way however
//! often it is asked; a plan the aircraft cannot fly at some mass records
//! that mass, and [`SegmentMissionModel::solve_on_frozen_plans`] restarts the
//! whole solve on a new plan frozen there, at most [`MAX_REFREEZES`] times.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use alas_config::mission::CruiseAltitudePolicy;
use alas_mass::fuel_plan::FuelModelError;

use super::adapt::{into_model_error, Adapted};
use super::integrate::{FlownLeg, FlyError};
use super::profile::{LegKind, ProfilePlan};
use super::SegmentMissionModel;

/// Reason string of a budget failure.
pub const SIZING_BUDGET_EXHAUSTED: &str = "sizing_budget_exhausted";

/// Richardson bound on the frozen step count: the step-doubling estimate
/// `|F_2n - F_n| / 3` of the trip-fuel discretization error (second-order
/// midpoint integration) must not exceed this fraction of trip fuel.
pub const RICHARDSON_TOLERANCE: f64 = 5.0e-4;

/// Largest step count the refinement may freeze.
const MAX_STEPS_PER_SEGMENT: usize = 64;

/// Reason string of a plan whose step count reached `MAX_STEPS_PER_SEGMENT`
/// with its Richardson estimate still above [`RICHARDSON_TOLERANCE`].
pub const MISSION_STEP_UNCONVERGED: &str = "mission_step_unconverged";

/// Why no frozen plan could be made.
#[derive(Debug, Clone, PartialEq)]
pub enum FreezeError {
    /// A trip with the plan's choices could not be flown.
    Fuel(FuelModelError),
    /// The step count reached `MAX_STEPS_PER_SEGMENT` with the Richardson
    /// estimate of trip fuel above [`RICHARDSON_TOLERANCE`].
    StepUnconverged {
        /// The capped step count per planned segment.
        steps_per_segment: usize,
        /// Richardson estimate of the trip-fuel error at that count, kg.
        richardson_error_kg: f64,
        /// Trip fuel at that count, kg.
        trip_fuel_kg: f64,
    },
}

impl From<FuelModelError> for FreezeError {
    fn from(error: FuelModelError) -> Self {
        Self::Fuel(error)
    }
}

impl std::fmt::Display for FreezeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fuel(error) => error.fmt(formatter),
            Self::StepUnconverged {
                steps_per_segment,
                richardson_error_kg,
                trip_fuel_kg,
            } => write!(
                formatter,
                "{MISSION_STEP_UNCONVERGED}: Richardson error {richardson_error_kg} kg of \
                 {trip_fuel_kg} kg trip fuel exceeds the {RICHARDSON_TOLERANCE} bound at \
                 {steps_per_segment} steps per segment"
            ),
        }
    }
}

impl std::error::Error for FreezeError {}

/// The step-doubling decision on trip fuels `coarse_kg` at `steps` per
/// segment and `fine_kg` at twice that, with its Richardson error, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Refinement {
    /// Freeze `steps`.
    Coarse(f64),
    /// Freeze `2 steps`.
    Fine(f64),
    /// Double the count and compare again.
    Double,
}

/// The coarse count is kept when its own error, 4/3 of the difference for a
/// second-order scheme, already meets [`RICHARDSON_TOLERANCE`]; otherwise the
/// fine count when its error, a third of it, does; otherwise the count is
/// doubled, and a capped count that still misses the bound is an error.
fn refine(steps: usize, coarse_kg: f64, fine_kg: f64) -> Result<Refinement, FreezeError> {
    let difference_kg = (fine_kg - coarse_kg).abs();
    if 4.0 / 3.0 * difference_kg <= RICHARDSON_TOLERANCE * fine_kg {
        Ok(Refinement::Coarse(4.0 / 3.0 * difference_kg))
    } else if difference_kg / 3.0 <= RICHARDSON_TOLERANCE * fine_kg {
        Ok(Refinement::Fine(difference_kg / 3.0))
    } else if 2 * steps >= MAX_STEPS_PER_SEGMENT {
        Err(FreezeError::StepUnconverged {
            steps_per_segment: 2 * steps,
            richardson_error_kg: difference_kg / 3.0,
            trip_fuel_kg: fine_kg,
        })
    } else {
        Ok(Refinement::Double)
    }
}

/// Re-freezes allowed when a frozen plan cannot be flown at a later mass.
pub const MAX_REFREEZES: u32 = 2;

/// The discrete choices of one trip, made once and reused.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenMissionPlan {
    /// Cruise-level policy the plan was made under.
    pub policy: CruiseAltitudePolicy,
    /// Mass the plan was made at, kg.
    pub takeoff_mass_kg: f64,
    /// Route it applies to, m; other ranges are flown unfrozen.
    pub range_m: f64,
    /// Planned cruise altitude, m.
    pub cruise_altitude_m: f64,
    /// Commanded-rate scales of the initial and step-climb rungs.
    pub climb_rate_scales: [f64; 3],
    /// Step-climb starts as fractions of the cruise distance.
    pub step_positions: Vec<f64>,
    /// Integration steps per planned segment.
    pub steps_per_segment: usize,
    /// Trip fuel at `takeoff_mass_kg` with these choices, kg.
    pub trip_fuel_kg: f64,
    /// Richardson estimate of that trip fuel's discretization error at the
    /// frozen count, kg: `4/3 |F_2n - F_n|` when `n` is frozen, a third of
    /// it when `2n` is.
    pub richardson_error_kg: f64,
}

/// An immutable frozen plan and the first mass at which one of its trips
/// could not be flown; clones share both. The mass only asks the owner of
/// the solve for a re-freeze; it never changes what the plan answers.
#[derive(Debug, Clone)]
pub(crate) struct FrozenHandle {
    plan: Arc<FrozenMissionPlan>,
    refreeze_at_kg: Arc<OnceLock<f64>>,
}

impl PartialEq for FrozenHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.plan, &other.plan) || self.plan == other.plan
    }
}

/// Work limits of one candidate evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizingBudget {
    /// Complete profile integrations (trip, diversion and search flights).
    pub max_trip_flights: u32,
    /// Propulsion-deck evaluations.
    pub max_deck_evals: u64,
    /// Outer sizing passes, enforced by the sizing loop that owns them.
    pub max_outer_passes: u32,
}

/// A budget with its counters; clones share the counters.
#[derive(Debug, Clone)]
pub(crate) struct BudgetState {
    limits: SizingBudget,
    flights: Arc<AtomicU64>,
    deck_start: u64,
}

impl PartialEq for BudgetState {
    fn eq(&self, other: &Self) -> bool {
        self.limits == other.limits
    }
}

impl SegmentMissionModel {
    /// Make the trip's discrete choices at `takeoff_mass_kg` over `range_m`
    /// under `policy`: cruise level, climb revisions, step-climb positions
    /// and the step count, doubled from the model's until the Richardson
    /// estimate meets [`RICHARDSON_TOLERANCE`], at most
    /// `MAX_STEPS_PER_SEGMENT`.
    ///
    /// # Errors
    ///
    /// [`FreezeError::Fuel`] as [`Self::fly_trip`];
    /// [`FreezeError::StepUnconverged`] when the capped count still misses
    /// the Richardson bound.
    pub fn freeze_plan(
        &self,
        takeoff_mass_kg: f64,
        range_m: f64,
        policy: &CruiseAltitudePolicy,
    ) -> Result<FrozenMissionPlan, FreezeError> {
        self.check_cancelled()?;
        self.validate().map_err(FuelModelError::InvalidModel)?;
        let mut model = self.clone();
        model.profile.cruise_altitude_policy = *policy;
        model.frozen = None;
        let choose = |steps: usize| {
            let mut refined = model.clone();
            refined.steps_per_segment = steps;
            refined.fly_adapted(LegKind::Trip, takeoff_mass_kg, range_m, true)
        };
        let replay = |steps: usize, choices: &Adapted| {
            let mut refined = model.clone();
            refined.steps_per_segment = steps;
            refined
                .fly_at_scales(
                    LegKind::Trip,
                    takeoff_mass_kg,
                    range_m,
                    choices.scales,
                    Some((choices.cruise_m, &choices.flown.step_positions)),
                    true,
                )
                .map(|(flown, _)| flown)
        };
        let frozen =
            |choices: Adapted, flown: FlownLeg, steps: usize, error_kg: f64| FrozenMissionPlan {
                policy: *policy,
                takeoff_mass_kg,
                range_m,
                cruise_altitude_m: choices.cruise_m,
                climb_rate_scales: choices.scales,
                trip_fuel_kg: flown.leg.fuel_kg,
                step_positions: choices.flown.step_positions,
                steps_per_segment: steps,
                richardson_error_kg: error_kg,
            };
        let mut steps = self.steps_per_segment.max(1);
        loop {
            // Changing level, climb-rate scales or step positions between
            // grids measures plan changes, not integration error. Both
            // grids fly the same choices selected by the finer model.
            let choices = choose(2 * steps)?;
            let fine =
                replay(2 * steps, &choices).map_err(|error| into_model_error(error, range_m))?;
            let coarse = match replay(steps, &choices) {
                Ok(flown) => flown,
                Err(FlyError::Fuel(error @ FuelModelError::Cancelled)) => return Err(error.into()),
                Err(error) if 2 * steps >= MAX_STEPS_PER_SEGMENT => {
                    return Err(into_model_error(error, range_m).into());
                }
                Err(_) => {
                    steps *= 2;
                    continue;
                }
            };
            match refine(steps, coarse.leg.fuel_kg, fine.leg.fuel_kg)? {
                Refinement::Coarse(error_kg) => {
                    return Ok(frozen(choices, coarse, steps, error_kg))
                }
                Refinement::Fine(error_kg) => {
                    return Ok(frozen(choices, fine, 2 * steps, error_kg))
                }
                Refinement::Double => steps *= 2,
            }
        }
    }

    /// Fly every trip over the plan's range with its choices.
    pub fn with_frozen_plan(mut self, plan: FrozenMissionPlan) -> Self {
        self.steps_per_segment = plan.steps_per_segment;
        self.profile.cruise_altitude_policy = plan.policy;
        self.frozen = Some(FrozenHandle {
            plan: Arc::new(plan),
            refreeze_at_kg: Arc::new(OnceLock::new()),
        });
        self
    }

    /// The frozen plan in force, if one is set.
    pub fn frozen_plan(&self) -> Option<FrozenMissionPlan> {
        self.frozen.as_ref().map(|handle| (*handle.plan).clone())
    }

    /// Stop with [`SIZING_BUDGET_EXHAUSTED`] once `budget` is spent.
    pub fn with_budget(mut self, budget: SizingBudget) -> Self {
        self.budget = Some(BudgetState {
            limits: budget,
            flights: Arc::new(AtomicU64::new(0)),
            deck_start: self.propulsion.evaluation_count(),
        });
        self
    }

    /// The budget in force, if any.
    pub fn budget(&self) -> Option<SizingBudget> {
        self.budget.as_ref().map(|state| state.limits)
    }

    /// Flights and deck evaluations charged since `with_budget`.
    pub fn work(&self) -> Option<(u64, u64)> {
        self.budget.as_ref().map(|state| {
            (
                state.flights.load(Ordering::Relaxed),
                self.propulsion.evaluation_count() - state.deck_start,
            )
        })
    }

    /// Fly `plan` after charging it to the budget.
    pub(super) fn fly_counted(
        &self,
        mass_kg: f64,
        plan: &ProfilePlan,
        frozen_steps: Option<&[f64]>,
    ) -> Result<FlownLeg, FlyError> {
        self.check_cancelled()?;
        if let Some(state) = &self.budget {
            let flights = state.flights.fetch_add(1, Ordering::Relaxed) + 1;
            let evals = self.propulsion.evaluation_count() - state.deck_start;
            let limits = state.limits;
            if flights > u64::from(limits.max_trip_flights) || evals > limits.max_deck_evals {
                return Err(FuelModelError::NotConverged(format!(
                    "{SIZING_BUDGET_EXHAUSTED}: {flights} flights and {evals} deck evaluations \
                     against {} and {}",
                    limits.max_trip_flights, limits.max_deck_evals
                ))
                .into());
            }
        }
        self.fly(mass_kg, plan, frozen_steps)
    }

    /// Fly a trip from the frozen plan when it covers `range_m`; `None`
    /// when no plan applies. A trip the plan cannot fly at this mass is
    /// flown adapted at it and records the mass for a re-freeze; the plan
    /// itself never changes.
    pub(super) fn fly_frozen(
        &self,
        mass_kg: f64,
        range_m: f64,
    ) -> Option<Result<FlownLeg, FuelModelError>> {
        let handle = self.frozen.as_ref()?;
        let plan = &handle.plan;
        if plan.range_m.to_bits() != range_m.to_bits() {
            return None;
        }
        let fixed = (plan.cruise_altitude_m, plan.step_positions.as_slice());
        let error = match self.fly_at_scales(
            LegKind::Trip,
            mass_kg,
            range_m,
            plan.climb_rate_scales,
            Some(fixed),
            true,
        ) {
            Ok((flown, _)) => return Some(Ok(flown)),
            Err(error) => error,
        };
        let refreezable = match &error {
            FlyError::TooShort { .. } | FlyError::Climb { .. } => true,
            FlyError::Fuel(FuelModelError::NotConverged(reason)) => {
                reason.contains("energy deficit") || reason.contains("speed schedule not attained")
            }
            FlyError::Fuel(_) => false,
        };
        if !refreezable {
            return Some(Err(into_model_error(error, range_m)));
        }
        // The first such mass wins; later ones leave it unchanged. The trip
        // itself is flown with the choices made at this mass, as a plan
        // frozen here would, which depends on the mass alone.
        let _ = handle.refreeze_at_kg.set(mass_kg);
        Some(
            self.fly_adapted(LegKind::Trip, mass_kg, range_m, true)
                .map(|adapted| adapted.flown),
        )
    }

    /// The first mass at which a trip of the frozen plan could not be flown
    /// for a reason a plan frozen at that mass could avoid (a climb deficit
    /// or a profile that does not fit), if any.
    pub fn refreeze_request_kg(&self) -> Option<f64> {
        self.frozen
            .as_ref()
            .and_then(|handle| handle.refreeze_at_kg.get().copied())
    }

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
        let mut solution = solve(&model);
        for _ in 0..MAX_REFREEZES {
            let Some(mass_kg) = model.refreeze_request_kg() else {
                break;
            };
            let plan = match self.freeze_plan(mass_kg, range_m, policy) {
                Ok(plan) => plan,
                Err(
                    error @ (FreezeError::StepUnconverged { .. }
                    | FreezeError::Fuel(FuelModelError::Cancelled)),
                ) => return Err(error),
                Err(FreezeError::Fuel(_)) => break,
            };
            model = self.clone().with_frozen_plan(plan);
            solution = solve(&model);
        }
        Ok((model, solution))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Step doubling on an exactly second-order trip fuel, `F_n = F (1 +
    /// c / n^2)`: every frozen count's true error against the converged `F`
    /// meets [`RICHARDSON_TOLERANCE`] and equals the reported estimate, and a
    /// trip whose 64-step error still exceeds the bound (`c = 10`, 0.24 %) is
    /// refused rather than frozen.
    #[test]
    fn step_doubling_freezes_only_counts_within_the_richardson_bound() {
        let exact_kg = 20_000.0;
        let fuel = |c: f64, n: usize| exact_kg * (1.0 + c / (n * n) as f64);
        let run = |c: f64| -> Result<(usize, f64), FreezeError> {
            let mut steps = 8;
            loop {
                match refine(steps, fuel(c, steps), fuel(c, 2 * steps))? {
                    Refinement::Coarse(error_kg) => return Ok((steps, error_kg)),
                    Refinement::Fine(error_kg) => return Ok((2 * steps, error_kg)),
                    Refinement::Double => steps *= 2,
                }
            }
        };
        for (c, expected_steps) in [(0.01, 8), (0.1, 16), (1.0, 64)] {
            let (steps, error_kg) = run(c).unwrap_or_else(|error| panic!("c {c}: {error}"));
            let true_error_kg = fuel(c, steps) - exact_kg;
            assert_eq!(steps, expected_steps, "c {c}");
            assert!(true_error_kg <= RICHARDSON_TOLERANCE * exact_kg, "c {c}");
            assert!(
                (error_kg - true_error_kg).abs() <= 1.0e-9 * exact_kg,
                "c {c}"
            );
        }
        assert!(matches!(
            run(10.0),
            Err(FreezeError::StepUnconverged {
                steps_per_segment: MAX_STEPS_PER_SEGMENT,
                ..
            })
        ));
    }
}
