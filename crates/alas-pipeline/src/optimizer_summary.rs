// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One adapter from the optimizer's result to what a user-facing surface
//! (desktop results view, report, CLI summary) states about the search.
//!
//! Every surface reads [`OptimizerRunSummary`] and never the optimizer result
//! directly, so a change in the optimizer's result layout is absorbed in
//! [`OptimizerRunSummary::from_result`] alone.

use alas_config::{AlasConfig, DesignVector};
use alas_opt::{
    BaselineComparison, OptimizationResult, ReportingBaseline, SearchDiagnostics, StageSummary,
};

use crate::dual_solver::SolverOptimizationStatus;

mod geometry;
mod labels;
pub mod objective;
pub use geometry::{
    GeometryComparison, AEROELASTIC_CAVEAT_TEXT, ASPECT_RATIO_FLAG_FRACTION,
    ASPECT_RATIO_FLAG_LABEL, BUFFET_BASIS_TEXT, SWEEP_FLAG_DEG, SWEEP_FLAG_LABEL,
};

/// What the search is: a bounded local refinement of the preset, not a
/// global search for the best aircraft of its class.
pub const SCOPE_LABEL: &str = "Local refinement around the preset";

/// The stopping rule of a run in which a stage may stop on its time limit.
pub const TIME_LIMITED_TEXT: &str = "Time-limited: the stopping point depends on machine speed and worker count; replay with the recorded evaluation counts for a bit-identical result at any worker count";

/// The row of the per-stage replay counts
/// ([`OptimizerRunSummary::replay_evaluations`]), each the value
/// `optimizer.solver.<stage>.replay_evaluations` takes to replay the run.
pub const REPLAY_COUNT_LABEL: &str =
    "Replay count (pre-gate-passed candidates, including repeats), screening / refinement";

/// The row of the refinement's planned budget against its configured
/// ceiling; a replay sets `replay_planned_evaluations` to the planned budget.
pub const PLANNED_BUDGET_LABEL: &str = "Refinement, planned evaluation budget / ceiling";

/// The stopping rule of a run in which every stage stops on evaluations.
pub const EVALUATIONS_ONLY_TEXT: &str =
    "Evaluation budgets only: bit-identical at any worker count";

/// The winner against the baseline design, both scored by the same in-loop
/// product model, so a difference between the two is like for like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineDelta {
    /// Mission objective of the baseline, in the objective's own unit.
    pub baseline_objective: f64,
    /// Mission objective of the winner under the same model.
    pub winner_objective: f64,
    /// Sized block fuel of the baseline, kg. `None` when not recorded.
    pub baseline_block_fuel_kg: Option<f64>,
    /// Sized block fuel of the winner, kg. `None` when not recorded.
    pub winner_block_fuel_kg: Option<f64>,
    /// Whether the baseline passed every hard constraint of the search model.
    pub baseline_valid: bool,
}

impl BaselineDelta {
    /// Winner minus baseline, as a fraction of the baseline objective.
    /// Negative means the winner is better for a minimised objective.
    /// `None` when the baseline objective is zero or non-finite.
    #[must_use]
    pub fn relative_objective_change(&self) -> Option<f64> {
        (self.baseline_objective.is_finite()
            && self.winner_objective.is_finite()
            && self.baseline_objective != 0.0)
            .then(|| (self.winner_objective - self.baseline_objective) / self.baseline_objective)
    }
}

impl From<&BaselineComparison> for BaselineDelta {
    fn from(comparison: &BaselineComparison) -> Self {
        Self {
            baseline_objective: comparison.baseline_objective_value,
            winner_objective: comparison.winner_objective_value,
            baseline_block_fuel_kg: comparison.baseline_block_fuel_kg,
            winner_block_fuel_kg: comparison.winner_block_fuel_kg,
            baseline_valid: comparison.baseline_feasible,
        }
    }
}

/// The reporting-fidelity work after the search, against the share of the
/// refinement budget reserved for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationCounts {
    /// Analyses run: the ladder candidates, the baseline and the one final
    /// analysis of the delivered design.
    pub analyses: usize,
    /// Refinement evaluations reserved for them.
    pub reserved: usize,
}

/// The search's own account of itself, as a user surface states it.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerRunSummary {
    /// Physical mission quantity recorded before ranking normalization.
    pub objective_kind: alas_config::ObjectiveKind,
    /// Stable identifier of the method that ran.
    pub method: String,
    /// Durable lifecycle reason the search stopped.
    pub termination: String,
    /// Whether the search reached its own convergence criterion.
    pub converged: bool,
    /// Whether the delivered design is feasible at reporting fidelity.
    pub delivered_feasible: bool,
    /// Status word for the run: never `Completed` for an infeasible design.
    pub status: SolverOptimizationStatus,
    /// Per-stage budget, use, wall time and termination, screening first.
    /// Each stage's `evaluations` is its replay count.
    pub stages: Vec<StageSummary>,
    /// Search wall-clock time, both stages and the verification, s.
    pub wall_time_s: f64,
    /// Seed the run replays from: the one it actually used when recorded,
    /// else the requested or configured one.
    pub seed: Option<u64>,
    /// Same-model baseline comparison, when the baseline was evaluated.
    pub baseline: Option<BaselineDelta>,
    /// Reporting-fidelity work after the search, when it ran.
    pub verification: Option<VerificationCounts>,
    /// The baseline re-evaluated at reporting fidelity beside the delivered
    /// design: the native-mission trip-fuel delta.
    pub reporting_baseline: Option<ReportingBaseline>,
    /// Whether the same-model baseline is a start the search clamped into
    /// explicit bounds rather than the unchanged preset
    /// ([`baseline_is_constrained_start`]).
    pub baseline_constrained: bool,
    /// The delivered wing against the baseline's, when both reports exist.
    pub geometry: Option<GeometryComparison>,
}

/// Whether the baseline of `diagnostics` is a constrained start, not the
/// unchanged preset: the optimizer clamped the preset into the search box.
#[must_use]
pub fn baseline_is_constrained_start(diagnostics: &SearchDiagnostics) -> bool {
    diagnostics.baseline_clamped
}

/// The same-model baseline compared with the design the run delivered.
///
/// The search's own comparison scores its winner, which is the delivered
/// design only when the finalist was accepted. A fallback delivery
/// (`DeliveredAcceptance::delivered_is_search_finalist == false`) shows the
/// delivered candidate's own history row instead; when that row cannot be
/// found the comparison is withheld rather than shown for a rejected design.
fn delivered_baseline_delta(
    result: &OptimizationResult,
    comparison: &BaselineComparison,
    delivered: Option<&DesignVector>,
) -> Option<BaselineDelta> {
    let delta = BaselineDelta::from(comparison);
    let is_fallback = result
        .delivered_acceptance
        .as_ref()
        .is_some_and(|acceptance| acceptance.verified && !acceptance.delivered_is_search_finalist);
    if !is_fallback {
        return Some(delta);
    }
    // The fuselage length and tail scale are derived from the other
    // coordinates (cabin sizing, tail volume solve), and the pipeline rebinds
    // the delivered vector to their re-derived values, so a row is identified
    // by the free coordinates alone.
    let bits = |design: &DesignVector| {
        DesignVector {
            fuselage_length_m: 0.0,
            tail_scale: 0.0,
            ..*design
        }
        .to_array()
        .into_iter()
        .map(f64::to_bits)
        .collect::<Vec<u64>>()
    };
    let target = bits(delivered?);
    let history = &result.history;
    let row = history
        .design_vectors
        .iter()
        .position(|design| bits(design) == target)?;
    let objective = history
        .objective_value
        .get(row)
        .copied()
        .filter(|value| value.is_finite())?;
    Some(BaselineDelta {
        winner_objective: objective,
        winner_block_fuel_kg: history
            .block_fuel_kg
            .get(row)
            .copied()
            .filter(|fuel| fuel.is_finite()),
        ..delta
    })
}

/// Plain-language text for each termination reason the search reports, as
/// `(reason, text)`. A reason not listed here is shown as reported.
pub const TERMINATION_TEXT: &[(&str, &str)] = &[
    ("converged", "Converged"),
    ("evaluation_budget", "Evaluation budget reached"),
    ("time_budget", "Time limit reached"),
    ("pregate_exhausted", "Pre-gate rejection cap reached"),
    ("stagnated", "Stopped improving"),
    ("cancelled", "Cancelled"),
    ("fixed_bounds", "No free design variable"),
    (
        "reporting_fidelity_rejected",
        "Rejected at reporting fidelity",
    ),
    (
        "reporting_fidelity_fallback",
        "Verified fallback candidate at reporting fidelity",
    ),
];

/// Plain-language text for `reason`; the raw reason when unlisted.
#[must_use]
pub fn termination_text(reason: &str) -> &str {
    TERMINATION_TEXT
        .iter()
        .find(|(listed, _)| *listed == reason)
        .map_or(reason, |(_, text)| text)
}

/// The `(evaluations, wall time, termination)` labels of one stage.
fn stage_labels(stage: &str, time_limited: bool) -> Option<[&'static str; 3]> {
    match (stage, time_limited) {
        ("screening", true) => Some([
            "Screening, evaluations used / budget",
            "Screening, wall time / limit",
            "Screening, termination",
        ]),
        ("screening", false) => Some([
            "Screening, evaluations used / budget",
            "Screening, wall time",
            "Screening, termination",
        ]),
        ("refinement", true) => Some([
            "Refinement, evaluations used / budget",
            "Refinement, wall time / limit",
            "Refinement, termination",
        ]),
        ("refinement", false) => Some([
            "Refinement, evaluations used / budget",
            "Refinement, wall time",
            "Refinement, termination",
        ]),
        _ => None,
    }
}

impl OptimizerRunSummary {
    /// Whether any stage could stop on its time limit.
    #[must_use]
    pub fn time_limited(&self) -> bool {
        self.stages.iter().any(|stage| stage.time_limited)
    }

    /// The replay count of `stage`: its pre-gate-passed candidates,
    /// including repeats ([`StageSummary::evaluations`]), not its coupled
    /// analyses.
    #[must_use]
    pub fn replay_evaluations(&self, stage: &str) -> Option<usize> {
        self.stages
            .iter()
            .find(|summary| summary.stage == stage)
            .map(|summary| summary.evaluations)
    }

    /// `field` of `stage` as text, `-` when the stage did not run.
    fn stage_value(&self, stage: &str, field: impl Fn(&StageSummary) -> usize) -> String {
        self.stages
            .iter()
            .find(|summary| summary.stage == stage)
            .map_or_else(|| "-".to_owned(), |summary| field(summary).to_string())
    }

    /// Plain-language termination reason; the raw reason when unlisted.
    #[must_use]
    pub fn termination_text(&self) -> &str {
        termination_text(&self.termination)
    }

    /// Summarize `result` under `config`; `seed_override` is the seed a
    /// caller requested for this run, used when the result does not record
    /// the seed it ran with.
    #[must_use]
    pub fn from_result(
        result: &OptimizationResult,
        config: &AlasConfig,
        seed_override: Option<u64>,
    ) -> Self {
        Self::from_delivered_result(result, config, seed_override, None)
    }

    /// [`Self::from_result`] for a run that delivered `delivered`, the design
    /// the pipeline reports on. A fallback delivery needs it to find the
    /// delivered candidate's own objective and block fuel.
    #[must_use]
    pub fn from_delivered_result(
        result: &OptimizationResult,
        config: &AlasConfig,
        seed_override: Option<u64>,
        delivered: Option<&DesignVector>,
    ) -> Self {
        let diagnostics = result.search_diagnostics.as_ref();
        let configured = config
            .optimizer
            .solver
            .seed
            .and_then(|seed| u64::try_from(seed).ok());
        let acceptance = result.delivered_acceptance.as_ref();
        Self {
            method: result.method.clone(),
            objective_kind: config.optimizer.objective.kind,
            termination: result.termination.clone(),
            converged: result.converged(),
            delivered_feasible: result.is_delivered_feasible(),
            status: SolverOptimizationStatus::for_delivered(result),
            stages: diagnostics.map_or_else(Vec::new, |d| d.stages.clone()),
            wall_time_s: result.wall_time_s,
            seed: diagnostics
                .and_then(|d| d.seed)
                .or(seed_override)
                .or(configured),
            baseline: diagnostics
                .and_then(|d| d.baseline.as_ref())
                .and_then(|comparison| delivered_baseline_delta(result, comparison, delivered)),
            verification: acceptance.map(|acceptance| VerificationCounts {
                analyses: acceptance.analyses + 1,
                reserved: alas_opt::verification_reserve(&config.optimizer.solver).evaluations,
            }),
            reporting_baseline: acceptance.and_then(|acceptance| acceptance.baseline.clone()),
            baseline_constrained: diagnostics.is_some_and(baseline_is_constrained_start),
            geometry: None,
        }
    }

    /// Summary of the optimization carried by a finished pipeline run.
    #[must_use]
    pub fn from_pipeline(result: &crate::PipelineResult) -> Option<Self> {
        let optimization = result.optimization_result.as_ref()?;
        let mut summary = Self::from_delivered_result(
            optimization,
            &result.config,
            result.execution.seed_requested,
            result.optimized_design.as_ref(),
        );
        summary.geometry = result
            .optimized_report
            .as_ref()
            .zip(result.baseline_analysis.as_ref())
            .and_then(|(winner, baseline)| GeometryComparison::of_reports(winner, baseline));
        Some(summary)
    }
}

// A test asserts on values it built here, so a failed expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
#[path = "optimizer_summary_tests.rs"]
mod tests;
