// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact per-dispatch memoization, explicitly admitted by deterministic models.
use crate::fuel_plan::{FuelBurnModel, FuelModelError, FuelPlan};
use crate::fuel_policy::plan_fuel;
use alas_config::FuelPolicyConfig;
use std::collections::BTreeMap;

pub(super) struct PlanEvaluator<'a> {
    policy: &'a FuelPolicyConfig,
    model: &'a dyn FuelBurnModel,
    range_m: f64,
    cache: Option<BTreeMap<u64, Result<FuelPlan, FuelModelError>>>,
    /// Highest mass priced successfully so far, kg.
    highest_ok_kg: f64,
    /// Lowest mass whose pricing failed with a mass-bracketable error, kg.
    lowest_failed_kg: f64,
    /// Whether any mass failed with an error no other mass can resolve.
    unbracketable_failure: bool,
}

/// What an outcome at one mass says about another, given the dispatch's own
/// assumption (see `evaluate_bracketed`) that the evaluable masses are one
/// contiguous region reaching down from the zero-fuel mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Inferred {
    /// At or below a mass that priced successfully.
    Evaluable,
    /// At or above a mass that failed with a bracketable error.
    Unevaluable,
}
impl<'a> PlanEvaluator<'a> {
    pub(super) fn new(
        policy: &'a FuelPolicyConfig,
        model: &'a dyn FuelBurnModel,
        range_m: f64,
    ) -> Self {
        Self {
            policy,
            model,
            range_m,
            cache: model.deterministic_for_dispatch().then(BTreeMap::new),
            highest_ok_kg: f64::NEG_INFINITY,
            lowest_failed_kg: f64::INFINITY,
            unbracketable_failure: false,
        }
    }
    /// The outcome at `mass_kg` implied by the masses already priced, or
    /// `None` when they do not settle it.
    ///
    /// Only an exact, memoizing evaluator infers (the model opted in with
    /// `deterministic_for_dispatch`), and only while every priced mass agrees
    /// with a single contiguous evaluable region: some success above some
    /// failure, or a failure no other mass can resolve, withdraws every
    /// inference so the caller prices each mass itself, as it always did.
    pub(super) fn inferred(&self, mass_kg: f64) -> Option<Inferred> {
        self.cache.as_ref()?;
        if self.unbracketable_failure || self.highest_ok_kg >= self.lowest_failed_kg {
            return None;
        }
        if mass_kg <= self.highest_ok_kg {
            Some(Inferred::Evaluable)
        } else if mass_kg >= self.lowest_failed_kg {
            Some(Inferred::Unevaluable)
        } else {
            None
        }
    }
    pub(super) fn evaluate(&mut self, mass_kg: f64) -> Result<FuelPlan, FuelModelError> {
        self.model.check_cancellation()?;
        let key = mass_kg.to_bits();
        if let Some(answer) = self.cache.as_ref().and_then(|cache| cache.get(&key)) {
            return answer.clone();
        }
        let answer = plan_fuel(self.policy, self.model, mass_kg, self.range_m);
        // Never retain a supervisor event as an aircraft-model answer.
        if !matches!(answer, Err(FuelModelError::Cancelled)) {
            match &answer {
                Ok(_) => self.highest_ok_kg = self.highest_ok_kg.max(mass_kg),
                Err(error) if super::bracket::is_mass_bracketable(error) => {
                    self.lowest_failed_kg = self.lowest_failed_kg.min(mass_kg);
                }
                Err(_) => self.unbracketable_failure = true,
            }
            if let Some(cache) = self.cache.as_mut() {
                cache.insert(key, answer.clone());
            }
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_plan::LegEstimate;
    use std::cell::Cell;
    struct BoundaryModel {
        calls: Cell<usize>,
        cancelled: Cell<bool>,
        deterministic: bool,
        /// Evaluable below 21 t and again in 24-25 t, not only below 23 t.
        islands: bool,
    }
    impl FuelBurnModel for BoundaryModel {
        fn deterministic_for_dispatch(&self) -> bool {
            self.deterministic
        }
        fn check_cancellation(&self) -> Result<(), FuelModelError> {
            if self.cancelled.get() {
                Err(FuelModelError::Cancelled)
            } else {
                Ok(())
            }
        }
        fn trip(&self, mass: f64, _: f64) -> Result<LegEstimate, FuelModelError> {
            self.calls.set(self.calls.get() + 1);
            let unevaluable = if self.islands {
                !(mass <= 21000.0 || (24000.0..=25000.0).contains(&mass))
            } else {
                mass > 23000.0
            };
            if unevaluable {
                Err(FuelModelError::NotConverged("climb energy deficit".into()))
            } else {
                Ok(LegEstimate {
                    fuel_kg: 5000.0,
                    time_s: 5000.0,
                })
            }
        }
        fn diversion(&self, _: f64, _: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.0,
                time_s: 0.0,
            })
        }
        fn holding_fuel_flow_kg_s(&self, _: f64, _: f64) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
        fn cruise_fuel_flow_kg_s(&self, _: f64) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
    }
    fn model(deterministic: bool) -> BoundaryModel {
        BoundaryModel {
            calls: Cell::new(0),
            cancelled: Cell::new(false),
            deterministic,
            islands: false,
        }
    }
    fn islands(deterministic: bool) -> BoundaryModel {
        BoundaryModel {
            islands: true,
            ..model(deterministic)
        }
    }
    fn policy() -> FuelPolicyConfig {
        FuelPolicyConfig {
            scheme: alas_config::FuelScheme::TripFuelOnly,
            ..FuelPolicyConfig::default()
        }
    }
    #[test]
    fn exact_dispatch_memo_preserves_every_iterate_and_status() {
        let uncached = model(false);
        let cached = model(true);
        let limits = crate::dispatch::DispatchLimits {
            mtow_kg: 30000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let run = |model: &BoundaryModel| {
            crate::dispatch::solve_dispatch(19500.0, 627000.0, &policy(), model, &limits, 60, 0.01)
        };
        assert_eq!(run(&cached), run(&uncached));
        assert!(
            cached.calls.get() * 2 < uncached.calls.get(),
            "cached {} vs uncached {}",
            cached.calls.get(),
            uncached.calls.get()
        );
    }
    /// Successive overshoots of the Picard iteration walk down to the same
    /// edge of the evaluable region. Once the priced masses fix that edge the
    /// bisection reads its midpoints from them (`PlanEvaluator::inferred`)
    /// and prices only the mass it returns: every iterate, the status and the
    /// plan are those of the unmemoized bisection, in a fraction of the model
    /// calls, for any seed above the edge.
    #[test]
    fn inferred_midpoints_leave_the_dispatch_unchanged_and_price_far_fewer_masses() {
        for (zero_fuel_kg, mtow_kg) in [(19500.0, 30000.0), (20000.0, 31000.0), (18500.0, 40000.0)]
        {
            let limits = crate::dispatch::DispatchLimits {
                mtow_kg,
                mzfw_kg: None,
                mlw_kg: None,
                usable_capacity_kg: None,
            };
            let uncached = model(false);
            let cached = model(true);
            let run = |model: &BoundaryModel| {
                crate::dispatch::solve_dispatch(
                    zero_fuel_kg,
                    627000.0,
                    &policy(),
                    model,
                    &limits,
                    60,
                    0.01,
                )
            };
            let (cached_solution, uncached_solution) = (run(&cached), run(&uncached));
            assert_eq!(
                cached_solution, uncached_solution,
                "zero-fuel {zero_fuel_kg}"
            );
            assert!(
                cached.calls.get() * 2 < uncached.calls.get(),
                "cached {} vs uncached {}",
                cached.calls.get(),
                uncached.calls.get()
            );
        }
    }
    /// A mission that needs more mass than the model can fly stalls the
    /// iteration at the edge of the evaluable region: it stops there with the
    /// typed status instead of re-pricing the edge until the iteration cap,
    /// and the cap changes nothing it reports.
    #[test]
    fn an_iteration_stalled_at_the_evaluable_edge_stops_and_reports_the_edge() {
        let limits = crate::dispatch::DispatchLimits {
            mtow_kg: 30000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        let run = |max_iterations: usize| {
            let model = model(true);
            let solution = crate::dispatch::solve_dispatch(
                19500.0,
                627000.0,
                &policy(),
                &model,
                &limits,
                max_iterations,
                0.01,
            );
            (solution, model.calls.get())
        };
        let (short, short_calls) = run(8);
        let (long, long_calls) = run(600);
        assert!(matches!(
            long.status,
            crate::dispatch::DispatchStatus::NotConverged { .. }
        ));
        assert!(long.iterates.len() < 8, "{} iterates", long.iterates.len());
        assert_eq!(short.status, long.status);
        assert_eq!(
            short.takeoff_mass_kg.to_bits(),
            long.takeoff_mass_kg.to_bits()
        );
        assert_eq!(short.plan, long.plan);
        assert_eq!(short_calls, long_calls);
        // The edge: the highest mass the model prices, to the bisection resolution.
        assert!(long.takeoff_mass_kg <= 23000.0 && long.takeoff_mass_kg > 22999.98);
    }
    /// An evaluable region that is not contiguous withdraws every inference:
    /// the bisection prices each midpoint itself, as it always did.
    #[test]
    fn a_non_contiguous_evaluable_region_withdraws_the_inference() {
        for mtow_kg in [26000.0, 40000.0, 55000.0] {
            let limits = crate::dispatch::DispatchLimits {
                mtow_kg,
                mzfw_kg: None,
                mlw_kg: None,
                usable_capacity_kg: None,
            };
            let uncached = islands(false);
            let cached = islands(true);
            let run = |model: &BoundaryModel| {
                crate::dispatch::solve_dispatch(
                    19500.0,
                    627000.0,
                    &policy(),
                    model,
                    &limits,
                    60,
                    0.01,
                )
            };
            assert_eq!(run(&cached), run(&uncached), "mtow {mtow_kg}");
        }
        let policy = policy();
        let model = islands(true);
        let mut plans = PlanEvaluator::new(&policy, &model, 627000.0);
        assert!(plans.evaluate(20000.0).is_ok());
        assert!(plans.evaluate(30000.0).is_err());
        assert_eq!(plans.inferred(10000.0), Some(Inferred::Evaluable));
        assert_eq!(plans.inferred(35000.0), Some(Inferred::Unevaluable));
        assert!(plans.evaluate(22000.0).is_err());
        assert!(plans.evaluate(24500.0).is_ok());
        assert_eq!(plans.inferred(10000.0), None);
        assert_eq!(plans.inferred(35000.0), None);
    }
    #[test]
    fn cancellation_precedes_cached_answer_and_models_must_opt_in() {
        let policy = policy();
        let cached = model(true);
        let mut plans = PlanEvaluator::new(&policy, &cached, 627000.0);
        assert!(plans.evaluate(22000.0).is_ok());
        cached.cancelled.set(true);
        assert!(matches!(
            plans.evaluate(22000.0),
            Err(FuelModelError::Cancelled)
        ));
        cached.cancelled.set(false);
        assert!(plans.evaluate(22000.0).is_ok());
        assert_eq!(cached.calls.get(), 1);
        let uncached = model(false);
        let mut plans = PlanEvaluator::new(&policy, &uncached, 627000.0);
        assert!(plans.evaluate(22000.0).is_ok());
        assert!(plans.evaluate(22000.0).is_ok());
        assert_eq!(uncached.calls.get(), 2);
    }
}
