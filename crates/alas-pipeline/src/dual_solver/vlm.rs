// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The native VLM optimization branch.

use super::*;

/// Run the native search, then make product-profile finalists survive the
/// application's reporting-fidelity re-evaluation before the branch is
/// reported as completed.
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

    // The ladder below marks its own phase per candidate; entering it here
    // as well would put two identical records at the same timestamp.
    let scope = alas_opt::CancelScope::attach(cancel);
    let started = Instant::now();
    // The ladder runs inside the refinement's verification reserve: its
    // evaluations less the baseline and the final analysis, and the stage
    // time the search left. The finalist always runs.
    let reserve = alas_opt::verification_reserve(&effective_config.optimizer.solver);
    // The registered aircraft at reporting fidelity, once per run, for the
    // relative balance guard of every finalist. It is the baseline analysis
    // when the baseline is the registered vector, else one more analysis.
    let mut analyses = 0usize;
    let reporting_nominal = match scope.evaluation(|| {
        crate::acceptance::ReportingNominal::evaluate(&config, acceptance_route, cancel)
    }) {
        None => None,
        Some(Ok(evaluated)) => {
            analyses += 1;
            Some(evaluated)
        }
        Some(Err(error)) => {
            return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error);
        }
    };
    let baseline_is_nominal = reporting_nominal
        .as_ref()
        .is_some_and(|evaluated| *evaluated.design() == nominal);
    let separate_nominal = usize::from(reporting_nominal.is_some() && !baseline_is_nominal);
    let ladder = reserve
        .evaluations
        .saturating_sub(2 + separate_nominal)
        .clamp(1, MAX_VERIFIED_CANDIDATES);
    let time_left_s = reserve.time_s.map(|_| {
        optimization
            .search_diagnostics
            .iter()
            .flat_map(|diagnostics| &diagnostics.stages)
            .find(|stage| stage.stage == "refinement")
            .map_or(0.0, |stage| stage.time_limit_s - stage.wall_time_s)
    });
    let candidates = optimization.ranked_hard_feasible_candidates(ladder);
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
        if rank > 0 && time_left_s.is_some_and(|left| started.elapsed().as_secs_f64() >= left) {
            tracing::info!(
                rank,
                "reporting-fidelity ladder stopped on the refinement time limit"
            );
            break;
        }
        analyses += 1;
        scope.enter(
            alas_opt::CancelPhase::ReportingFidelityVerification,
            rank as u64,
        );
        let verification = match scope.evaluation(|| {
            crate::acceptance::verify_finalist_cancellable(
                &config,
                candidate,
                acceptance_route,
                reporting_nominal.as_ref(),
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
    // The unmodified baseline through the same re-evaluation, so the
    // delivered design's gain is a same-model delta rather than a comparison
    // across the in-loop and reporting models.
    let baseline = (!scope.requested())
        .then(|| {
            analyses += usize::from(!baseline_is_nominal);
            crate::acceptance::compare_with_baseline(
                &config,
                &nominal,
                &delivered,
                acceptance_route,
                reporting_nominal.as_ref(),
                cancel,
            )
        })
        .flatten();
    optimization.record_delivered_acceptance(DeliveredAcceptance {
        verified,
        finalist_rejected_by,
        delivered_rejected_by,
        rejection_messages,
        candidates_evaluated: evaluated,
        delivered_is_search_finalist,
        wall_time_s: started.elapsed().as_secs_f64(),
        analyses,
        baseline,
    });

    let status = SolverOptimizationStatus::for_delivered(&optimization);
    SolverOptimizationResult {
        solver: SolverKind::Vlm,
        status,
        design: Some(delivered.design),
        optimization: Some(optimization),
        report: Some(delivered.report),
        avl_result: None,
        output_dir,
        error: None,
    }
}
