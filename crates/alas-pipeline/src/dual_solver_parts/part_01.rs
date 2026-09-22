// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alas_aero::avl::{AvlPolar, AvlPolarPoint};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_opt::objective::DesignObjective;
use alas_opt::{
    assess_candidate_with_polar, DeliveredAcceptance, DesignOptimizer, ExternalPolar,
    ObjectiveEvaluation, ObjectiveEvaluator, OptimizationResult,
};

use crate::acceptance::{
    verify_finalist, AcceptanceRoute, FinalistVerification, MAX_VERIFIED_CANDIDATES,
};
use crate::avl::{run_avl_analysis, AvlAnalysisResult, AvlAnalysisStatus};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::solver_mode::{OptimizationSolverMode, SolverKind};

/// Reported when a branch's optimizer stopped on the pipeline's own
/// cancellation signal rather than a search failure.
///
/// The `"Cancelled safely"` prefix matches `pipeline::check_cancelled`'s own
/// wording, which is the exact prefix `alas-gui` checks
/// (`crates/alas-gui/src/run.rs`, `e.starts_with("Cancelled safely")`) to
/// report a run as cancelled instead of failed; this string keeps a
/// cancelled optimization branch on that same GUI path without any GUI
/// change.
const CANCELLED_DURING_OPTIMIZATION: &str = "Cancelled safely during design-space optimization";

/// Wall-clock deadline for one external AVL sweep inside the optimizer's own
/// evaluation loop, seconds.
///
/// Distinct from the 300 s deadline the *final* comparison run uses. Inside
/// the search this deadline is also the cancellation bound: `alas-exec` polls
/// the child every 25 ms and force-kills the process tree when the deadline
/// passes, but it has no cancellation flag, so a sweep already in flight when
/// the flag is set runs to its own deadline. Sixty seconds is far above any
/// converging AVL sweep of a transport deck at this mesh and far below the
/// comparison deadline, which bounds the drain without changing what a
/// healthy sweep produces. A sweep that needs longer than this was not going
/// to produce a usable polar for a search candidate.
const AVL_EVALUATION_TIMEOUT_S: f64 = 60.0;

/// Written to a cancelled branch's own directory so the analyses the search
/// already paid for survive the cancellation.
const CANCELLED_SEARCH_RECORD: &str = "cancelled_search.json";

/// Lifecycle state of one requested optimization branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverOptimizationStatus {
    /// This backend was not selected for the run.
    NotRequested,
    /// The backend produced a design and a full report.
    Completed,
    /// The selected backend could not produce a usable result.
    Failed,
}

impl SolverOptimizationStatus {
    /// Stable spelling for run manifests and UI diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// One independently optimized aircraft solution.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationResult {
    /// Concrete backend that produced this solution.
    pub solver: SolverKind,
    /// Explicit lifecycle state.
    pub status: SolverOptimizationStatus,
    /// Best design vector, when the branch completed.
    pub design: Option<DesignVector>,
    /// Optimizer trajectory, when the branch completed.
    pub optimization: Option<OptimizationResult>,
    /// Full report for this branch's best design.
    pub report: Option<AnalysisReport>,
    /// Retained AVL run for the AVL branch's best design.
    pub avl_result: Option<AvlAnalysisResult>,
    /// Branch-local output directory, when one is configured.
    pub output_dir: Option<PathBuf>,
    /// Actionable failure detail, present only for a failed branch.
    pub error: Option<String>,
}

impl SolverOptimizationResult {
    fn not_requested(solver: SolverKind) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::NotRequested,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir: None,
            error: None,
        }
    }

    fn failed(solver: SolverKind, output_dir: Option<PathBuf>, error: impl Into<String>) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::Failed,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir,
            error: Some(error.into()),
        }
    }
}

/// Run-scoped pair of independently optimized aircraft solutions.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationSet {
    /// Native VLM optimization branch.
    pub vlm: SolverOptimizationResult,
    /// External AVL optimization branch.
    pub avl: SolverOptimizationResult,
}

impl SolverOptimizationSet {
    /// Select the requested primary solution, without silently falling back
    /// from an explicitly AVL-only request.
    pub fn selected(
        &self,
        mode: OptimizationSolverMode,
    ) -> Result<&SolverOptimizationResult, String> {
        match mode {
            OptimizationSolverMode::Vlm => self.completed_or_error(&self.vlm),
            OptimizationSolverMode::Avl => self.completed_or_error(&self.avl),
            OptimizationSolverMode::Both => {
                if self.vlm.status == SolverOptimizationStatus::Completed {
                    Ok(&self.vlm)
                } else {
                    self.completed_or_error(&self.avl)
                }
            }
        }
    }

    fn completed_or_error<'a>(
        &self,
        result: &'a SolverOptimizationResult,
    ) -> Result<&'a SolverOptimizationResult, String> {
        if result.status == SolverOptimizationStatus::Completed {
            Ok(result)
        } else {
            Err(result.error.clone().unwrap_or_else(|| {
                format!(
                    "{} optimization did not produce a usable result",
                    result.solver.as_str()
                )
            }))
        }
    }
}

/// Run the requested optimizer branches, optionally in parallel.
// Each argument is an independent runtime control or typed input boundary;
// keeping them explicit makes the pipeline call site auditable and avoids a
// configuration object that could silently mix the two solver namespaces.
#[allow(clippy::too_many_arguments)]
pub fn run_solver_optimizations(
    config: &AlasConfig,
    mode: OptimizationSolverMode,
    parallel: bool,
    seed: Option<u64>,
    environment: &RunEnvironment,
    nominal_design: &DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<&Path>,
    acceptance_route: Option<&AcceptanceRoute>,
    cancel: Option<&AtomicBool>,
) -> SolverOptimizationSet {
    let want_vlm = matches!(
        mode,
        OptimizationSolverMode::Vlm | OptimizationSolverMode::Both
    );
    let want_avl = matches!(
        mode,
        OptimizationSolverMode::Avl | OptimizationSolverMode::Both
    );
    let bounds = bounds.map(<[(f64, f64)]>::to_vec);
    let nominal = *nominal_design;
    let vlm_config = serial_solver_config(config, parallel);
    let avl_config = serial_solver_config(config, parallel);
    let vlm_environment = environment.clone();
    let avl_environment = environment.clone();
    let vlm_output = output_dir.map(|path| path.join("solvers/vlm"));
    let avl_output = output_dir.map(|path| path.join("solvers/avl"));

    let run_vlm = || {
        if want_vlm {
            run_vlm_optimizer(
                vlm_config,
                seed,
                vlm_environment,
                nominal,
                bounds.as_deref(),
                vlm_output,
                acceptance_route,
                cancel,
            )
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Vlm)
        }
    };
    let run_avl = || {
        if want_avl {
            run_avl_optimizer(
                avl_config,
                seed,
                avl_environment,
                nominal,
                bounds.as_deref(),
                avl_output,
                cancel,
            )
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Avl)
        }
    };

    if parallel && want_vlm && want_avl {
        std::thread::scope(|scope| {
            let vlm = scope.spawn(run_vlm);
            let avl = scope.spawn(run_avl);
            SolverOptimizationSet {
                vlm: vlm.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Vlm,
                        None,
                        "VLM optimizer worker panicked",
                    )
                }),
                avl: avl.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Avl,
                        None,
                        "AVL optimizer worker panicked",
                    )
                }),
            }
        })
    } else {
        SolverOptimizationSet {
            vlm: run_vlm(),
            avl: run_avl(),
        }
    }
}

/// Run the native search, then make the design it delivers survive the
/// application's own reporting-fidelity re-evaluation before the branch is
/// reported as completed.
///
/// The search ranks candidates on the in-loop panel mesh and its analytic
/// dispatch closure; the published analysis re-solves the winner on the finer
/// reported mesh and flies the native mission at the fuel policy's load case.
/// Those are two models, and they can disagree by about the coarse mesh's own
/// error - enough to move a trimmed body attitude out of a two-degree design
/// window, or to leave a route the analytic closure sized for unflyable at
/// the mass it closed at. The optimizer used to report `converged` in exactly
/// those runs and the feasibility stage used to report the same aircraft
/// INFEASIBLE.
///
/// So the finalist is re-evaluated here, inside the optimization stage's own
/// clock. If it is accepted, nothing changes but the record. If it is
/// rejected, the loop offers the search's next-best *hard-feasible* candidate
/// - never a design the search itself rejected - and the first one the
/// application accepts is delivered, with the run reported as a
/// reporting-fidelity fallback rather than as convergence. If none is
/// accepted the search's own finalist is still returned, with its full
/// report and every finding intact, and the run is reported as
/// `reporting_fidelity_rejected`. No limit, residual or tolerance is
/// weakened anywhere in that ladder.
fn run_vlm_optimizer(
    config: AlasConfig,
    seed: Option<u64>,
    _environment: RunEnvironment,
    nominal: DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<PathBuf>,
    acceptance_route: Option<&AcceptanceRoute>,
    cancel: Option<&AtomicBool>,
) -> SolverOptimizationResult {
    let output_dir = create_branch_directory(output_dir);
    let effective_config = match seeded_config(&config, seed) {
        Ok(config) => config,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error),
    };
    let mut optimizer = DesignOptimizer::new(effective_config.clone());
    let mut optimization = match optimizer.run_cancellable(bounds, Some(&nominal), None, cancel) {
        Ok(result) => result,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Vlm,
                output_dir,
                format!("VLM optimization failed: {error}"),
            )
        }
    };
    if optimization.was_cancelled() {
        // Stop here, before the reporting-fidelity re-verification ladder
        // below spends more analyses on a candidate the search itself did
        // not finish choosing between. What the search *did* produce is
        // written out first: it is real analysis effort, and discarding it
        // would make every cancelled run indistinguishable from one that
        // never started.
        write_cancelled_search_record(output_dir.as_deref(), &optimization, cancel);
        return SolverOptimizationResult::failed(
            SolverKind::Vlm,
            output_dir,
            CANCELLED_DURING_OPTIMIZATION,
        );
    }

    // The ladder below marks its own phase per candidate; entering it here
    // as well would put two identical records at the same timestamp.
    let scope = alas_opt::CancelScope::attach(cancel);
    let started = Instant::now();
    let candidates = optimization.ranked_hard_feasible_candidates(MAX_VERIFIED_CANDIDATES);
    let mut evaluated = 0usize;
    let mut finalist: Option<FinalistVerification> = None;
    let mut finalist_rejected_by = Vec::new();
    let mut rejection_messages: Vec<String> = Vec::new();
    let mut accepted: Option<(usize, FinalistVerification)> = None;
    for (rank, candidate) in candidates.iter().enumerate() {
        // Rank 0 is always re-evaluated: without it the branch has no
        // delivered design at all. Beyond it the ladder is optional quality
        // improvement, so a cancellation stops it at the next candidate
        // instead of paying up to `MAX_VERIFIED_CANDIDATES` coupled analyses
        // of drain.
        if rank > 0 && scope.requested() {
            scope.work_skipped(format!(
                "reporting-fidelity ladder stopped after {rank} of {} candidates",
                candidates.len()
            ));
            break;
        }
        scope.enter(
            alas_opt::CancelPhase::ReportingFidelityVerification,
            rank as u64,
        );
        let verification =
            match scope.evaluation(|| verify_finalist(&config, candidate, acceptance_route)) {
                Ok(verification) => verification,
                Err(error) if rank == 0 => {
                    // The search returned a design its own replay rejects. That
                    // is an integration error in the search, not a reporting
                    // disagreement, and it stays a branch failure.
                    return SolverOptimizationResult::failed(
                        SolverKind::Vlm,
                        output_dir,
                        format!("VLM finalist replay failed: {error}"),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, rank, "fallback candidate could not be re-evaluated");
                    continue;
                }
            };
        evaluated += 1;
        if rank == 0 {
            finalist_rejected_by = verification.rejected_by();
            rejection_messages = verification.rejection_messages();
        }
        if verification.accepted() {
            accepted = Some((rank, verification));
            break;
        }
        if rank == 0 {
            finalist = Some(verification);
        }
    }

    let (delivered, delivered_is_search_finalist) = match (accepted, finalist) {
        (Some((rank, verification)), _) => (verification, rank == 0),
        (None, Some(verification)) => (verification, true),
        (None, None) => {
            return SolverOptimizationResult::failed(
                SolverKind::Vlm,
                output_dir,
                "VLM finalist could not be re-evaluated at reporting fidelity",
            )
        }
    };
    let verified = delivered.accepted();
    let delivered_rejected_by = delivered.rejected_by();
    if !verified {
        // Nothing was accepted, so the messages a reader needs are the
        // delivered design's own rather than the finalist's.
        rejection_messages = delivered.rejection_messages();
    }
    if !verified {
        tracing::warn!(
            rejected_by = %delivered_rejected_by.join(", "),
            candidates_evaluated = evaluated,
            "no candidate survived the reporting-fidelity re-evaluation"
        );
    }
    optimization.record_delivered_acceptance(DeliveredAcceptance {
        verified,
        finalist_rejected_by,
        delivered_rejected_by,
        rejection_messages,
        candidates_evaluated: evaluated,
        delivered_is_search_finalist,
        wall_time_s: started.elapsed().as_secs_f64(),
    });

    SolverOptimizationResult {
        solver: SolverKind::Vlm,
        status: SolverOptimizationStatus::Completed,
        design: Some(delivered.design),
        optimization: Some(optimization),
        report: Some(delivered.report),
        avl_result: None,
        output_dir,
        error: None,
    }
}

fn run_avl_optimizer(
    config: AlasConfig,
    seed: Option<u64>,
    environment: RunEnvironment,
    nominal: DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<PathBuf>,
    cancel: Option<&AtomicBool>,
) -> SolverOptimizationResult {
    let Some(executable) = environment.avl_exe.clone() else {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            "AVL optimization requested but no native AVL executable is configured",
        );
    };
    let Some(output_root) = output_dir.clone() else {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            None,
            "AVL optimization requires an output directory to retain solver evidence",
        );
    };
    let _ = std::fs::create_dir_all(&output_root);
    let effective_config = match seeded_config(&config, seed) {
        Ok(config) => config,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Avl, output_dir, error),
    };
    let mut objective = AvlObjective::new(
        config.clone(),
        executable.clone(),
        output_root.join("evaluations"),
        cancel,
    );
    let mut optimizer = DesignOptimizer::new(effective_config);
    let optimization = match optimizer.run_with_evaluator_cancellable(
        bounds,
        Some(&nominal),
        &mut objective,
        None,
        cancel,
    ) {
        Ok(result) => result,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Avl,
                output_dir,
                format!("AVL optimization failed: {error}"),
            )
        }
    };
    if optimization.was_cancelled() {
        write_cancelled_search_record(output_dir.as_deref(), &optimization, cancel);
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            CANCELLED_DURING_OPTIMIZATION,
        );
    }
    let design = optimization.best_design;
    let report = match FullAnalysis::new(config.clone()).run(&design, true) {
        Ok(report) => report,
        Err(error) => {
            return SolverOptimizationResult::failed(
                SolverKind::Avl,
                output_dir,
                format!("AVL best-design analysis failed: {error}"),
            )
        }
    };
    let final_dir = output_root.join("final");
    let avl_result = run_avl_analysis(&report, &config, &final_dir, Some(&executable), 300.0);
    if avl_result.status != AvlAnalysisStatus::CompletedComparable {
        return SolverOptimizationResult::failed(
            SolverKind::Avl,
            output_dir,
            format!(
                "AVL best-design output is not comparable: {}",
                avl_result
                    .error
                    .as_deref()
                    .unwrap_or(avl_result.status.as_str())
            ),
        );
    }
    SolverOptimizationResult {
        solver: SolverKind::Avl,
        status: SolverOptimizationStatus::Completed,
        design: Some(design),
        optimization: Some(optimization),
        report: Some(report),
        avl_result: Some(avl_result),
        output_dir,
        error: None,
    }
}

/// The solver configuration a run with `parallel` uses.
///
/// `parallel` is the run-level "use this machine" switch (`--no-parallel`
/// clears it), and it used to reach only the choice to run the VLM and AVL
/// branches side by side: candidate evaluation inside each branch read
/// `optimizer.solver.workers` and never saw the flag, so a serial request
/// still started a batch on every worker the setting allowed. Clamping the
/// count here is what makes the serial request actually serial.
///
/// It is a scheduling change only - the batch, its designs and their scores
/// are identical at any worker count - so a serial run returns the same
/// aircraft, more slowly.
///
/// What this does *not* claim is a single-threaded process. One coupled
/// evaluation still fills its vortex-lattice influence matrix and factorises
/// it across the shared rayon pool (`alas_aero::vlm::system`,
/// `alas_math::linalg`). That is data parallelism inside one arithmetic
/// operation, with a result documented and tested as bit-identical to the
/// serial loop, not concurrent evaluation of independent work; no candidate,
/// branch or pipeline stage overlaps another under `--no-parallel`.
fn serial_solver_config(config: &AlasConfig, parallel: bool) -> AlasConfig {
    let mut config = config.clone();
    if !parallel {
        config.optimizer.solver.workers = 1;
    }
    config
}

/// Persist what a cancelled search produced, in its own branch directory.
///
/// # Why a cancelled run writes anything at all
///
/// A cancelled branch delivers no design - it is not feasible, not converged,
/// and the pipeline fails the run - but it did execute real coupled analyses,
/// and their count, timing and the phase the stop landed in are the evidence
/// a later run is planned from. Throwing them away is what made every
/// cancelled run look identical from the outside.
///
/// # What this file is not
///
/// Every verdict field in it is `false` by construction, and it carries no
/// design vector: nothing downstream reads it, and nothing in it can be
/// mistaken for a result. It is a record of effort spent, labelled as such.
fn write_cancelled_search_record(
    output_dir: Option<&Path>,
    optimization: &OptimizationResult,
    cancel: Option<&AtomicBool>,
) {
    let Some(directory) = output_dir else {
        return;
    };
    let diagnostics = optimization.search_diagnostics.as_ref();
    let telemetry = alas_opt::CancelScope::attach(cancel)
        .snapshot()
        .and_then(|snapshot| serde_json::to_value(snapshot).ok());
    let record = serde_json::json!({
        "record_kind": "cancelled_search",
        "explanation": "The search was stopped by its supervisor before it reached any \
                        stopping criterion of its own. This file records the analyses the run \
                        had already paid for. It is not a result: no design is delivered, and \
                        every verdict below is false.",
        "method": optimization.method,
        "strategy": optimization.strategy,
        "termination": optimization.termination,
        "stop_reason": alas_opt::StopReason::from_termination(&optimization.termination),
        "converged": false,
        "delivered_feasible": false,
        "best_valid": false,
        "wall_time_s": optimization.wall_time_s,
        "analyses": {
            "search_evaluations": diagnostics.map(|d| d.analysis_evaluations),
            "screening_evaluations": diagnostics.map(|d| d.screening_evaluations),
            "screening_feasible": diagnostics.map(|d| d.screening_feasible),
            "verification_evaluations": diagnostics.map(|d| d.verification_evaluations),
            "cache_hits": diagnostics.map(|d| d.cache_hits),
            "poll_iterations": diagnostics.map(|d| d.poll_iterations),
            "history_evaluations": optimization.history.n_evaluations(),
        },
        "timing_s": {
            "scan_wall_time": diagnostics.map(|d| d.scan_wall_time_s),
            "search_wall_time": diagnostics.map(|d| d.search_wall_time_s),
        },
        "cancellation_telemetry": telemetry,
    });
    let path = directory.join(CANCELLED_SEARCH_RECORD);
    match serde_json::to_vec_pretty(&record) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&path, bytes) {
                tracing::warn!(%error, path = %path.display(), "cannot persist the cancelled-search record");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "cannot serialize the cancelled-search record");
        }
    }
}

fn create_branch_directory(output_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = &output_dir {
        let _ = std::fs::create_dir_all(path);
    }
    output_dir
}

fn seeded_config(config: &AlasConfig, seed: Option<u64>) -> Result<AlasConfig, String> {
    let mut effective = config.clone();
    if let Some(seed) = seed {
        effective.optimizer.solver.seed = Some(
            i64::try_from(seed)
                .map_err(|_| "optimizer seed exceeds the supported integer range")?,
        );
    }
    Ok(effective)
}

/// The AVL-backed objective: one external solver process per uncached
/// candidate.
///
/// This is the only optimizer evaluation path in the product that spawns a
/// process, so it is the only one whose cancellation bound is set by
/// something other than an internal analysis. `alas-exec` owns the child - it
/// polls it every 25 ms and force-kills the whole process tree on its
/// deadline - and exposes no cancellation flag, so this objective bounds the
/// drain the two ways available to a caller: it refuses to *start* a sweep
/// once cancellation has been requested, and it runs each sweep under
/// [`AVL_EVALUATION_TIMEOUT_S`] rather than the comparison deadline.
struct AvlObjective<'a> {
    config: AlasConfig,
    objective: DesignObjective,
    executable: PathBuf,
    output_root: PathBuf,
    cache: BTreeMap<String, ObjectiveEvaluation>,
    scope: alas_opt::CancelScope<'a>,
}

impl<'a> AvlObjective<'a> {
    fn new(
        config: AlasConfig,
        executable: PathBuf,
        output_root: PathBuf,
        cancel: Option<&'a AtomicBool>,
    ) -> Self {
        Self {
            objective: DesignObjective::new(config.clone()),
            config,
            executable,
            output_root,
            cache: BTreeMap::new(),
            scope: alas_opt::CancelScope::attach(cancel),
        }
    }

    fn cache_key(design: &DesignVector) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for value in design.to_array() {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3_u64);
            }
        }
        format!("{hash:016x}")
    }
}

impl ObjectiveEvaluator for AvlObjective<'_> {
    fn evaluate(&mut self, design: &DesignVector) -> ObjectiveEvaluation {
        let key = Self::cache_key(design);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let evaluation = self.evaluate_uncached(design, &key);
        self.cache.insert(key, evaluation.clone());
        evaluation
    }
}
