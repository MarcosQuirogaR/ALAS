// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Solver-neutral objective values supplied to the design optimizer.
//!
//! The differential-evolution algorithm must not know whether a candidate was
//! evaluated by the native VLM, an external AVL process, or a test double.
//! Keeping this contract in `alas-opt` lets pipeline adapters own geometry and
//! process orchestration while the search retains one reproducible history and
//! one set of mutation and convergence rules.

use alas_config::design_variables::DesignVector;

/// One candidate result returned by an optimization backend.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveEvaluation {
    /// Scalar cost minimized by differential evolution.
    pub cost: f64,
    /// Whether the candidate passed the backend's active validity policy.
    pub valid: bool,
    /// Lift-to-drag ratio, when the backend computed one.
    pub l_over_d: f64,
    /// Wing span in meters.
    pub span_m: f64,
    /// Angle of attack in degrees.
    pub alpha_deg: f64,
    /// Reference wing area in square meters.
    pub area_m2: f64,
    /// Trim horizontal-stabilizer incidence in degrees.
    pub trim_ih_deg: f64,
    /// Stable machine-readable reason when the candidate is rejected.
    pub reject_reason: String,
}

impl ObjectiveEvaluation {
    /// Construct a rejected candidate while retaining the optimizer's failure
    /// cost and the same history shape as the native VLM objective.
    pub fn rejected(cost: f64, reason: impl Into<String>) -> Self {
        Self {
            cost,
            valid: false,
            l_over_d: 0.0,
            span_m: 0.0,
            alpha_deg: 0.0,
            area_m2: 0.0,
            trim_ih_deg: 0.0,
            reject_reason: reason.into(),
        }
    }
}

/// Evaluates one valid design vector using a concrete aerodynamic backend.
pub trait ObjectiveEvaluator {
    /// Evaluate `design` and return the scalar search cost plus diagnostics.
    fn evaluate(&mut self, design: &DesignVector) -> ObjectiveEvaluation;
}

impl<F> ObjectiveEvaluator for F
where
    F: FnMut(&DesignVector) -> ObjectiveEvaluation,
{
    fn evaluate(&mut self, design: &DesignVector) -> ObjectiveEvaluation {
        self(design)
    }
}
