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
    /// Whether each candidate passed all physical validity checks.
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
