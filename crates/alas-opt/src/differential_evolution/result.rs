// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result, diagnostics and error types of the differential-evolution optimizer.

use alas_config::design_variables::DesignVector;
use serde::{Deserialize, Serialize};

use super::{
    BaselineComparison, NoFeasibleDesign, ReportingBaseline, RestorationDiagnostics,
    StageRejections, StageSummary,
};
use crate::history::OptimizationHistory;

/// One member of a retained multi-objective Pareto set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoCandidate {
    /// Candidate design vector.
    pub design: DesignVector,
    /// Scalar objective retained for deterministic downstream winner selection.
    pub cost: f64,
    /// Mission objective value minimised (the cost under a delegated evaluator).
    pub objective_value: f64,
    /// Wing span objective, in meters.
    pub span_m: f64,
    /// Wing reference area objective, in square meters.
    pub area_m2: f64,
    /// Whether every product feasibility check passed.
    pub valid: bool,
}

/// Outcome of a design optimization run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Winning design candidate found by the optimizer.
    pub best_design: DesignVector,
    /// Objective function cost of the winning design.
    pub best_cost: f64,
    /// Whether the winning design passed the active evaluator policy.
    ///
    /// Native product candidates must pass every active hard constraint,
    /// including the structural physics gates in every objective mode.
    /// This is the search model's verdict, not final reporting-fidelity or
    /// external-solver validation; see [`Self::delivered_acceptance`].
    #[serde(default)]
    pub best_valid: bool,
    /// Full evaluation history collected during the run.
    pub history: OptimizationHistory,
    /// Elapsed wall-clock time in seconds.
    pub wall_time_s: f64,
    /// Stable identifier of the search method that produced this result.
    #[serde(default = "default_result_method")]
    pub method: String,
    /// Mutation/crossover strategy label of the search kernel.
    #[serde(default = "default_result_strategy")]
    pub strategy: String,
    /// Durable search lifecycle reason. For the product search this is the
    /// refinement's own: `converged`, `stagnated`, `evaluation_budget`,
    /// `time_budget` or `cancelled`; it is kept separate from the strategy
    /// label so a budget stop is not mistaken for convergence.
    #[serde(default = "default_result_termination")]
    pub termination: String,
    /// Final nondominated set for a multi-objective method.
    #[serde(default)]
    pub pareto_front: Vec<ParetoCandidate>,
    /// Measured search lifecycle, for a caller that needs to distinguish a
    /// converged run from one that stopped on a budget or a safety limit.
    /// Absent on a result deserialized from a file saved before the record
    /// existed.
    #[serde(default)]
    pub search_diagnostics: Option<SearchDiagnostics>,
    /// What the application's own reporting-fidelity re-evaluation made of
    /// the design this result delivers.  Absent when no caller performed one;
    /// present and authoritative when one did.
    #[serde(default)]
    pub delivered_acceptance: Option<DeliveredAcceptance>,
}

/// What one product search actually did, measured rather than configured.
///
/// The point of this record is that "converged in N seconds" can be checked:
/// it carries the termination verdict, the analyses that verdict was paid for
/// with, and the wall-clock split between the screening and refinement
/// stages ([`Self::stages`] has the full per-stage accounting).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchDiagnostics {
    /// Bounded feasibility restoration, inside the refinement budget.
    #[serde(default)]
    pub restoration: Option<RestorationDiagnostics>,
    /// Whether the refinement reached its convergence criterion. A budget,
    /// time, stagnation or cancellation stop is `false`.
    pub converged: bool,
    /// Coupled full-fidelity analyses executed by the refinement.
    pub analysis_evaluations: usize,
    /// Repeated designs served from the refinement's exact cache.
    pub cache_hits: usize,
    /// Refinement generations completed.
    pub poll_iterations: usize,
    /// Candidates the screening stage analysed (passed the pre-gate). With the shipped
    /// screening model (the refinement's own) they are part of the run's
    /// history; a distinct screening model keeps them out of it.
    pub screening_evaluations: usize,
    /// Screened candidates feasible under the screening model.
    pub screening_feasible: usize,
    /// Screening elite designs scored on the refinement path. Separate
    /// screening requires full-model analyses; a shared model reuses its
    /// scores. This is distinct from reporting-mesh finalist verification.
    pub verification_evaluations: usize,
    /// Wall-clock seconds in the screening stage.
    pub scan_wall_time_s: f64,
    /// Wall-clock seconds in the refinement stage.
    pub search_wall_time_s: f64,
    /// Worker threads a batch was spread across.
    pub workers: usize,
    /// Refinement initial population, set from the refinement budget and
    /// independent of `workers`.
    pub poll_block_size: usize,
    /// Objective of the first feasible point the refinement reached.
    pub first_feasible_cost: Option<f64>,
    /// Relative improvement of the winner over that first feasible point,
    /// dimensionless.
    pub relative_improvement: Option<f64>,
    /// Fraction of the final refinement population that was strictly
    /// feasible. This remains the population statistic when a later
    /// restoration succeeds; see `restoration.feasible` for that verdict.
    #[serde(default)]
    pub feasible_fraction: f64,
    /// The epsilon-constrained method's boundary at the last generation
    /// evaluated, `0` once past the epsilon control fraction of the budget
    /// (see `search_methods::lshade_de`). Deb's ordinary feasibility rule
    /// applies exactly when this is `0`.
    #[serde(default)]
    pub epsilon_level: f64,
    /// One entry per search stage, in run order: screening, refinement.
    #[serde(default)]
    pub stages: Vec<StageSummary>,
    /// The design-vector pre-gate of each stage in [`Self::stages`], same
    /// order: its rejection cap and the rejections by reason.
    #[serde(default)]
    pub rejections: Vec<StageRejections>,
    /// The seed the run replays from; recorded for an unseeded run too.
    #[serde(default)]
    pub seed: Option<u64>,
    /// What the search claims to be (`SEARCH_SCOPE`).
    #[serde(default)]
    pub scope: String,
    /// The winner against the baseline under the same model, when a
    /// baseline was evaluated.
    #[serde(default)]
    pub baseline: Option<BaselineComparison>,
    /// Whether the baseline lay outside the search box and the search
    /// started from it clamped into the box: [`Self::baseline`] then
    /// compares the winner with that constrained start, not with the
    /// unchanged baseline, and must be labelled so.
    #[serde(default)]
    pub baseline_clamped: bool,
    /// The history row the winner was analysed in. Its vector can differ
    /// from `best_design` where the evaluator derives a coordinate (the
    /// tail scale of a reference adaptation), so the row is recorded rather
    /// than searched for.
    #[serde(default)]
    pub winner_history_row: Option<usize>,
    /// Every candidate the run requested, across every stage, for display
    /// ([`crate::EvaluationTrace`]).
    #[serde(default)]
    pub evaluation_trace: crate::EvaluationTrace,
}

/// Termination label for a run whose delivered design was rejected by the
/// reporting-fidelity re-evaluation.
pub const REPORTING_FIDELITY_REJECTED: &str = "reporting_fidelity_rejected";

/// Termination label for a run that delivered a verified design other than
/// the finalist the search's own convergence certificate belongs to.
pub const REPORTING_FIDELITY_FALLBACK: &str = "reporting_fidelity_fallback";

/// Termination label for a search stopped by its caller's cooperative
/// cancellation flag.
///
/// This is the one label that says the search reached no stopping criterion of
/// its own: not convergence, not a budget, not a mesh limit. Every kernel that
/// observes a cancellation flag reports exactly this string, so a caller can
/// recognise a cancelled run without parsing per-kernel vocabulary, and the
/// design such a run carries is never a delivered optimization result.
pub const CANCELLED: &str = "cancelled";

/// The one termination label that is itself a convergence certificate.
///
/// The product L-SHADE kernel reports convergence through
/// `SearchDiagnostics::converged` instead, which is the authority
/// [`OptimizationResult::converged`] prefers (population spread plus
/// best-feasible-cost stagnation; see `search_methods::lshade_de`).
const CONVERGED_TERMINATION: &str = "converged";

impl OptimizationResult {
    /// Whether the search stopped on its caller's cancellation flag.
    #[must_use]
    pub fn was_cancelled(&self) -> bool {
        self.termination == CANCELLED
    }

    /// Whether the search reported reaching its own convergence criterion.
    ///
    /// This is the *search*'s verdict on its own stopping condition. It is not
    /// a feasibility claim and not a manufacturability claim: see
    /// [`Self::is_delivered_feasible`] for the conjunction a caller must use
    /// before calling a design a delivered optimization result.
    #[must_use]
    pub fn converged(&self) -> bool {
        match self.search_diagnostics.as_ref() {
            Some(diagnostics) => diagnostics.converged,
            None => self.termination == CONVERGED_TERMINATION,
        }
    }

    /// Whether this result may be reported as a feasible delivered design.
    ///
    /// Every one of these must hold, and each is a distinct way a search can
    /// end without a usable aircraft:
    ///
    /// - the winner satisfied the active constraint set (`best_valid`);
    /// - the objective is finite, so no failure sentinel is being read as a
    ///   score;
    /// - the run was not cancelled, so a search stopped from outside cannot
    ///   deliver whichever candidate it happened to be holding;
    /// - the reporting-fidelity re-evaluation, when a caller performed one,
    ///   verified the delivered design.
    ///
    /// Convergence is deliberately *not* required: a budget-exhausted search
    /// can still deliver a verified feasible aircraft, and it is reported as
    /// feasible-but-not-converged rather than as either "feasible" alone or a
    /// failure. Report [`Self::converged`] alongside this, never instead of
    /// it.
    #[must_use]
    pub fn is_delivered_feasible(&self) -> bool {
        self.best_valid
            && self.best_cost.is_finite()
            && !self.was_cancelled()
            && self.termination != REPORTING_FIDELITY_REJECTED
            && self
                .delivered_acceptance
                .as_ref()
                .is_none_or(|acceptance| acceptance.verified)
    }
}

/// The application's verdict on the design a search actually delivers.
///
/// The search ranks candidates on the in-loop panel mesh and its own coupled
/// sizing closure. The published analysis re-solves the same aircraft on the
/// finer reported mesh and flies the route with the native mission and the
/// fuel policy, and the two models do not have to agree: on a supercritical
/// wing the chordwise convergence is first-order in panel count, so the
/// trimmed body attitude alone moves by about the coarse mesh's own error.
/// Before this record existed the search could report `converged` for a
/// design the application then reported INFEASIBLE, which is two different
/// statements printed as one.
///
/// A caller that performs the re-evaluation attaches this record through
/// [`OptimizationResult::record_delivered_acceptance`], which is what makes
/// the convergence verdict conditional on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveredAcceptance {
    /// Whether the delivered design passed the reporting-fidelity coupled
    /// re-evaluation with no error-severity finding.
    pub verified: bool,
    /// Identifiers of the findings that rejected the search's own finalist,
    /// in report order. Empty when the finalist was accepted.
    pub finalist_rejected_by: Vec<String>,
    /// Identifiers of the findings that rejected the delivered design, when
    /// no candidate could be verified at all.
    pub delivered_rejected_by: Vec<String>,
    /// The rejecting findings' own messages, for whichever design the two
    /// lists above describe.
    ///
    /// An identifier says *which* check refused the design; only the message
    /// says why. `fuel_policy_unavailable` is the case that made this
    /// necessary: it is raised when the analytic dispatch model could not be
    /// built or could not solve, and the reason it carries is the only place
    /// that says which of those happened. Without it a reader is told a
    /// finalist was rejected and given no way to act on it.
    #[serde(default)]
    pub rejection_messages: Vec<String>,
    /// How many candidates were re-evaluated at reporting fidelity, the
    /// search's own finalist included.
    pub candidates_evaluated: usize,
    /// Whether the delivered design is the finalist the search returned, as
    /// opposed to a lower-ranked candidate promoted after the finalist was
    /// rejected.
    pub delivered_is_search_finalist: bool,
    /// Wall-clock seconds spent on the re-evaluation, inside the same stage
    /// clock the search is timed with.
    pub wall_time_s: f64,
    /// Reporting-fidelity analyses run, ladder candidates and the baseline,
    /// failed ones included. They are charged to the refinement's
    /// verification reserve (`crate::verification_reserve`), together with
    /// the one final analysis the pipeline runs on the delivered design.
    #[serde(default)]
    pub analyses: usize,
    /// The unmodified baseline re-evaluated the same way, and the delivered
    /// design's trip fuel against it. Absent when it could not be evaluated.
    #[serde(default)]
    pub baseline: Option<ReportingBaseline>,
}

impl OptimizationResult {
    /// Attach the application's reporting-fidelity verdict and make the
    /// run's own convergence label agree with it.
    ///
    /// Convergence survives only when the design delivered is the finalist
    /// the search's mesh-local certificate belongs to *and* that finalist was
    /// accepted by the re-evaluation. A verified fallback candidate is a
    /// usable aircraft but carries no such certificate, so it is reported as
    /// [`REPORTING_FIDELITY_FALLBACK`] rather than as convergence; a design
    /// rejected outright becomes [`REPORTING_FIDELITY_REJECTED`]. Neither
    /// case weakens or hides the findings that produced it: they are carried
    /// in this record by identifier and reported in full by the feasibility
    /// stage.
    pub fn record_delivered_acceptance(&mut self, acceptance: DeliveredAcceptance) {
        if !acceptance.verified {
            // Consumers that inspect the scalar flag must observe the same
            // final verdict as consumers of the detailed acceptance record.
            self.best_valid = false;
            self.termination = REPORTING_FIDELITY_REJECTED.to_owned();
        } else if !acceptance.delivered_is_search_finalist {
            self.termination = REPORTING_FIDELITY_FALLBACK.to_owned();
        }
        let certified = acceptance.verified && acceptance.delivered_is_search_finalist;
        if let Some(diagnostics) = self.search_diagnostics.as_mut() {
            diagnostics.converged = diagnostics.converged && certified;
        }
        self.delivered_acceptance = Some(acceptance);
    }

    /// Hard-feasible candidates from this run's own history, best first, for
    /// a caller that must re-verify more than the finalist.
    ///
    /// Ranked exactly as the search ranks: a candidate with any violated hard
    /// residual is never offered, so a fallback can only ever be a design the
    /// search itself considered admissible. `best_design` is placed first
    /// whatever its history row says, because it is the design the search
    /// returned, and the winner's own history row is skipped. Duplicate
    /// vectors are removed on their exact bit pattern so a design is not
    /// verified twice.
    pub fn ranked_hard_feasible_candidates(&self, limit: usize) -> Vec<DesignVector> {
        let winner_row = self
            .search_diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.winner_history_row);
        let mut ranked: Vec<(usize, f64, DesignVector)> = self
            .history
            .design_vectors
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                Some(*index) != winner_row
                    && self.history.valid.get(*index).copied().unwrap_or(false)
                    && self
                        .history
                        .hard_violation
                        .get(*index)
                        .copied()
                        .is_some_and(|violation| violation <= 0.0)
                    && self
                        .history
                        .cost
                        .get(*index)
                        .copied()
                        .is_some_and(f64::is_finite)
            })
            .map(|(index, design)| (index, self.history.cost[index], *design))
            .collect();
        ranked.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));

        let key = |design: &DesignVector| {
            design
                .to_array()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<u64>>()
        };
        let mut seen = vec![key(&self.best_design)];
        let mut candidates = vec![self.best_design];
        for (_, _, design) in ranked {
            if candidates.len() >= limit.max(1) {
                break;
            }
            let candidate_key = key(&design);
            if seen.contains(&candidate_key) {
                continue;
            }
            seen.push(candidate_key);
            candidates.push(design);
        }
        candidates
    }
}

/// Failure from the public optimizer boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizationError {
    /// The selected method is not implemented by this build.
    #[error("invalid optimizer configuration: {0}")]
    InvalidConfiguration(String),
    /// The supplied design-space bounds cannot be searched safely.
    #[error("invalid optimizer bounds: {0}")]
    InvalidBounds(String),
    /// Every evaluated candidate failed the active objective/analysis policy.
    #[error("no feasible design: {0:?}")]
    NoFeasibleDesign(NoFeasibleDesign),
}

fn default_result_method() -> String {
    "differential_evolution".to_owned()
}

fn default_result_strategy() -> String {
    crate::search_methods::product_de::STRATEGY.to_owned()
}

fn default_result_termination() -> String {
    "unknown".to_owned()
}
