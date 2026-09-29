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
            if mass > 23000.0 {
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
            cached.calls.get() * 10 < uncached.calls.get(),
            "cached {} vs uncached {}",
            cached.calls.get(),
            uncached.calls.get()
        );
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
