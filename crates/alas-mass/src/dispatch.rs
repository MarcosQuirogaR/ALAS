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

use crate::fuel_plan::{FuelBurnModel, FuelPlan, FuelQuantity};
use crate::fuel_policy::plan_fuel;

/// The first takeoff-mass guess is a multiple of the zero-fuel mass, not a
/// physical constant: a Picard iteration on a well-posed fuel closure
/// converges from any reasonable starting point, and one quarter of the
/// zero-fuel mass is a generous first estimate of the fuel fraction for a
/// design-range transport mission.
const FIRST_GUESS_TOW_RATIO: f64 = 1.25;

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

/// Close the takeoff-mass fixed point for one mission.
///
/// Never panics: an invalid input or a model failure is reported as
/// [`DispatchStatus::ModelFailed`], with the solution otherwise carrying a
/// degenerate plan at `zero_fuel_mass_kg` so the caller always has a
/// well-formed [`DispatchSolution`] to inspect.
pub fn solve_dispatch(
    zero_fuel_mass_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> DispatchSolution {
    try_solve(
        zero_fuel_mass_kg,
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

fn try_solve(
    zero_fuel_mass_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
) -> Result<DispatchSolution, String> {
    validate_inputs(
        zero_fuel_mass_kg,
        range_m,
        limits,
        max_iterations,
        tolerance_kg,
    )?;

    let mut takeoff_mass_kg = (zero_fuel_mass_kg * FIRST_GUESS_TOW_RATIO).min(limits.mtow_kg);
    let mut iterates = Vec::with_capacity(max_iterations);
    let mut previous_change_kg: Option<f64> = None;
    let mut converged = false;

    for _ in 0..max_iterations {
        let plan = plan_fuel(policy, model, takeoff_mass_kg, range_m)
            .map_err(|error| error.to_string())?;
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
    let mut plan =
        plan_fuel(policy, model, takeoff_mass_kg, range_m).map_err(|error| error.to_string())?;
    let mut resolved_takeoff_mass_kg = takeoff_mass_kg;
    let required_unclamped_kg = zero_fuel_mass_kg + plan.takeoff_fuel_kg();

    let status = if required_unclamped_kg > limits.mtow_kg + tolerance_kg {
        resolved_takeoff_mass_kg = limits.mtow_kg;
        plan = plan_fuel(policy, model, resolved_takeoff_mass_kg, range_m)
            .map_err(|error| error.to_string())?;
        DispatchStatus::MtowLimited {
            shortfall_kg: required_unclamped_kg - limits.mtow_kg,
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
