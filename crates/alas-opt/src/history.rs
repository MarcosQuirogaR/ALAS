// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/objective.py
// Reference: alas @ rust-port-baseline.

//! Recording evaluated design candidates and tracking convergence diagnostics.

use std::collections::HashMap;

use alas_config::design_variables::DesignVector;
use serde::{Deserialize, Serialize};

/// Records the trajectory of evaluated designs during an optimization run.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OptimizationHistory {
    /// Evaluated design vectors in evaluation order.
    pub design_vectors: Vec<DesignVector>,
    /// Whether each candidate passed the active optimizer validity policy.
    ///
    /// In unconstrained product searches, a completed finite-aero evaluation
    /// is valid even when the physical requirement diagnostics are non-empty.
    pub valid: Vec<bool>,
    /// Objective cost computed for each candidate.
    pub cost: Vec<f64>,
    /// Lift-to-drag ratio achieved by each candidate.
    pub l_over_d: Vec<f64>,
    /// Wing span in meters.
    pub span_m: Vec<f64>,
    /// Cruise angle of attack in degrees.
    pub alpha_deg: Vec<f64>,
    /// Reference wing area in square meters.
    pub area_m2: Vec<f64>,
    /// Trim horizontal-stabilizer incidence in degrees.
    pub trim_ih_deg: Vec<f64>,
    /// Reason string for invalid designs (e.g. `"static_margin+cg_envelope"`).
    pub reject_reason: Vec<String>,
    /// Mission-sized objective value before normalization; `NaN` for a
    /// legacy weighted-penalty evaluation.
    #[serde(default)]
    pub objective_value: Vec<f64>,
    /// Sized takeoff mass, kg; `NaN` for a legacy weighted-penalty evaluation.
    #[serde(default)]
    pub takeoff_mass_kg: Vec<f64>,
    /// Sized block fuel, kg; `NaN` for a legacy weighted-penalty evaluation.
    #[serde(default)]
    pub block_fuel_kg: Vec<f64>,
    /// Sum of normalized hard-constraint violations; `0.0` for a legacy
    /// weighted-penalty evaluation.
    #[serde(default)]
    pub hard_violation: Vec<f64>,
    /// Sum of normalized soft-constraint violations; `0.0` for a legacy
    /// weighted-penalty evaluation.
    #[serde(default)]
    pub soft_violation: Vec<f64>,
}

impl OptimizationHistory {
    /// Create a new empty history recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a single evaluation step.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        dv: DesignVector,
        valid: bool,
        cost: f64,
        ld: f64,
        span: f64,
        alpha: f64,
        area: f64,
        trim_ih: f64,
        reason: impl Into<String>,
    ) {
        self.design_vectors.push(dv);
        self.valid.push(valid);
        self.cost.push(cost);
        self.l_over_d.push(ld);
        self.span_m.push(span);
        self.alpha_deg.push(alpha);
        self.area_m2.push(area);
        self.trim_ih_deg.push(trim_ih);
        self.reject_reason.push(reason.into());
        // Keeps the mission-sized vectors aligned with every other one even
        // when the evaluation that just ran is a legacy weighted-penalty
        // candidate that never computed them.
        self.objective_value.push(f64::NAN);
        self.takeoff_mass_kg.push(f64::NAN);
        self.block_fuel_kg.push(f64::NAN);
        self.hard_violation.push(0.0);
        self.soft_violation.push(0.0);
    }

    /// Record a mission-sized evaluation step.
    ///
    /// Pushes the same legacy fields [`Self::record`] does, then overwrites
    /// the sentinel it just pushed into the five mission-sized vectors with
    /// the real values, which is what keeps every vector's length identical
    /// without duplicating the legacy push logic.
    #[allow(clippy::too_many_arguments)] // mirrors `record`'s own arity plus the five mission-sized fields
    pub fn record_mission_sized(
        &mut self,
        dv: DesignVector,
        valid: bool,
        cost: f64,
        ld: f64,
        span: f64,
        alpha: f64,
        area: f64,
        trim_ih: f64,
        reason: impl Into<String>,
        objective_value: f64,
        takeoff_mass_kg: f64,
        block_fuel_kg: f64,
        hard_violation: f64,
        soft_violation: f64,
    ) {
        self.record(dv, valid, cost, ld, span, alpha, area, trim_ih, reason);
        if let Some(last) = self.objective_value.last_mut() {
            *last = objective_value;
        }
        if let Some(last) = self.takeoff_mass_kg.last_mut() {
            *last = takeoff_mass_kg;
        }
        if let Some(last) = self.block_fuel_kg.last_mut() {
            *last = block_fuel_kg;
        }
        if let Some(last) = self.hard_violation.last_mut() {
            *last = hard_violation;
        }
        if let Some(last) = self.soft_violation.last_mut() {
            *last = soft_violation;
        }
    }

    /// Append another evaluation trace while preserving its evaluation order.
    ///
    /// The native differential-evolution objective uses this when worker
    /// threads evaluate disjoint candidate batches. Keeping the merge here
    /// makes it impossible to append only part of the parallel history.
    pub(crate) fn append(&mut self, mut other: Self) {
        self.design_vectors.append(&mut other.design_vectors);
        self.valid.append(&mut other.valid);
        self.cost.append(&mut other.cost);
        self.l_over_d.append(&mut other.l_over_d);
        self.span_m.append(&mut other.span_m);
        self.alpha_deg.append(&mut other.alpha_deg);
        self.area_m2.append(&mut other.area_m2);
        self.trim_ih_deg.append(&mut other.trim_ih_deg);
        self.reject_reason.append(&mut other.reject_reason);
        self.objective_value.append(&mut other.objective_value);
        self.takeoff_mass_kg.append(&mut other.takeoff_mass_kg);
        self.block_fuel_kg.append(&mut other.block_fuel_kg);
        self.hard_violation.append(&mut other.hard_violation);
        self.soft_violation.append(&mut other.soft_violation);
    }

    /// Number of total evaluations recorded.
    pub fn n_evaluations(&self) -> usize {
        self.cost.len()
    }

    /// Number of valid evaluations recorded.
    pub fn n_valid(&self) -> usize {
        self.valid.iter().filter(|&&v| v).count()
    }

    /// Counts occurrences of each individual rejection reason.
    ///
    /// Compound reasons separated by `+` are split into individual category counts.
    pub fn reject_reason_counts(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for compound in &self.reject_reason {
            if compound.is_empty() {
                continue;
            }
            for reason in compound.split('+') {
                if !reason.is_empty() {
                    *counts.entry(reason.to_owned()).or_insert(0) += 1;
                }
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_vector_lengths(history: &OptimizationHistory) -> Vec<usize> {
        vec![
            history.design_vectors.len(),
            history.valid.len(),
            history.cost.len(),
            history.l_over_d.len(),
            history.span_m.len(),
            history.alpha_deg.len(),
            history.area_m2.len(),
            history.trim_ih_deg.len(),
            history.reject_reason.len(),
            history.objective_value.len(),
            history.takeoff_mass_kg.len(),
            history.block_fuel_kg.len(),
            history.hard_violation.len(),
            history.soft_violation.len(),
        ]
    }

    #[test]
    fn a_legacy_record_pushes_sentinels_into_the_mission_sized_vectors() {
        let mut history = OptimizationHistory::new();
        history.record(
            DesignVector::default(),
            true,
            1.0,
            20.0,
            60.0,
            2.0,
            400.0,
            1.0,
            "",
        );
        let lengths = all_vector_lengths(&history);
        assert!(lengths.iter().all(|&len| len == 1), "{lengths:?}");
        assert!(history.objective_value[0].is_nan());
        assert!(history.takeoff_mass_kg[0].is_nan());
        assert!(history.block_fuel_kg[0].is_nan());
        assert_eq!(history.hard_violation[0], 0.0);
        assert_eq!(history.soft_violation[0], 0.0);
    }

    #[test]
    fn record_mission_sized_overwrites_the_sentinels_it_just_pushed() {
        let mut history = OptimizationHistory::new();
        history.record_mission_sized(
            DesignVector::default(),
            false,
            5.0,
            18.0,
            65.0,
            2.5,
            420.0,
            1.5,
            "mtow_ceiling",
            12_000.0,
            80_000.0,
            9_500.0,
            0.4,
            0.1,
        );
        assert_eq!(history.objective_value, vec![12_000.0]);
        assert_eq!(history.takeoff_mass_kg, vec![80_000.0]);
        assert_eq!(history.block_fuel_kg, vec![9_500.0]);
        assert_eq!(history.hard_violation, vec![0.4]);
        assert_eq!(history.soft_violation, vec![0.1]);
        assert_eq!(history.reject_reason, vec!["mtow_ceiling".to_owned()]);
    }

    #[test]
    fn appending_a_mixed_batch_keeps_every_vector_the_same_length_in_order() {
        let mut first = OptimizationHistory::new();
        first.record(
            DesignVector::default(),
            true,
            1.0,
            20.0,
            60.0,
            2.0,
            400.0,
            1.0,
            "",
        );
        let mut second = OptimizationHistory::new();
        second.record_mission_sized(
            DesignVector::default(),
            false,
            5.0,
            18.0,
            65.0,
            2.5,
            420.0,
            1.5,
            "mtow_ceiling",
            12_000.0,
            80_000.0,
            9_500.0,
            0.4,
            0.1,
        );
        first.append(second);
        let lengths = all_vector_lengths(&first);
        assert!(lengths.iter().all(|&len| len == 2), "{lengths:?}");
        assert!(first.objective_value[0].is_nan());
        assert_eq!(first.objective_value[1], 12_000.0);
        assert_eq!(
            first.reject_reason,
            vec![String::new(), "mtow_ceiling".to_owned()]
        );
    }
}
