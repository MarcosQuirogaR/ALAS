// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff-mass fixed point: finding the fuel load a mission's own
//! weight requires.
//!
//! [`crate::fuel_policy::plan_fuel`] prices a plan from a takeoff mass it is
//! given; it does not find the mass whose plan reproduces it. Trip fuel
//! grows with takeoff mass, and takeoff mass is the airframe plus payload
//! plus the fuel the plan just priced, so the two must be solved together.
//! [`solve_dispatch`] is that outer loop: a damped Picard iteration on
//! `takeoff_mass_kg = zero_fuel_mass_kg + plan_fuel(takeoff_mass_kg).takeoff_fuel_kg()`,
//! clamped at every step to the structural takeoff-weight limit.
//!
//! The loop never panics and never returns an error type: a model failure,
//! an invalid input, or a takeoff-weight or tank-capacity shortfall are all
//! reported through [`DispatchStatus`], alongside the plan evaluated at
//! whatever mass the solution actually represents.

use alas_config::{FuelPolicyConfig, FuelScheme};

use crate::fuel_plan::{FuelBurnModel, FuelModelError, FuelPlan, FuelQuantity};
use crate::fuel_policy::plan_fuel;

/// The first takeoff-mass guess is a multiple of the zero-fuel mass, not a
/// physical constant: a Picard iteration on a well-posed fuel closure
/// converges from any reasonable starting point, and one quarter of the
/// zero-fuel mass is a generous first estimate of the fuel fraction for a
/// design-range transport mission.
const FIRST_GUESS_TOW_RATIO: f64 = 1.25;

/// Bisection budget when the burn model cannot be evaluated at the mass the
/// iteration wants, searching for the highest mass below it where it can.
const EVALUATION_BRACKET_ITERATIONS: usize = 40;

/// Resolution of that bisection, kg.
const EVALUATION_BRACKET_RESOLUTION_KG: f64 = 0.01;

/// The structural and tank limits a dispatch solution is checked against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DispatchLimits {
    /// Maximum takeoff weight, kg. CS/FAR 25.25(a): a declared limit bounded
    /// by structure, flight performance and Part 36 noise, not a physics
    /// output.
    pub mtow_kg: f64,
    /// Maximum zero-fuel weight, kg, if the airframe declares one.
    pub mzfw_kg: Option<f64>,
    /// Maximum landing weight, kg, if the airframe declares one.
    pub mlw_kg: Option<f64>,
    /// Usable tank capacity, kg, at the fuel density the caller is
    /// costing the mission at. `None` means the tank arrangement has not
    /// been sized yet, so no tank check is made.
    pub usable_capacity_kg: Option<f64>,
}

/// How a dispatch iteration ended.
#[derive(Debug, Clone, PartialEq)]
pub enum DispatchStatus {
    /// The takeoff-mass update fell below `tolerance_kg` without hitting
    /// MTOW or the tank.
    Converged,
    /// The fuel the mission needs, at MTOW, still exceeds MTOW: the
    /// airframe cannot carry both the payload it was given and the fuel the
    /// range requires. `shortfall_kg` is the unclamped required takeoff
    /// mass minus MTOW, always positive.
    MtowLimited {
        /// How much over MTOW the required takeoff mass is, kg.
        shortfall_kg: f64,
    },
    /// The ramp fuel the plan calls for exceeds the usable tank capacity.
    /// Checked only when MTOW is not already the binding limit.
    TankLimited {
        /// How much the ramp fuel exceeds usable capacity, kg.
        shortfall_kg: f64,
    },
    /// `max_iterations` was reached without the change in takeoff mass
    /// falling below `tolerance_kg`, and neither MTOW nor the tank is
    /// binding.
    NotConverged {
        /// The takeoff-mass change at the last iteration, kg.
        last_change_kg: f64,
    },
    /// The burn model, or an input to the closure itself, could not
    /// produce a plan. Carries the reason as text because the model's own
    /// error type may vary by implementation.
    ModelFailed(String),
}

/// One step of the takeoff-mass iteration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DispatchIterate {
    /// Takeoff mass after this step's update, kg.
    pub takeoff_mass_kg: f64,
    /// Takeoff fuel the plan priced at the mass this step started from, kg.
    pub required_takeoff_fuel_kg: f64,
    /// The signed change applied to reach `takeoff_mass_kg`, kg (after
    /// relaxation, if a sign reversal triggered it).
    pub change_kg: f64,
}

/// The result of closing the takeoff-mass fixed point.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchSolution {
    /// How the iteration ended.
    pub status: DispatchStatus,
    /// The zero-fuel mass the closure was solved for, kg.
    pub zero_fuel_mass_kg: f64,
    /// The takeoff mass the solution represents, kg -- clamped to MTOW when
    /// [`DispatchStatus::MtowLimited`].
    pub takeoff_mass_kg: f64,
    /// Takeoff mass plus taxi fuel, kg.
    pub ramp_mass_kg: f64,
    /// Estimated landing mass at the destination under the priced plan, kg.
    pub destination_landing_mass_kg: f64,
    /// The fuel plan evaluated at `takeoff_mass_kg`, consistent with every
    /// other field of this solution.
    pub plan: FuelPlan,
    /// Every step the iteration took, in order, for diagnosing a solution
    /// that did not converge.
    pub iterates: Vec<DispatchIterate>,
    /// Whether `destination_landing_mass_kg` exceeds the declared MLW.
    pub landing_mass_exceeds_mlw: bool,
    /// Whether `zero_fuel_mass_kg` exceeds the declared MZFW.
    pub zero_fuel_mass_exceeds_mzfw: bool,
}

/// Close the takeoff-mass fixed point for one mission, starting the Picard
/// iteration from [`FIRST_GUESS_TOW_RATIO`] times the zero-fuel mass.
///
/// Never panics: an invalid input or a model failure across the whole
/// evaluation bracket is reported as [`DispatchStatus::ModelFailed`], with
/// the solution otherwise carrying a degenerate plan at `zero_fuel_mass_kg`
/// so the caller always has a well-formed [`DispatchSolution`] to inspect.
pub fn solve_dispatch(
    zero_fuel_mass_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> DispatchSolution {
    solve_dispatch_with_initial_guess(
        zero_fuel_mass_kg,
        zero_fuel_mass_kg * FIRST_GUESS_TOW_RATIO,
        range_m,
        policy,
        model,
        limits,
        max_iterations,
        tolerance_kg,
    )
}

/// As [`solve_dispatch`], but starting the Picard iteration from an explicit
/// `initial_guess_kg` instead of the default [`FIRST_GUESS_TOW_RATIO`] seed.
///
/// `limits` is always the declared physical envelope (MTOW/MZFW/MLW/tank
/// capacity) the solution is checked against and reported relative to, and
/// this never adjusts it: a caller that saw [`DispatchStatus::ModelFailed`]
/// from the default seed (for instance because the model has no evaluable
/// point near the default guess, even though a feasible root exists lower
/// down) can retry from a lower `initial_guess_kg` without touching
/// `limits`, so a retry never misreports a false, seed-dependent
/// `MtowLimited`/`TankLimited` against a fictitious, lowered ceiling instead
/// of the aircraft's real one. The closure itself also recovers from a
/// model failure encountered mid-iteration (an overshoot into an
/// unevaluable region above a feasible root) the same way; see
/// [`evaluate_bracketed`]. Only a model that cannot be evaluated anywhere
/// between the zero-fuel mass and the seed, or between it and MTOW when that
/// is checked, is reported as [`DispatchStatus::ModelFailed`].
// Each argument is a separately reported dispatch input (masses, range,
// reserves, the model); bundling them would hide which one a caller sets.
#[allow(clippy::too_many_arguments)]
pub fn solve_dispatch_with_initial_guess(
    zero_fuel_mass_kg: f64,
    initial_guess_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> DispatchSolution {
    try_solve(
        zero_fuel_mass_kg,
        initial_guess_kg,
        range_m,
        policy,
        model,
        limits,
        max_iterations,
        tolerance_kg,
    )
    .unwrap_or_else(|reason| {
        failed_solution(
            policy.scheme,
            zero_fuel_mass_kg,
            DispatchStatus::ModelFailed(reason),
        )
    })
}

/// Whether `error` is a mass-dependent rating/energy shortfall that a
/// *different takeoff mass* can plausibly resolve, as opposed to a route,
/// polar/validity, or invalid-input failure that no mass in the bracket
/// fixes.
///
/// Only [`FuelModelError::NotConverged`] is ever bracketable, and only when
/// its message names a rating or energy deficit; the enum's other typed
/// variants (`RouteTooShort`, `InvalidDistance`, `MassOutOfRange`,
/// `InvalidModel`) are never mass-dependent in this sense and are excluded
/// outright. Within `NotConverged`, a `"polar validity"` or `"speed schedule
/// not attained"` rejection is excluded too: those are validity/schedule
/// breaches at the flown condition, not a power margin the search should
/// paper over by quietly flying a different mass.
fn is_mass_bracketable(error: &FuelModelError) -> bool {
    match error {
        FuelModelError::NotConverged(reason) => {
            (reason.contains("deficit") || reason.contains("rating"))
                && !reason.contains("polar validity")
                && !reason.contains("speed schedule not attained")
        }
        FuelModelError::RouteTooShort { .. }
        | FuelModelError::InvalidDistance { .. }
        | FuelModelError::MassOutOfRange { .. }
        | FuelModelError::InvalidModel(_) => false,
    }
}

/// Evaluate the plan at `candidate_kg`, or -- if the burn model fails there
/// with a mass-bracketable error (see [`is_mass_bracketable`]) -- at the
/// highest mass between `zero_fuel_mass_kg` and `candidate_kg` where it
/// succeeds, by bisection.
///
/// This never invents a feasible point beyond what the model itself
/// supports, and never bisects past a non-mass-dependent failure: it only
/// stops a rating/energy shortfall strictly above a feasible root (an
/// overshoot during the Picard iteration, or a seed placed in an
/// unevaluable region) from blocking that root. A route, polar/validity, or
/// invalid-input failure -- at the candidate or anywhere the bisection
/// probes -- is returned immediately as that typed failure, not silently
/// treated as "too heavy, try lighter".
///
/// The bisection assumes the evaluable set is a single contiguous region
/// reaching down from `zero_fuel_mass_kg` (a heavier aircraft needs no less
/// power margin than a lighter one at the same condition, so a mass-
/// dependent deficit above some threshold does not reappear below it). It
/// does not sample every mass in `[zero_fuel_mass_kg, candidate_kg]`, so a
/// failure at both endpoints is read *under that assumption* -- not as an
/// exhaustive proof the model is unevaluable at every point in between --
/// and reported as such.
fn evaluate_bracketed(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    zero_fuel_mass_kg: f64,
    candidate_kg: f64,
    range_m: f64,
) -> Result<(f64, FuelPlan), String> {
    if candidate_kg <= zero_fuel_mass_kg {
        return plan_fuel(policy, model, candidate_kg, range_m)
            .map(|plan| (candidate_kg, plan))
            .map_err(|error| error.to_string());
    }
    let candidate_error = match plan_fuel(policy, model, candidate_kg, range_m) {
        Ok(plan) => return Ok((candidate_kg, plan)),
        Err(error) => error,
    };
    if !is_mass_bracketable(&candidate_error) {
        return Err(candidate_error.to_string());
    }
    let mut low = zero_fuel_mass_kg;
    let mut low_plan = plan_fuel(policy, model, low, range_m).map_err(|error| {
        format!(
            "burn model failed at both ends of the {low:.1}-{candidate_kg:.1} kg search bracket ({error} at the zero-fuel mass, {candidate_error} at {candidate_kg:.1} kg); assuming a single contiguous evaluable region reaching down from the zero-fuel mass, this bracket has none, though every intermediate mass was not sampled"
        )
    })?;
    let mut high = candidate_kg;
    for _ in 0..EVALUATION_BRACKET_ITERATIONS {
        if high - low <= EVALUATION_BRACKET_RESOLUTION_KG {
            break;
        }
        let mid = 0.5 * (low + high);
        match plan_fuel(policy, model, mid, range_m) {
            Ok(plan) => {
                low = mid;
                low_plan = plan;
            }
            Err(error) if is_mass_bracketable(&error) => high = mid,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok((low, low_plan))
}

// The bracketed solve takes the same separately reported dispatch inputs as
// its public caller, plus the bracket itself.
#[allow(clippy::too_many_arguments)]
fn try_solve(
    zero_fuel_mass_kg: f64,
    initial_guess_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> Result<DispatchSolution, String> {
    validate_inputs(
        zero_fuel_mass_kg,
        initial_guess_kg,
        range_m,
        limits,
        max_iterations,
        tolerance_kg,
    )?;

    let mut takeoff_mass_kg = initial_guess_kg.min(limits.mtow_kg);
    let mut iterates = Vec::with_capacity(max_iterations);
    let mut previous_change_kg: Option<f64> = None;
    let mut converged = false;

    for _ in 0..max_iterations {
        let (evaluated_mass_kg, plan) =
            evaluate_bracketed(policy, model, zero_fuel_mass_kg, takeoff_mass_kg, range_m)?;
        takeoff_mass_kg = evaluated_mass_kg;
        let required_takeoff_fuel_kg = plan.takeoff_fuel_kg();
        let target_kg = (zero_fuel_mass_kg + required_takeoff_fuel_kg).min(limits.mtow_kg);
        let raw_change_kg = target_kg - takeoff_mass_kg;

        // A sign reversal means the last step overshot the fixed point;
        // damping by half prevents the iteration from oscillating around it
        // indefinitely instead of converging.
        let reversed = previous_change_kg.is_some_and(|previous| {
            previous != 0.0 && raw_change_kg != 0.0 && previous.signum() != raw_change_kg.signum()
        });
        let change_kg = if reversed {
            0.5 * raw_change_kg
        } else {
            raw_change_kg
        };

        takeoff_mass_kg += change_kg;
        iterates.push(DispatchIterate {
            takeoff_mass_kg,
            required_takeoff_fuel_kg,
            change_kg,
        });
        previous_change_kg = Some(change_kg);

        if change_kg.abs() < tolerance_kg {
            converged = true;
            break;
        }
    }

    // The last recorded plan was evaluated at the mass this iteration
    // started from, not the one it updated to; re-evaluate once more so the
    // returned plan is exactly consistent with the returned takeoff mass.
    // That re-evaluation can itself clip `takeoff_mass_kg` down (the last
    // step landed above a rating/energy boundary the loop never got to
    // react to), so the loop's own `converged` flag no longer certifies
    // this point: recompute the actual closure residual at whatever mass
    // was evaluated and classify convergence from that, not from the flag.
    let (resolved_after_loop_kg, plan_after_loop) =
        evaluate_bracketed(policy, model, zero_fuel_mass_kg, takeoff_mass_kg, range_m)?;
    let mut resolved_takeoff_mass_kg = resolved_after_loop_kg;
    let mut plan = plan_after_loop;
    let required_unclamped_kg = zero_fuel_mass_kg + plan.takeoff_fuel_kg();
    let closure_residual_kg = (resolved_takeoff_mass_kg - required_unclamped_kg).abs();
    let converged = converged && closure_residual_kg <= tolerance_kg;

    let status = if required_unclamped_kg > limits.mtow_kg + tolerance_kg {
        // `DispatchSolution::takeoff_mass_kg`'s own contract is "clamped to
        // MTOW when MtowLimited": that label may only be used when the model
        // is actually evaluable there, so the returned plan and mass are
        // both exactly at the declared limit, not merely near it. When MTOW
        // itself is not evaluable (a rating/energy deficit right at the
        // structural boundary), reporting a plan clipped below it as if it
        // were "at MTOW" would be a false, ambiguous physical claim. Only
        // fall back to `NotConverged` -- carrying the best evaluable
        // evidence -- when that lower point *itself* still demonstrates a
        // requirement over MTOW; otherwise this is a boundary evaluation
        // failure, not a demonstrated structural limit, and is reported as
        // the typed `ModelFailed` every other unrecoverable failure is.
        let (mtow_evaluated_kg, mtow_plan) =
            evaluate_bracketed(policy, model, zero_fuel_mass_kg, limits.mtow_kg, range_m)?;
        let mtow_boundary_evaluable =
            (limits.mtow_kg - mtow_evaluated_kg).abs() <= EVALUATION_BRACKET_RESOLUTION_KG;
        let mtow_required_unclamped_kg = zero_fuel_mass_kg + mtow_plan.takeoff_fuel_kg();
        if mtow_boundary_evaluable {
            resolved_takeoff_mass_kg = limits.mtow_kg;
            plan = mtow_plan;
            DispatchStatus::MtowLimited {
                shortfall_kg: (mtow_required_unclamped_kg - limits.mtow_kg).max(0.0),
            }
        } else if mtow_required_unclamped_kg > limits.mtow_kg + tolerance_kg {
            resolved_takeoff_mass_kg = mtow_evaluated_kg;
            plan = mtow_plan;
            DispatchStatus::NotConverged {
                last_change_kg: mtow_required_unclamped_kg - mtow_evaluated_kg,
            }
        } else {
            return Err(format!(
                "MTOW boundary at {:.1} kg is not evaluable, and the best mass the model supports below it ({mtow_evaluated_kg:.1} kg) does not itself demonstrate a requirement over MTOW",
                limits.mtow_kg
            ));
        }
    } else if let Some(capacity_kg) = limits
        .usable_capacity_kg
        .filter(|&capacity_kg| plan.ramp_fuel_kg() > capacity_kg)
    {
        DispatchStatus::TankLimited {
            shortfall_kg: plan.ramp_fuel_kg() - capacity_kg,
        }
    } else if converged {
        DispatchStatus::Converged
    } else {
        DispatchStatus::NotConverged {
            last_change_kg: iterates.last().map_or(0.0, |iterate| iterate.change_kg),
        }
    };

    Ok(DispatchSolution {
        status,
        zero_fuel_mass_kg,
        takeoff_mass_kg: resolved_takeoff_mass_kg,
        ramp_mass_kg: resolved_takeoff_mass_kg + plan.taxi.kg,
        destination_landing_mass_kg: plan.destination_landing_mass_kg,
        landing_mass_exceeds_mlw: limits
            .mlw_kg
            .is_some_and(|mlw_kg| plan.destination_landing_mass_kg > mlw_kg),
        zero_fuel_mass_exceeds_mzfw: limits
            .mzfw_kg
            .is_some_and(|mzfw_kg| zero_fuel_mass_kg > mzfw_kg),
        plan,
        iterates,
    })
}

fn validate_inputs(
    zero_fuel_mass_kg: f64,
    initial_guess_kg: f64,
    range_m: f64,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> Result<(), String> {
    if !zero_fuel_mass_kg.is_finite() || zero_fuel_mass_kg <= 0.0 {
        return Err(format!(
            "zero-fuel mass {zero_fuel_mass_kg} kg must be finite and positive"
        ));
    }
    if !initial_guess_kg.is_finite() || initial_guess_kg < zero_fuel_mass_kg {
        return Err(format!(
            "initial guess {initial_guess_kg} kg must be finite and at least the zero-fuel mass {zero_fuel_mass_kg} kg"
        ));
    }
    if !range_m.is_finite() || range_m < 0.0 {
        return Err(format!("range {range_m} m must be finite and nonnegative"));
    }
    if !limits.mtow_kg.is_finite() || limits.mtow_kg < zero_fuel_mass_kg {
        return Err(format!(
            "MTOW {} kg must be finite and at least the zero-fuel mass {zero_fuel_mass_kg} kg",
            limits.mtow_kg
        ));
    }
    for (name, value) in [
        ("MZFW", limits.mzfw_kg),
        ("MLW", limits.mlw_kg),
        ("usable tank capacity", limits.usable_capacity_kg),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("{name} {value} kg must be finite and positive"));
            }
        }
    }
    if !tolerance_kg.is_finite() || tolerance_kg <= 0.0 {
        return Err(format!(
            "tolerance {tolerance_kg} kg must be finite and positive"
        ));
    }
    if max_iterations == 0 {
        return Err("max_iterations must be at least one".to_owned());
    }
    Ok(())
}

/// A degenerate plan carrying only the zero-fuel mass, used when the
/// closure could not be solved at all.
fn empty_plan(scheme: FuelScheme, zero_fuel_mass_kg: f64) -> FuelPlan {
    let landing_mass_kg = if zero_fuel_mass_kg.is_finite() && zero_fuel_mass_kg > 0.0 {
        zero_fuel_mass_kg
    } else {
        0.0
    };
    FuelPlan {
        scheme,
        taxi: FuelQuantity::NONE,
        trip: FuelQuantity::NONE,
        contingency: FuelQuantity::NONE,
        alternate: FuelQuantity::NONE,
        final_reserve: FuelQuantity::NONE,
        additional: FuelQuantity::NONE,
        extra: FuelQuantity::NONE,
        trip_time_s: 0.0,
        destination_landing_mass_kg: landing_mass_kg,
        reserve_landing_mass_kg: landing_mass_kg,
    }
}

fn failed_solution(
    scheme: FuelScheme,
    zero_fuel_mass_kg: f64,
    status: DispatchStatus,
) -> DispatchSolution {
    let plan = empty_plan(scheme, zero_fuel_mass_kg);
    let landing_mass_kg = plan.destination_landing_mass_kg;
    DispatchSolution {
        status,
        zero_fuel_mass_kg,
        takeoff_mass_kg: landing_mass_kg,
        ramp_mass_kg: landing_mass_kg,
        destination_landing_mass_kg: landing_mass_kg,
        plan,
        iterates: Vec::new(),
        landing_mass_exceeds_mlw: false,
        zero_fuel_mass_exceeds_mzfw: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};

    /// A linear toy model: trip fuel is a fixed fraction of the takeoff
    /// mass it starts from, which makes the fixed point closed-form
    /// (`TOW = ZFW / (1 - trip_fraction)`) and the convergence check exact.
    struct ToyModel {
        trip_fraction: f64,
        cruise_speed_m_s: f64,
    }

    impl FuelBurnModel for ToyModel {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: self.trip_fraction * takeoff_mass_kg,
                time_s: range_m / self.cruise_speed_m_s,
            })
        }

        fn diversion(
            &self,
            start_mass_kg: f64,
            distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.02 * start_mass_kg,
                time_s: distance_m / self.cruise_speed_m_s,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.00001 * mass_kg)
        }

        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(0.00003 * mass_kg)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.05)
        }
    }

    fn trip_fuel_only_policy() -> FuelPolicyConfig {
        FuelPolicyConfig {
            scheme: FuelScheme::TripFuelOnly,
            ..Default::default()
        }
    }

    #[test]
    fn the_closure_converges_and_the_takeoff_mass_reproduces_its_own_fuel() {
        let policy = trip_fuel_only_policy();
        let model = ToyModel {
            trip_fraction: 0.30,
            cruise_speed_m_s: 200.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 30, 1e-3);

        assert_eq!(solution.status, DispatchStatus::Converged);
        assert!(solution.iterates.len() < 30, "{}", solution.iterates.len());
        assert!(
            (solution.takeoff_mass_kg
                - (solution.zero_fuel_mass_kg + solution.plan.takeoff_fuel_kg()))
            .abs()
                < 1e-2
        );
        // Closed form: TOW = ZFW / (1 - trip_fraction).
        assert!((solution.takeoff_mass_kg - 50_000.0 / 0.70).abs() < 1e-1);
    }

    #[test]
    fn an_mtow_shortfall_is_reported_with_the_plan_evaluated_at_mtow() {
        let policy = trip_fuel_only_policy();
        let model = ToyModel {
            trip_fraction: 0.50,
            cruise_speed_m_s: 200.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 80_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 30, 1e-3);

        match solution.status {
            DispatchStatus::MtowLimited { shortfall_kg } => assert!(shortfall_kg > 0.0),
            other => panic!("expected MtowLimited, got {other:?}"),
        }
        assert_eq!(solution.takeoff_mass_kg, limits.mtow_kg);
        assert!((solution.plan.trip.kg - model.trip_fraction * limits.mtow_kg).abs() < 1e-6);
    }

    #[test]
    fn a_tank_shortfall_is_reported_when_ramp_fuel_exceeds_capacity() {
        let policy = trip_fuel_only_policy();
        let model = ToyModel {
            trip_fraction: 0.10,
            cruise_speed_m_s: 200.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 200_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: Some(3_000.0),
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 30, 1e-3);

        match solution.status {
            DispatchStatus::TankLimited { shortfall_kg } => assert!(shortfall_kg > 0.0),
            other => panic!("expected TankLimited, got {other:?}"),
        }
        assert!(solution.plan.ramp_fuel_kg() > limits.usable_capacity_kg.unwrap());
    }

    #[test]
    fn invalid_inputs_are_reported_without_panicking() {
        let policy = trip_fuel_only_policy();
        let model = ToyModel {
            trip_fraction: 0.10,
            cruise_speed_m_s: 200.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 40_000.0, // below the zero-fuel mass
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 30, 1e-3);
        assert!(matches!(solution.status, DispatchStatus::ModelFailed(_)));

        let valid_limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(
            f64::NAN,
            1_000_000.0,
            &policy,
            &model,
            &valid_limits,
            30,
            1e-3,
        );
        assert!(matches!(solution.status, DispatchStatus::ModelFailed(_)));
    }

    /// A toy model that fails above a fixed mass, with a *mass-bracketable*
    /// [`FuelModelError::NotConverged`] naming a rating/energy deficit --
    /// exactly the shape a real off-design deck failure takes.
    struct FailAboveMassModel {
        trip_fraction: f64,
        cruise_speed_m_s: f64,
        fail_above_kg: f64,
    }

    impl FuelBurnModel for FailAboveMassModel {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            if takeoff_mass_kg > self.fail_above_kg {
                return Err(FuelModelError::NotConverged(format!(
                    "level flight energy deficit at {takeoff_mass_kg:.0} kg: rating exceeded"
                )));
            }
            Ok(LegEstimate {
                fuel_kg: self.trip_fraction * takeoff_mass_kg,
                time_s: range_m / self.cruise_speed_m_s,
            })
        }

        fn diversion(
            &self,
            start_mass_kg: f64,
            distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.02 * start_mass_kg,
                time_s: distance_m / self.cruise_speed_m_s,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.00001 * mass_kg)
        }

        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(0.00003 * mass_kg)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.05)
        }
    }

    /// As [`FailAboveMassModel`], but the above-mass failure is a
    /// [`FuelModelError::RouteTooShort`] -- a typed, non-mass-dependent
    /// failure that [`is_mass_bracketable`] must never bisect past.
    struct FailAboveMassWithRouteError {
        trip_fraction: f64,
        cruise_speed_m_s: f64,
        fail_above_kg: f64,
    }

    impl FuelBurnModel for FailAboveMassWithRouteError {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            if takeoff_mass_kg > self.fail_above_kg {
                return Err(FuelModelError::RouteTooShort {
                    range_m,
                    minimum_range_m: range_m * 1.5,
                });
            }
            Ok(LegEstimate {
                fuel_kg: self.trip_fraction * takeoff_mass_kg,
                time_s: range_m / self.cruise_speed_m_s,
            })
        }

        fn diversion(
            &self,
            start_mass_kg: f64,
            distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.02 * start_mass_kg,
                time_s: distance_m / self.cruise_speed_m_s,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.00001 * mass_kg)
        }

        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(0.00003 * mass_kg)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.05)
        }
    }

    /// A model that never evaluates, at any mass: the genuinely hopeless
    /// case the bracket search must not paper over.
    struct AlwaysFailsModel;

    impl FuelBurnModel for AlwaysFailsModel {
        fn trip(
            &self,
            _takeoff_mass_kg: f64,
            _range_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Err(FuelModelError::NotConverged(
                "level flight energy deficit: no evaluable point".to_owned(),
            ))
        }

        fn diversion(
            &self,
            _start_mass_kg: f64,
            _distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.0,
                time_s: 0.0,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            _mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }

        fn cruise_fuel_flow_kg_s(&self, _mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
    }

    /// ZFW 50 t, model valid through 75 t only, true MTOW 100 t. The fixed
    /// point (`TOW = ZFW / (1 - trip_fraction)`) is 70 t, strictly below the
    /// model's failure ceiling and far below MTOW. The default seed
    /// (1.25 x ZFW = 62.5 t) never even enters the failing region here, so
    /// this is the baseline: closure must converge normally, at 70 t, with
    /// `limits.mtow_kg` untouched and no false `MtowLimited`.
    #[test]
    fn a_feasible_root_below_the_models_failure_ceiling_converges_normally() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassModel {
            trip_fraction: 2.0 / 7.0, // root: 50_000 / (1 - 2/7) = 70_000
            cruise_speed_m_s: 200.0,
            fail_above_kg: 75_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 40, 1e-3);

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        assert_eq!(solution.status, DispatchStatus::Converged);
        assert!(
            (solution.takeoff_mass_kg - 70_000.0).abs() < 1.0,
            "{}",
            solution.takeoff_mass_kg
        );
        assert!(
            (solution.takeoff_mass_kg
                - (solution.zero_fuel_mass_kg + solution.plan.takeoff_fuel_kg()))
            .abs()
                < 1e-2,
            "mass/plan conservation: takeoff mass must equal ZFW plus the priced trip fuel"
        );
    }

    /// As above, but the search is explicitly seeded *inside* the model's
    /// unevaluable region (90 t, above the 75 t ceiling and below the 100 t
    /// MTOW). Before the `alas-mass` bracket fix, a single failed evaluation
    /// at this seed reported `ModelFailed` outright; a caller-side retry
    /// that then lowered `limits.mtow_kg` to dodge it would have gone on to
    /// report a false `MtowLimited` against that fictitious ceiling instead
    /// of the real one. This proves the fixed closure instead recovers by
    /// bisecting `limits` unchanged and finds the same 70 t root.
    #[test]
    fn seeding_inside_the_failure_region_still_finds_the_feasible_root() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassModel {
            trip_fraction: 2.0 / 7.0,
            cruise_speed_m_s: 200.0,
            fail_above_kg: 75_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch_with_initial_guess(
            50_000.0,
            90_000.0,
            1_000_000.0,
            &policy,
            &model,
            &limits,
            40,
            1e-3,
        );

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        assert_eq!(
            solution.status,
            DispatchStatus::Converged,
            "a seed inside the unevaluable region must not report ModelFailed or a false MtowLimited"
        );
        assert!(
            (solution.takeoff_mass_kg - 70_000.0).abs() < 1.0,
            "{}",
            solution.takeoff_mass_kg
        );
        assert!(
            (solution.takeoff_mass_kg
                - (solution.zero_fuel_mass_kg + solution.plan.takeoff_fuel_kg()))
            .abs()
                < 1e-2,
            "mass/plan conservation: takeoff mass must equal ZFW plus the priced trip fuel"
        );
    }

    /// A control case: an explicit seed strictly *below* the feasible root
    /// (60 t, between the 50 t ZFW and the 70 t root) must converge to the
    /// same root by ordinary Picard contraction, with no bracket recovery
    /// needed and no false limit reported.
    #[test]
    fn seeding_below_the_root_converges_without_a_false_mtow_limit() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassModel {
            trip_fraction: 2.0 / 7.0,
            cruise_speed_m_s: 200.0,
            fail_above_kg: 75_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch_with_initial_guess(
            50_000.0,
            60_000.0,
            1_000_000.0,
            &policy,
            &model,
            &limits,
            40,
            1e-3,
        );

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        assert_eq!(solution.status, DispatchStatus::Converged);
        assert!(
            (solution.takeoff_mass_kg - 70_000.0).abs() < 1.0,
            "{}",
            solution.takeoff_mass_kg
        );
        assert!(
            (solution.takeoff_mass_kg
                - (solution.zero_fuel_mass_kg + solution.plan.takeoff_fuel_kg()))
            .abs()
                < 1e-2,
            "mass/plan conservation: takeoff mass must equal ZFW plus the priced trip fuel"
        );
    }

    /// The model is evaluable only up to 60 t; MTOW is declared at 100 t and
    /// is therefore never evaluable. Even the best point the model supports
    /// (~60 t) demonstrably needs far more fuel than MTOW admits (90%
    /// trip fraction), so this is real evidence of an MTOW-class shortfall
    /// -- but the solution must not claim `MtowLimited` with a plan
    /// "evaluated at exactly MTOW" it never actually produced. It must
    /// report `NotConverged` with `takeoff_mass_kg` at the mass it actually
    /// evaluated (well below the declared MTOW), not a false clamp to it.
    #[test]
    fn an_unevaluable_mtow_boundary_is_not_reported_as_a_false_mtow_limit() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassModel {
            trip_fraction: 0.9,
            cruise_speed_m_s: 200.0,
            fail_above_kg: 60_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 40, 1e-3);

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        assert!(
            !matches!(solution.status, DispatchStatus::MtowLimited { .. }),
            "must not claim MtowLimited when the MTOW boundary itself was never evaluated: {:?}",
            solution.status
        );
        assert!(
            matches!(solution.status, DispatchStatus::NotConverged { .. }),
            "expected NotConverged with the best-evaluable evidence, got {:?}",
            solution.status
        );
        assert!(
            solution.takeoff_mass_kg < limits.mtow_kg,
            "the returned mass must be the actually-evaluated point, not a false clamp to MTOW: {}",
            solution.takeoff_mass_kg
        );
        assert!(
            (solution.takeoff_mass_kg - 60_000.0).abs() < 1.0,
            "{}",
            solution.takeoff_mass_kg
        );
    }

    /// A model that cannot be evaluated anywhere, including at the
    /// zero-fuel mass itself, is a genuine model failure: the bracket search
    /// must not swallow it into a fabricated `Converged`/`MtowLimited`.
    #[test]
    fn a_model_with_no_evaluable_point_still_reports_model_failed() {
        let policy = trip_fuel_only_policy();
        let model = AlwaysFailsModel;
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 40, 1e-3);
        assert!(matches!(solution.status, DispatchStatus::ModelFailed(_)));
    }

    /// A non-mass-dependent failure (here `RouteTooShort`) above the seed
    /// must surface as a typed `ModelFailed` directly, never bisected past
    /// as if a lighter mass would fix a route-length or validity problem.
    #[test]
    fn a_route_length_failure_is_never_mass_bracketed() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassWithRouteError {
            trip_fraction: 2.0 / 7.0,
            cruise_speed_m_s: 200.0,
            fail_above_kg: 75_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch_with_initial_guess(
            50_000.0,
            90_000.0,
            1_000_000.0,
            &policy,
            &model,
            &limits,
            40,
            1e-3,
        );

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        match solution.status {
            DispatchStatus::ModelFailed(reason) => {
                assert!(
                    reason.contains("shorter than"),
                    "expected the RouteTooShort text to surface untouched, got: {reason}"
                );
            }
            other => panic!(
                "a route-length failure must stay a typed ModelFailed, not be bracketed away: {other:?}"
            ),
        }
    }

    /// As [`FailAboveMassWithRouteError`], but the above-mass failure is
    /// [`FuelModelError::InvalidModel`] -- another typed, non-mass-dependent
    /// failure `is_mass_bracketable` must reject outright.
    struct FailAboveMassWithInvalidModel {
        trip_fraction: f64,
        cruise_speed_m_s: f64,
        fail_above_kg: f64,
    }

    impl FuelBurnModel for FailAboveMassWithInvalidModel {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            if takeoff_mass_kg > self.fail_above_kg {
                return Err(FuelModelError::InvalidModel(
                    "polar fit has no valid coefficients".to_owned(),
                ));
            }
            Ok(LegEstimate {
                fuel_kg: self.trip_fraction * takeoff_mass_kg,
                time_s: range_m / self.cruise_speed_m_s,
            })
        }

        fn diversion(
            &self,
            start_mass_kg: f64,
            distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.02 * start_mass_kg,
                time_s: distance_m / self.cruise_speed_m_s,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.00001 * mass_kg)
        }

        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(0.00003 * mass_kg)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.05)
        }
    }

    #[test]
    fn an_invalid_model_failure_is_never_mass_bracketed() {
        let policy = trip_fuel_only_policy();
        let model = FailAboveMassWithInvalidModel {
            trip_fraction: 2.0 / 7.0,
            cruise_speed_m_s: 200.0,
            fail_above_kg: 75_000.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let original_limits = limits;
        let solution = solve_dispatch_with_initial_guess(
            50_000.0,
            90_000.0,
            1_000_000.0,
            &policy,
            &model,
            &limits,
            40,
            1e-3,
        );

        assert_eq!(limits, original_limits, "DispatchLimits must never mutate");
        match solution.status {
            DispatchStatus::ModelFailed(reason) => {
                assert!(
                    reason.contains("polar fit"),
                    "expected the InvalidModel text to surface untouched, got: {reason}"
                );
            }
            other => panic!(
                "an InvalidModel failure must stay a typed ModelFailed, not be bracketed away: {other:?}"
            ),
        }
    }

    #[test]
    fn mzfw_and_mlw_flags_report_a_declared_limit_exceeded() {
        let policy = trip_fuel_only_policy();
        let model = ToyModel {
            trip_fraction: 0.10,
            cruise_speed_m_s: 200.0,
        };
        let limits = DispatchLimits {
            mtow_kg: 100_000.0,
            mzfw_kg: Some(40_000.0), // below the zero-fuel mass given
            mlw_kg: Some(45_000.0),  // below the estimated destination landing mass
            usable_capacity_kg: None,
        };
        let solution = solve_dispatch(50_000.0, 1_000_000.0, &policy, &model, &limits, 30, 1e-3);
        assert!(solution.zero_fuel_mass_exceeds_mzfw);
        assert!(solution.landing_mass_exceeds_mlw);
    }
}
