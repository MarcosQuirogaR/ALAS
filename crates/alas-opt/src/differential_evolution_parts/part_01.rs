// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use serde::{Deserialize, Serialize};

use crate::evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
use crate::history::OptimizationHistory;
use crate::objective::{apply_candidate_payload_load_case, DesignObjective};
use crate::python_rng::{Pcg64, RandomState};
use crate::search_methods::{MethodOutcome, ScoredPoint};

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
    /// In the unconstrained product-search mode this means that geometry,
    /// mass and aerodynamic evaluation completed with a finite objective;
    /// physical requirement violations are retained as downstream report
    /// diagnostics rather than making the candidate ineligible.
    #[serde(default)]
    pub best_valid: bool,
    /// Full evaluation history collected during the run.
    pub history: OptimizationHistory,
    /// Elapsed wall-clock time in seconds.
    pub wall_time_s: f64,
    /// Stable identifier of the search method that produced this result.
    #[serde(default = "default_result_method")]
    pub method: String,
    /// Mutation/crossover strategy used by the search.
    #[serde(default = "default_result_strategy")]
    pub strategy: String,
    /// Durable search lifecycle reason.  For product MADS this is one of
    /// `evaluation_budget`, `mesh_limit`, `iteration_limit`, `fixed_bounds`
    /// or `invalid_input`; it is kept separate from the legacy strategy
    /// label so a budget stop is not mistaken for convergence.
    #[serde(default = "default_result_termination")]
    pub termination: String,
    /// Final nondominated set for a multi-objective method.
    #[serde(default)]
    pub pareto_front: Vec<ParetoCandidate>,
    /// Measured search lifecycle, for a caller that needs to distinguish a
    /// converged run from one that stopped on a budget or a safety limit.
    /// Absent for the frozen differential-evolution replay.
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
/// with, and the wall-clock split between the staged scan and the search, all
/// measured from the stage's own analysis-start instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchDiagnostics {
    /// Whether the search reached its convergence criterion. A budget,
    /// iteration, mesh-floor or watchdog stop is `false`.
    pub converged: bool,
    /// Coupled full-fidelity analyses executed by the search stage.
    pub analysis_evaluations: usize,
    /// Repeated mesh nodes served from the search's cache, so never analysed.
    pub cache_hits: usize,
    /// Poll iterations completed.
    pub poll_iterations: usize,
    /// Reduced-model analyses executed by the broad scan. These are ranked on
    /// a coarser mesh and a looser sizing closure and are not comparable with
    /// the full-fidelity evaluations above.
    pub screening_evaluations: usize,
    /// How many screened candidates were feasible under the reduced model.
    pub screening_feasible: usize,
    /// Full-fidelity analyses spent verifying the scan finalists.
    pub verification_evaluations: usize,
    /// Wall-clock seconds in the broad scan.
    pub scan_wall_time_s: f64,
    /// Wall-clock seconds in the MADS stage.
    pub search_wall_time_s: f64,
    /// Worker threads used inside one evaluation block.
    pub workers: usize,
    /// Points evaluated per opportunistic poll block. Fixed independently of
    /// `workers` so results do not change with the hardware.
    pub poll_block_size: usize,
    /// Objective of the first feasible point the search reached.
    pub first_feasible_cost: Option<f64>,
    /// Relative improvement of the winner over that first feasible point,
    /// dimensionless.
    pub relative_improvement: Option<f64>,
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
/// Every current product kernel reports convergence through
/// `SearchDiagnostics::converged` instead, which is the authority
/// [`OptimizationResult::converged`] prefers: `mads` maps its own
/// `TerminationReason`, `sqp` carries the driver's stationarity verdict, and
/// the product DE kernel implements no convergence test at all and is never
/// converged. This string covers the legacy reference-compatibility DE driver,
/// which predates the diagnostics record and reports only a label.
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
    /// returned. Duplicate vectors are removed on their exact bit pattern so
    /// a re-evaluated mesh node is not verified twice.
    pub fn ranked_hard_feasible_candidates(&self, limit: usize) -> Vec<DesignVector> {
        let mut ranked: Vec<(usize, f64, DesignVector)> = self
            .history
            .design_vectors
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.history
                    .valid
                    .get(*index)
                    .copied()
                    .unwrap_or(false)
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

/// Evidence returned when a search evaluated candidates but none passed the
/// active objective/analysis validity policy.
///
/// Rejected candidates are deliberately not promoted to
/// [`OptimizationResult`].  A caller that wants to inspect the failed search
/// can use the counts below without accidentally treating a review artifact as
/// an aircraft design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoFeasibleDesign {
    /// Number of candidates evaluated before the search ended.
    pub evaluated_candidates: usize,
    /// Counts of the machine-readable rejection categories observed.
    pub rejection_reason_counts: BTreeMap<String, usize>,
}

impl NoFeasibleDesign {
    fn from_history(history: &OptimizationHistory) -> Self {
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

/// Failure from the public optimizer boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizationError {
    /// The selected method or strategy is not implemented by this build.
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
    "best1bin".to_owned()
}

fn default_result_termination() -> String {
    "unknown".to_owned()
}

/// Searches the aircraft design space to minimize the [`DesignObjective`].
#[derive(Debug, Clone)]
pub struct DesignOptimizer {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    reference_mass_coordinates: bool,
}

#[path = "../differential_evolution_optimizer.rs"]
mod differential_evolution_optimizer;
fn runtime_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn validate_bounds(bounds: &[(f64, f64)]) -> Result<(), OptimizationError> {
    if bounds.len() != alas_config::DESIGN_VARIABLE_SPECS.len() {
        return Err(OptimizationError::InvalidBounds(format!(
            "expected {} design-variable bounds, got {}",
            alas_config::DESIGN_VARIABLE_SPECS.len(),
            bounds.len()
        )));
    }
    for (index, &(lower, upper)) in bounds.iter().enumerate() {
        if !lower.is_finite() || !upper.is_finite() {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} must contain finite values, got [{lower:?}, {upper:?}]"
            )));
        }
        if lower > upper {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} has lower {lower} greater than upper {upper}"
            )));
        }
        if !(upper - lower).is_finite() {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} has a non-finite width [{lower}, {upper}]"
            )));
        }
    }
    Ok(())
}

fn ensure_feasible(result: OptimizationResult) -> Result<OptimizationResult, OptimizationError> {
    if result.best_valid {
        Ok(result)
    } else {
        Err(OptimizationError::NoFeasibleDesign(
            NoFeasibleDesign::from_history(&result.history),
        ))
    }
}

/// The search's view of one recorded evaluation, read at its own history row.
///
/// A batched evaluation appends one row per candidate in candidate order, so a
/// block's scores must be read at their own indices: reading the last row
/// would give every point in the block the score of whichever candidate
/// happened to be appended last.
fn scored_point_at(
    values: &[f64],
    cost: f64,
    history: &OptimizationHistory,
    index: usize,
) -> ScoredPoint {
    let valid = history.valid.get(index).copied().unwrap_or(false) && cost.is_finite();
    let l_over_d = history.l_over_d.get(index).copied().unwrap_or(0.0);
    // The mission objective when the native path recorded one; the scalar
    // cost is the only objective a delegated evaluator reports.
    let objective_value = history
        .objective_value
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(cost);
    let span_m = history.span_m.get(index).copied().unwrap_or(f64::INFINITY);
    let area_m2 = history.area_m2.get(index).copied().unwrap_or(f64::INFINITY);
    let reason = history
        .reject_reason
        .get(index)
        .map(String::as_str)
        .unwrap_or("evaluation_failure");
    let hard_violation = history
        .hard_violation
        .get(index)
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(f64::NAN);
    let constraint_violation = if valid {
        // A strictly feasible candidate has no hard violation at all, so this
        // is zero and the ranking key reduces to the objective, exactly as
        // before. A candidate admitted only by the controlled-relaxation
        // policy still carries the violations that were relaxed, and the key
        // is lexicographic in (admissible, violation, cost), so every fully
        // feasible design ranks ahead of every relaxed one whatever their
        // objectives - which is clarified ledger D03, obtained without a
        // tuned penalty.
        if hard_violation.is_finite() && hard_violation > 0.0 {
            hard_violation
        } else {
            0.0
        }
    } else if hard_violation.is_finite() && hard_violation > 0.0 {
        // A physical miss is ordered by its dimensionless aggregate
        // violation. Counting reject labels made a severe single miss appear
        // better than several small misses and discarded the actual physics.
        hard_violation
    } else {
        // Analysis failures are an extreme barrier and remain behind every
        // completed physical miss, even when the delegated evaluator did not
        // provide a residual table.
        let category_count = reason
            .split('+')
            .filter(|part| !part.is_empty())
            .count()
            .max(1) as f64;
        if !l_over_d.is_finite() || l_over_d <= 0.0 {
            1_000_000.0 + category_count
        } else {
            category_count
        }
    };
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid,
        constraint_violation,
        objectives: [objective_value, span_m, area_m2],
    }
}

fn result_from_method(
    outcome: MethodOutcome,
    method: &str,
    strategy: &str,
    termination: &str,
    history: &OptimizationHistory,
    wall_time_s: f64,
) -> OptimizationResult {
    let winner = outcome.winner;
    let best_design = DesignVector::from_array(&winner.values).unwrap_or_default();
    let pareto_front = outcome
        .pareto_front
        .into_iter()
        .filter_map(|point| {
            let design = DesignVector::from_array(&point.values).ok()?;
            Some(ParetoCandidate {
                design,
                cost: point.cost,
                objective_value: point.objectives[0],
                span_m: point.objectives[1],
                area_m2: point.objectives[2],
                valid: point.valid,
            })
        })
        .collect();
    OptimizationResult {
        best_design,
        best_cost: winner.cost,
        best_valid: winner.valid,
        history: history.clone(),
        wall_time_s,
        method: method.to_owned(),
        strategy: strategy.to_owned(),
        termination: termination.to_owned(),
        pareto_front,
        // Filled in by the product search, which is the only caller that
        // measures its own lifecycle.
        search_diagnostics: None,
        // Filled in by the application that re-evaluates the finalist at
        // reporting fidelity; the search cannot answer this about itself.
        delivered_acceptance: None,
    }
}

trait SearchObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64;
    fn history(&self) -> &OptimizationHistory;

    /// Evaluate a candidate batch and return `(cost, valid)` in input order.
    ///
    /// Backends with mutable external state use the serial default. The native
    /// objective overrides this to parallelize its CPU-bound, cloned analyses.
    fn evaluate_batch(&mut self, designs: &[Vec<f64>], _workers: usize) -> Vec<(f64, bool)> {
        designs
            .iter()
            .map(|design| {
                let cost = self.evaluate(design);
                let valid =
                    self.history().valid.last().copied().unwrap_or(false) && cost.is_finite();
                (cost, valid)
            })
            .collect()
    }
}

impl SearchObjective for DesignObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        DesignObjective::evaluate(self, design)
    }

    fn history(&self) -> &OptimizationHistory {
        &self.history
    }

    fn evaluate_batch(&mut self, designs: &[Vec<f64>], workers: usize) -> Vec<(f64, bool)> {
        if workers <= 1 || designs.len() <= 1 {
            return designs
                .iter()
                .map(|design| {
                    let cost = DesignObjective::evaluate(self, design);
                    let valid =
                        self.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
                    (cost, valid)
                })
                .collect();
        }

        let worker_count = workers.min(designs.len());
        let chunk_size = designs.len().div_ceil(worker_count);
        let mut baseline = self.clone();
        // Only the new batch belongs in each worker's returned trace. The
        // caller's prior history is merged once after all joins succeed.
        baseline.history = OptimizationHistory::new();

        let mut handles = Vec::with_capacity(worker_count);
        for chunk in designs.chunks(chunk_size) {
            let mut local = baseline.clone();
            let candidates = chunk.to_vec();
            handles.push(std::thread::spawn(move || {
                let mut evaluations = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    let cost = DesignObjective::evaluate(&mut local, &candidate);
                    let valid =
                        local.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
                    evaluations.push((cost, valid));
                }
                (evaluations, local.history)
            }));
        }

        let mut merged = Vec::with_capacity(designs.len());
        let mut histories = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.join() {
                Ok((evaluations, history)) => {
                    merged.extend(evaluations);
                    histories.push(history);
                }
                Err(_) => {
                    // A worker panic is not allowed to lose the optimizer's
                    // history or leave it partially merged. Re-run this batch
                    // serially in the caller thread, where any ordinary
                    // objective rejection remains represented as data.
                    return designs
                        .iter()
                        .map(|design| {
                            let cost = DesignObjective::evaluate(self, design);
                            let valid = self.history.valid.last().copied().unwrap_or(false)
                                && cost.is_finite();
                            (cost, valid)
                        })
                        .collect();
                }
            }
        }
        for history in histories {
            self.history.append(history);
        }
        merged
    }
}

struct DelegatedObjective<'a, E: ObjectiveEvaluator + ?Sized> {
    evaluator: &'a mut E,
    history: OptimizationHistory,
    failure_cost: f64,
}

impl<'a, E: ObjectiveEvaluator + ?Sized> DelegatedObjective<'a, E> {
    fn new(evaluator: &'a mut E, failure_cost: f64) -> Self {
        Self {
            evaluator,
            history: OptimizationHistory::new(),
            failure_cost,
        }
    }
}

impl<E: ObjectiveEvaluator + ?Sized> SearchObjective for DelegatedObjective<'_, E> {
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        let typed = match DesignVector::from_array(design) {
            Ok(value) => value,
            Err(_) => {
                let evaluation = ObjectiveEvaluation::rejected(self.failure_cost, "geometry_build");
                self.history.record(
                    DesignVector::default(),
                    evaluation.valid,
                    evaluation.cost,
                    evaluation.l_over_d,
                    evaluation.span_m,
                    evaluation.alpha_deg,
                    evaluation.area_m2,
                    evaluation.trim_ih_deg,
                    evaluation.reject_reason,
                );
                return evaluation.cost;
            }
        };
        let evaluation = self.evaluator.evaluate(&typed);
        self.history.record(
            typed,
            evaluation.valid,
            evaluation.cost,
            evaluation.l_over_d,
            evaluation.span_m,
            evaluation.alpha_deg,
            evaluation.area_m2,
            evaluation.trim_ih_deg,
            evaluation.reject_reason,
        );
        evaluation.cost
    }

    fn history(&self) -> &OptimizationHistory {
        &self.history
    }
}

fn candidate_is_better(
    candidate_cost: f64,
    candidate_valid: bool,
    incumbent_cost: f64,
    incumbent_valid: bool,
    feasibility_first: bool,
) -> bool {
    if feasibility_first && candidate_valid != incumbent_valid {
        return candidate_valid;
    }
    candidate_cost.total_cmp(&incumbent_cost).is_lt()
}

fn candidate_is_at_least_as_good(
    candidate_cost: f64,
    candidate_valid: bool,
    incumbent_cost: f64,
    incumbent_valid: bool,
    feasibility_first: bool,
) -> bool {
    if feasibility_first && candidate_valid != incumbent_valid {
        return candidate_valid;
    }
    candidate_cost <= incumbent_cost
}

struct TrialState<'a> {
    population: &'a mut [Vec<f64>],
    costs: &'a mut [f64],
    validities: &'a mut [bool],
    feasibility_first: bool,
}
