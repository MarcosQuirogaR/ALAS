// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A compact record of every candidate a run asked for, across every stage.
//!
//! [`crate::OptimizationHistory`] holds one full row per analysis of the
//! refinement's model. It leaves out what a reader needs to see how much of
//! the design space a run explored: candidates the design-vector pre-gate
//! rejected before any analysis, the screening stage when it runs its own
//! (cheaper) model, and the reporting-fidelity re-evaluation of the
//! finalists. [`EvaluationTrace`] keeps one small record per such candidate,
//! in request order, for display only. Nothing in the search reads it, so
//! recording it cannot change what the search does or returns.
//!
//! A record is 20 bytes; a run of 40 000 requested candidates costs about
//! 0.8 MB. The worst case is bounded by the stage caps: each stage records
//! at most its evaluation ceiling plus its pre-gate rejection cap
//! (`PREGATE_REJECTS_PER_EVALUATION` = 20 times that ceiling), so the default
//! ceilings of 20 000 per stage bound a run at about 840 000 records, about
//! 17 MB, plus a handful of verifications. A larger configured ceiling
//! raises the bound in proportion.
//!
//! Within one batch the pre-gate rejections are recorded before the analyses
//! of the same batch, which run concurrently and have no order of their own.
//! Repeated designs served from a stage's exact cache, and candidates a
//! cancellation stopped before any analysis started, are not evaluations
//! and are not recorded.

use alas_config::design_variables::DesignVector;
use serde::{Deserialize, Serialize};

use crate::history::OptimizationHistory;
use crate::search_methods::Tier;

/// The stage of the run a candidate was requested by, in run order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStage {
    /// Space-filling scan of the design box, possibly on a cheaper model.
    Screening,
    /// The refinement kernel, including its pilot of the screening elite.
    Refinement,
    /// Bounded feasibility restoration inside the refinement budget.
    Restoration,
    /// Reporting-fidelity re-evaluation of the ranked finalists.
    Verification,
}

impl TraceStage {
    /// Every stage, in run order.
    pub const ALL: [Self; 4] = [
        Self::Screening,
        Self::Refinement,
        Self::Restoration,
        Self::Verification,
    ];

    /// Short display tag of the stage.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Screening => "Screening",
            Self::Refinement => "Refinement",
            Self::Restoration => "Restoration",
            Self::Verification => "Verification",
        }
    }
}

/// What became of a requested candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceClass {
    /// Passed the stage's validity policy.
    Valid,
    /// Refused: by the design-vector pre-gate, by a violated hard residual of
    /// a closed analysis, or by the reporting-fidelity assessment.
    Rejected,
    /// The analysis failed or its sizing closure did not converge.
    Failed,
}

/// One requested candidate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TracedEvaluation {
    /// Stage that requested it.
    pub stage: TraceStage,
    /// Its outcome.
    pub class: TraceClass,
    /// The physical objective in the objective's own unit (for example kg of
    /// block fuel), as the requesting stage's model computed it; `None` when
    /// the model produced no finite value. A verification record carries the
    /// search model's value of the same candidate.
    pub objective: Option<f32>,
    /// Dimensionless ranking cost; `None` for a failure sentinel, a
    /// pre-gate rejection or a non-finite value.
    pub cost: Option<f32>,
}

/// Every candidate a run requested, in request order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationTrace {
    /// The records, in request order.
    pub evaluations: Vec<TracedEvaluation>,
    /// Whether the screening records were scored by a model distinct from
    /// the refinement's (the shipped cheaper screening model). Their
    /// objectives are then not comparable with the refinement's.
    #[serde(default)]
    pub screening_separate: bool,
}

/// The history reason of a candidate a cancellation stopped before any
/// analysis started; the batch evaluator records it under the same text.
pub(crate) const CANCELLED_UNSTARTED: &str = "cancelled_unstarted";

/// `value` as `f32` when it is finite in both precisions.
pub(crate) fn finite_f32(value: f64) -> Option<f32> {
    let narrowed = value as f32;
    narrowed.is_finite().then_some(narrowed)
}

impl EvaluationTrace {
    /// Number of recorded candidates.
    pub fn len(&self) -> usize {
        self.evaluations.len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.evaluations.is_empty()
    }

    /// Append one record.
    pub fn push(
        &mut self,
        stage: TraceStage,
        class: TraceClass,
        objective: Option<f32>,
        cost: Option<f32>,
    ) {
        self.evaluations.push(TracedEvaluation {
            stage,
            class,
            objective,
            cost,
        });
    }

    /// Append every record of `other`, keeping its order.
    pub fn append(&mut self, mut other: Self) {
        self.screening_separate |= other.screening_separate;
        self.evaluations.append(&mut other.evaluations);
    }

    /// Number of records of `class`.
    pub fn count(&self, class: TraceClass) -> usize {
        self.evaluations
            .iter()
            .filter(|evaluation| evaluation.class == class)
            .count()
    }

    /// The trace a history alone supports: every row as a refinement
    /// analysis. Used for a result that carries no trace of its own.
    pub fn from_history(history: &OptimizationHistory) -> Self {
        let evaluations = (0..history.n_evaluations())
            .filter_map(|row| analysed_record(history, row, TraceStage::Refinement))
            .collect();
        Self {
            evaluations,
            screening_separate: false,
        }
    }
}

/// The record of the analysis in history row `row`, ranked the way the
/// search ranks it (`candidate_tier`); `None` for a candidate a cancellation
/// stopped before it started.
///
/// A row the search ranks as not closed is a failure, except a delegated
/// evaluator's refusal: it reports a lift-to-drag ratio and no residual
/// table, so its analysis completed and the evaluator rejected the design.
pub(crate) fn analysed_record(
    history: &OptimizationHistory,
    row: usize,
    stage: TraceStage,
) -> Option<TracedEvaluation> {
    let reason = history.reject_reason.get(row).map_or("", String::as_str);
    if reason == CANCELLED_UNSTARTED {
        return None;
    }
    let cost = history.cost.get(row).copied().unwrap_or(f64::NAN);
    let valid = history.valid.get(row).copied().unwrap_or(false) && cost.is_finite();
    let hard_violation = history.hard_violation.get(row).copied().unwrap_or(f64::NAN);
    let l_over_d = history.l_over_d.get(row).copied().unwrap_or(f64::NAN);
    let delegated_refusal = l_over_d.is_finite() && l_over_d > 0.0 && hard_violation == 0.0;
    let tier = crate::differential_evolution::candidate_tier(
        valid,
        reason,
        if hard_violation.is_finite() && hard_violation >= 0.0 {
            hard_violation
        } else {
            f64::NAN
        },
    );
    let class = match tier {
        Tier::Feasible => TraceClass::Valid,
        Tier::ClosedInfeasible | Tier::PreGateFailed => TraceClass::Rejected,
        Tier::NotClosed if delegated_refusal => TraceClass::Rejected,
        Tier::NotClosed => TraceClass::Failed,
    };
    Some(TracedEvaluation {
        stage,
        class,
        objective: history
            .objective_value
            .get(row)
            .copied()
            .and_then(finite_f32),
        cost: (class != TraceClass::Failed)
            .then(|| finite_f32(cost))
            .flatten(),
    })
}

impl crate::OptimizationResult {
    /// The run's trace: the search's own when it recorded one, else the one
    /// its history supports ([`EvaluationTrace::from_history`]).
    pub fn evaluation_trace(&self) -> EvaluationTrace {
        self.search_diagnostics
            .as_ref()
            .map(|diagnostics| &diagnostics.evaluation_trace)
            .filter(|trace| !trace.is_empty())
            .cloned()
            .unwrap_or_else(|| EvaluationTrace::from_history(&self.history))
    }

    /// Record the reporting-fidelity re-evaluation of `design`, rank `rank`
    /// of the verification ladder, with the search model's objective of the
    /// same candidate. A result without search diagnostics records nothing.
    pub fn record_verification(&mut self, rank: usize, design: &DesignVector, class: TraceClass) {
        let Some(diagnostics) = self.search_diagnostics.as_mut() else {
            return;
        };
        let history = &self.history;
        let row = if rank == 0 {
            diagnostics.winner_history_row
        } else {
            None
        }
        .or_else(|| {
            history
                .design_vectors
                .iter()
                .position(|candidate| candidate == design)
        });
        let value_at = |values: &[f64]| row.and_then(|row| values.get(row).copied());
        diagnostics.evaluation_trace.push(
            TraceStage::Verification,
            class,
            value_at(&history.objective_value).and_then(finite_f32),
            value_at(&history.cost).and_then(finite_f32),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_history_trace_classifies_valid_closed_misses_and_failures() {
        let mut history = OptimizationHistory::new();
        let dv = DesignVector::default();
        history.record_mission_sized(
            dv, true, 0.5, 17.0, 30.0, 2.0, 120.0, 1.0, "", 7_000.0, 70_000.0, 7_000.0, 0.0, 0.0,
        );
        history.record_mission_sized(
            dv, false, 2.5, 17.0, 30.0, 2.0, 120.0, 1.0, "mtow", 9_000.0, 90_000.0, 9_000.0, 0.4,
            0.0,
        );
        history.record_mission_sized(
            dv,
            false,
            1.0e6,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            "trim_solve",
            f64::NAN,
            f64::NAN,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        );
        history.record(
            dv,
            false,
            1.0e6,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            "cancelled_unstarted",
        );
        let trace = EvaluationTrace::from_history(&history);
        assert_eq!(trace.len(), 3);
        assert_eq!(trace.count(TraceClass::Valid), 1);
        assert_eq!(trace.count(TraceClass::Rejected), 1);
        assert_eq!(trace.count(TraceClass::Failed), 1);
        assert_eq!(trace.evaluations[1].objective, Some(9_000.0));
        assert_eq!(trace.evaluations[2].objective, None);
        assert_eq!(
            trace.evaluations[2].cost, None,
            "a failure sentinel is no cost"
        );
    }

    #[test]
    fn narrowing_keeps_only_values_finite_in_single_precision() {
        assert_eq!(finite_f32(1.5), Some(1.5));
        assert_eq!(finite_f32(f64::NAN), None);
        assert_eq!(finite_f32(1.0e300), None);
        assert_eq!(std::mem::size_of::<TracedEvaluation>(), 20);
    }
}
