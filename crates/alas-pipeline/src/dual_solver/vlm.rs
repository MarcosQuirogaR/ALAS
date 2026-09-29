// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The native VLM optimization branch.

use super::*;

/// Run the native search, then make product-profile finalists survive the
/// application's reporting-fidelity re-evaluation before the branch is
/// reported as completed. The ALAS v1.1.0 `scipy_legacy` profile returns its
/// scalar-cost winner directly and does not apply the later product gate.
///
/// The search ranks candidates on the in-loop panel mesh and its analytic
/// dispatch closure; the published analysis re-solves the winner on the finer
/// reported mesh and flies the native mission at the fuel policy's load case.
/// Those are two models, and they can disagree by about the coarse mesh's own
/// error - enough to move a trimmed body attitude out of a two-degree design
/// window, or to leave a route the analytic closure sized for unflyable at
/// the mass it closed at. Such a run must not report `converged` while the
/// feasibility stage reports the same aircraft INFEASIBLE.
///
/// So the finalist is re-evaluated here, inside the optimization stage's own
/// clock. If it is accepted, nothing changes but the record. If it is
/// rejected, the loop offers the search's next-best *hard-feasible* candidate
/// - never a design the search itself rejected - and the first one the
///   application accepts is delivered, with the run reported as a
///   reporting-fidelity fallback rather than as convergence. If none is
///   accepted the search's own finalist is still returned, with its full
///   report and every finding intact, and the run is reported as
///   `reporting_fidelity_rejected`. No limit, residual or tolerance is
///   weakened anywhere in that ladder.
pub(super) struct VlmOptimizerRequest<'a> {
    pub(super) config: AlasConfig,
    pub(super) seed: Option<u64>,
    pub(super) nominal: DesignVector,
    pub(super) bounds: Option<&'a [(f64, f64)]>,
    pub(super) output_dir: Option<PathBuf>,
    pub(super) acceptance_route: Option<&'a AcceptanceRoute>,
    pub(super) cancel: Option<&'a AtomicBool>,
}

pub(super) fn run_vlm_optimizer(request: VlmOptimizerRequest<'_>) -> SolverOptimizationResult {
    let VlmOptimizerRequest {
        config,
        seed,
        nominal,
        bounds,
        output_dir,
        acceptance_route,
        cancel,
    } = request;
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

    // The SciPy-compatible profile selects a winner from its weighted scalar
    // objective and hands that design to the ordinary full-analysis/reporting
    // path. The mission-sized product finalist gate can reject valid winners
    // on requirements that profile does not enforce (for example the
    // structural mesh and physical CG-range checks), so do not replay it for
    // this profile.
    if effective_config.optimizer.solver.method == alas_config::optimizer::SCIPY_LEGACY_METHOD {
        let design = optimization.best_design;
        tracing::info!("SciPy legacy profile skips mission-sized finalist acceptance replay");
        let report = match FullAnalysis::new(config.clone()).run(&design, true) {
            Ok(report) => report,
            Err(error) => {
                return SolverOptimizationResult::failed(
                    SolverKind::Vlm,
                    output_dir,
                    format!("VLM best-design analysis failed: {error}"),
                )
            }
        };
        return SolverOptimizationResult {
            solver: SolverKind::Vlm,
            status: SolverOptimizationStatus::Completed,
            design: Some(design),
            optimization: Some(optimization),
            report: Some(report),
            avl_result: None,
            output_dir,
            error: None,
        };
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
        let verification = match scope.evaluation(|| {
            crate::acceptance::verify_finalist_cancellable(
                &config,
                candidate,
                acceptance_route,
                cancel,
            )
        }) {
            Ok(verification) => verification,
            Err(error) if error.starts_with("Cancelled safely") => {
                return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error);
            }
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
