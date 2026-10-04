// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Read-only evidence from a search, including unsuccessful restoration.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::{
    DesignOptimizer, DesignVector, OptimizationError, OptimizationHistory, OptimizationResult,
};

impl DesignOptimizer {
    /// Run the same native search and retain failed candidates for review.
    ///
    /// This diagnostic outcome is not a deliverable design. Its evidence keeps
    /// the least-violating candidate and complete history when the strict
    /// [`Self::run`] would return [`OptimizationError::NoFeasibleDesign`].
    /// Invalid requests still return an error before search begins.
    pub fn run_diagnostics(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> Result<DiagnosticSearchOutcome, OptimizationError> {
        self.run_diagnostics_cancellable(bounds, initial_design, progress_callback, None)
    }

    /// Cancellable diagnostic search, retaining completed evidence without
    /// interpreting a stopped run as evidence of physical infeasibility.
    /// Invalid requests and worker-resource failures remain typed errors.
    pub fn run_diagnostics_cancellable(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> Result<DiagnosticSearchOutcome, OptimizationError> {
        self.run_native_search(bounds, initial_design, progress_callback, cancel)
            .map(DiagnosticSearchOutcome::new)
    }
}

/// Explicit additional work spent restoring a failed evolutionary search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestorationDiagnostics {
    /// Upper bound on requested scores, inside the refinement budget.
    pub evaluation_budget: usize,
    /// Actual coupled analyses; exact cache reuse is excluded.
    pub analysis_evaluations: usize,
    /// Scores reused from the full-fidelity exact cache.
    pub cache_hits: usize,
    /// Completed coordinate-poll waves, at most four.
    pub iterations: usize,
    /// Measured elapsed wall time in seconds.
    pub wall_time_s: f64,
    /// Aggregate normalized hard violation before restoration.
    pub initial_violation: f64,
    /// Aggregate normalized hard violation afterwards.
    pub final_violation: f64,
    /// Final poll radius as a fraction of each original bound width.
    pub final_radius_normalized: f64,
    /// Whether restoration found a fully evaluated feasible candidate.
    pub feasible: bool,
}

/// Evidence returned when no evaluated candidate passed the active validity
/// policy. Rejected candidates are never promoted to deliverable results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoFeasibleDesign {
    /// Number of candidates evaluated before the search ended.
    pub evaluated_candidates: usize,
    /// Counts of the machine-readable rejection categories observed.
    pub rejection_reason_counts: BTreeMap<String, usize>,
}

impl NoFeasibleDesign {
    pub(super) fn from_history(history: &OptimizationHistory) -> Self {
        let mut rejection_reason_counts = BTreeMap::new();
        for reason in &history.reject_reason {
            for category in reason.split('+').filter(|category| !category.is_empty()) {
                *rejection_reason_counts
                    .entry(category.to_owned())
                    .or_insert(0) += 1;
            }
        }
        if rejection_reason_counts.is_empty() && history.n_evaluations() > 0 {
            rejection_reason_counts.insert("unknown".to_owned(), history.n_evaluations());
        }
        Self {
            evaluated_candidates: history.n_evaluations(),
            rejection_reason_counts,
        }
    }
}

/// Diagnostic evidence, deliberately separate from a deliverable optimization
/// result. Invalid candidates remain available for residual inspection and
/// reproducible follow-up searches; they are never promoted to feasible designs.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticSearchOutcome {
    search: OptimizationResult,
    rejection: Option<NoFeasibleDesign>,
}

impl DiagnosticSearchOutcome {
    pub(super) fn new(mut search: OptimizationResult) -> Self {
        let rejection = (!search.best_valid && !search.was_cancelled()).then(|| {
            if let Some(diagnostics) = search.search_diagnostics.as_mut() {
                diagnostics.converged = false;
            }
            search.termination = "no_feasible_design".to_owned();
            NoFeasibleDesign::from_history(&search.history)
        });
        Self { search, rejection }
    }

    /// Borrow complete candidate history and search accounting for review.
    /// `best_valid` is the actual evaluator verdict, including on failure.
    pub fn evidence(&self) -> &OptimizationResult {
        &self.search
    }

    /// Failure summary identical to the strict optimizer's rejection.
    pub fn rejection(&self) -> Option<&NoFeasibleDesign> {
        self.rejection.as_ref()
    }
}
