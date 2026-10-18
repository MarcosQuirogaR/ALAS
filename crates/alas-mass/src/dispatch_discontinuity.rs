// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closing the takeoff-mass fixed point when the priced fuel jumps with mass.
//!
//! The Picard iteration of [`super::solve_dispatch`] assumes the required
//! takeoff fuel `F(m)` is continuous in the takeoff mass `m`. A trip flown on
//! a frozen plan is not: a trip the plan cannot fly at some mass is flown on
//! the choices made at that mass instead (cruise level, step climbs), so
//! `F(m)` can drop or rise by a few hundred kilograms across one mass. The
//! residual `g(m) = zero_fuel_mass + F(m) - m` then changes sign across the
//! jump without vanishing: no fixed point exists, and the damped iteration
//! cycles between the two sides for ever (the A380-800 configured route at
//! its declared MTOW cycled with period three, last change -74.7 kg).
//!
//! The settled answer is the lowest takeoff mass, to the tolerance, whose
//! loaded fuel `m - zero_fuel_mass` covers the plan priced at that mass
//! (`g <= 0`): the aircraft never takes off short of its reserves, and it
//! carries a surplus no larger than the jump itself. Where `F` is continuous
//! the same bisection finds the ordinary root and the surplus is below the
//! tolerance, so the iteration's answer is unchanged whenever it converges.

use super::*;

/// Bisection budget; a bracket of a few hundred tonnes reaches a millimetre
/// of mass in forty halvings.
const SETTLE_ITERATIONS: usize = 60;

/// The takeoff mass a discontinuity settles on.
pub(super) struct Settled {
    /// Lowest takeoff mass, to the tolerance, whose loaded fuel covers its
    /// priced plan, kg.
    pub(super) takeoff_mass_kg: f64,
    /// The plan priced at that mass.
    pub(super) plan: FuelPlan,
}

/// Bisect the sign change of `g` the iteration's own evaluations bracket.
///
/// `samples` are `(takeoff mass, required takeoff fuel)` pairs the iteration
/// priced. Returns `Ok(None)` when they do not bracket a sign change (the
/// iteration was slow, not cycling, or the model is unevaluable between the
/// samples) so the caller keeps its `NotConverged` evidence.
///
/// # Errors
///
/// A cancellation or a failure no mass fixes, as [`evaluate_bracketed`].
pub(super) fn settle_on_sign_change(
    plans: &mut PlanEvaluator<'_>,
    zero_fuel_mass_kg: f64,
    samples: &[(f64, f64)],
    tolerance_kg: f64,
) -> Result<Option<Settled>, DispatchSolveError> {
    let g = |mass_kg: f64, fuel_kg: f64| zero_fuel_mass_kg + fuel_kg - mass_kg;
    let mut ordered: Vec<(f64, f64)> = samples
        .iter()
        .copied()
        .filter(|(mass_kg, fuel_kg)| mass_kg.is_finite() && fuel_kg.is_finite())
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The adjacent pair with the highest masses that is short below and
    // covered above is the sign change the iteration last straddled.
    let Some(pair) = ordered
        .windows(2)
        .rev()
        .find(|pair| g(pair[0].0, pair[0].1) > 0.0 && g(pair[1].0, pair[1].1) <= 0.0)
    else {
        return Ok(None);
    };
    let mut low_kg = pair[0].0;
    let mut high_kg = pair[1].0;
    let mut high_plan = match plans.evaluate(high_kg) {
        Ok(plan) => plan,
        Err(FuelModelError::Cancelled) => return Err(DispatchSolveError::Cancelled),
        Err(_) => return Ok(None),
    };
    for _ in 0..SETTLE_ITERATIONS {
        if high_kg - low_kg <= 0.1 * tolerance_kg {
            break;
        }
        let mid_kg = 0.5 * (low_kg + high_kg);
        match plans.evaluate(mid_kg) {
            Ok(plan) => {
                if g(mid_kg, plan.takeoff_fuel_kg()) > 0.0 {
                    low_kg = mid_kg;
                } else {
                    high_kg = mid_kg;
                    high_plan = plan;
                }
            }
            Err(FuelModelError::Cancelled) => return Err(DispatchSolveError::Cancelled),
            // A mass the model cannot fly between two it can: the evaluable
            // set is not an interval here, so no settled answer is claimed.
            Err(_) => return Ok(None),
        }
    }
    Ok(Some(Settled {
        takeoff_mass_kg: high_kg,
        plan: high_plan,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_plan::LegEstimate;

    /// A linear toy trip, `F = fraction * m`, whose fuel drops by `jump_kg`
    /// at and above `jump_at_kg`: the plan-structure change of a frozen trip.
    struct JumpModel {
        fraction: f64,
        jump_at_kg: f64,
        jump_kg: f64,
    }

    impl FuelBurnModel for JumpModel {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            let drop = if takeoff_mass_kg >= self.jump_at_kg {
                self.jump_kg
            } else {
                0.0
            };
            Ok(LegEstimate {
                fuel_kg: self.fraction * takeoff_mass_kg - drop,
                time_s: range_m / 200.0,
            })
        }
        fn diversion(&self, _start: f64, distance_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.0,
                time_s: distance_m / 200.0,
            })
        }
        fn holding_fuel_flow_kg_s(
            &self,
            _mass: f64,
            _altitude: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
        fn cruise_fuel_flow_kg_s(&self, _mass: f64) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
    }

    fn policy() -> FuelPolicyConfig {
        FuelPolicyConfig {
            scheme: FuelScheme::TripFuelOnly,
            taxi_time_min: 0.0,
            ..Default::default()
        }
    }

    fn limits() -> DispatchLimits {
        DispatchLimits {
            mtow_kg: 1_000_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        }
    }

    /// A fuel drop that straddles the identity: below 400 t the trip needs
    /// `0.2 m`, above it `0.2 m - 300`. With `ZFW = 320 t` the lower branch
    /// would close at 400 t (just not below the jump) and the upper branch at
    /// 399.6 t (not above it), so no fixed point exists and the damped Picard
    /// iteration cycles. The settled answer is the jump mass, loaded fuel
    /// covering the priced plan.
    #[test]
    fn a_fuel_jump_that_straddles_the_fixed_point_settles_on_the_covered_side() {
        let model = JumpModel {
            fraction: 0.2,
            jump_at_kg: 399_900.0,
            jump_kg: 300.0,
        };
        let solution = solve_dispatch(
            320_000.0,
            1_000_000.0,
            &policy(),
            &model,
            &limits(),
            50,
            1.0,
        );
        assert_eq!(
            solution.status,
            DispatchStatus::Converged,
            "{:?}",
            solution.iterates
        );
        assert!(solution.settled_at_discontinuity);
        assert!(
            (solution.takeoff_mass_kg - 399_900.0).abs() <= 1.0,
            "{}",
            solution.takeoff_mass_kg
        );
        let surplus_kg =
            solution.takeoff_mass_kg - solution.zero_fuel_mass_kg - solution.plan.takeoff_fuel_kg();
        assert!(
            (0.0..=300.0).contains(&surplus_kg),
            "loaded fuel must cover the plan by no more than the jump: {surplus_kg}"
        );
    }

    /// A continuous model never reaches the fallback: the ordinary Picard
    /// answer is returned bit for bit and the flag stays down.
    #[test]
    fn a_continuous_closure_is_not_marked_as_settled_on_a_discontinuity() {
        let model = JumpModel {
            fraction: 0.2,
            jump_at_kg: f64::INFINITY,
            jump_kg: 0.0,
        };
        let solution = solve_dispatch(
            320_000.0,
            1_000_000.0,
            &policy(),
            &model,
            &limits(),
            50,
            1.0,
        );
        assert_eq!(solution.status, DispatchStatus::Converged);
        assert!(!solution.settled_at_discontinuity);
        assert!((solution.takeoff_mass_kg - 400_000.0).abs() < 1.0);
    }

    /// Samples that never change sign give no bracket: the caller keeps its
    /// `NotConverged` evidence instead of a fabricated answer.
    #[test]
    fn samples_without_a_sign_change_claim_nothing() {
        let model = JumpModel {
            fraction: 0.2,
            jump_at_kg: f64::INFINITY,
            jump_kg: 0.0,
        };
        let fuel_policy = policy();
        let mut plans = PlanEvaluator::new(&fuel_policy, &model, 1_000_000.0);
        let settled = settle_on_sign_change(
            &mut plans,
            320_000.0,
            &[(390_000.0, 78_000.0), (395_000.0, 79_000.0)],
            1.0,
        )
        .expect("no failure");
        assert!(settled.is_none());
    }
}
