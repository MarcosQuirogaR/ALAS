// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The native VLM optimization branch.

use super::*;

#[path = "search_evidence.rs"]
mod search_evidence;

#[path = "verification.rs"]
mod verification;

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
    let mut persist_evidence = |result: &OptimizationResult| {
        search_evidence::persist(output_dir.as_deref(), result);
    };
    let mut optimization = match optimizer.run_cancellable_with_evidence(
        bounds,
        Some(&nominal),
        None,
        cancel,
        Some(&mut persist_evidence),
    ) {
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
    // It is job 0 of the first verification wave, so it runs beside the
    // finalists instead of ahead of them; the guard is applied once both are
    // known.
    let mut analyses = 0usize;
    let registered_design = (config.optimizer.design_space.mode
        == alas_config::DesignMode::ReferenceAdaptation)
        .then(|| alas_config::presets::get(&config.preset).ok())
        .flatten()
        .map(|preset| preset.design_vector);
    let baseline_is_nominal = registered_design == Some(nominal);
    let separate_nominal = usize::from(
        config.optimizer.design_space.mode == alas_config::DesignMode::ReferenceAdaptation
            && !baseline_is_nominal,
    );
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
    let jobs = match verification::evaluate_ordered(
        candidates.len() + 1,
        2.min(candidates.len() + 1),
        effective_config.optimizer.solver.resolved_workers(),
        started,
        time_left_s,
        &scope,
        |job| {
            if job == 0 {
                LadderJob::Nominal(crate::acceptance::ReportingNominal::evaluate(
                    &config,
                    acceptance_route,
                    cancel,
                ))
            } else {
                LadderJob::Finalist(crate::acceptance::verify_finalist_cancellable(
                    &config,
                    &candidates[job - 1],
                    acceptance_route,
                    None,
                    cancel,
                ))
            }
        },
        |done| {
            let nominal = done.iter().find_map(|(_, job)| match job {
                LadderJob::Nominal(Some(Ok(nominal))) => Some(nominal),
                _ => None,
            });
            done.iter().any(|(job, result)| match result {
                LadderJob::Nominal(Some(Err(_))) => true,
                LadderJob::Nominal(_) => false,
                LadderJob::Finalist(Ok(verification)) => {
                    let mut held = verification.clone();
                    crate::acceptance::hold_to_nominal(&mut held, &config, nominal);
                    held.accepted()
                }
                LadderJob::Finalist(Err(error)) => {
                    *job == 1 || error.starts_with("Cancelled safely")
                }
            })
        },
    ) {
        Ok(jobs) => jobs,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error),
    };
    let mut reporting_nominal = None;
    let mut verifications = Vec::with_capacity(jobs.len());
    for (job, result) in jobs {
        match result {
            LadderJob::Nominal(None) => {}
            LadderJob::Nominal(Some(Ok(evaluated))) => {
                analyses += 1;
                reporting_nominal = Some(evaluated);
            }
            LadderJob::Nominal(Some(Err(error))) => {
                return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error);
            }
            LadderJob::Finalist(result) => verifications.push((job - 1, result)),
        }
    }
    for (_, result) in &mut verifications {
        if let Ok(verification) = result {
            crate::acceptance::hold_to_nominal(verification, &config, reporting_nominal.as_ref());
        }
    }
    analyses += verifications.len();
    evaluated += verifications
        .iter()
        .filter(|(_, result)| result.is_ok())
        .count();
    for (rank, result) in verifications {
        let verification = match result {
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
                optimization.record_verification(
                    rank,
                    &candidates[rank],
                    alas_opt::TraceClass::Failed,
                );
                continue;
            }
        };
        let class = if verification.accepted() {
            alas_opt::TraceClass::Valid
        } else {
            alas_opt::TraceClass::Rejected
        };
        optimization.record_verification(rank, &candidates[rank], class);
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
    // Publish the vector the delivered report describes. The replay resolves
    // the search vector into the assessed aircraft, with a reference
    // adaptation's solved tail scale, and the report, mission and feasibility
    // are built on that aircraft; the vector it was handed is the search's.
    let delivered_design = delivered.report.design;
    let baseline_verification = reporting_nominal
        .as_ref()
        .and_then(|nominal| nominal.verified_analysis(&config, acceptance_route));
    let report = delivered.report.clone();
    let verification =
        crate::acceptance::VerifiedAnalysis::new(config, acceptance_route.cloned(), delivered);
    SolverOptimizationResult {
        solver: SolverKind::Vlm,
        status,
        design: Some(delivered_design),
        optimization: Some(optimization),
        report: Some(report),
        verification: Some(verification),
        baseline_verification,
        avl_result: None,
        output_dir,
        error: None,
    }
}

/// One job of the reporting-fidelity ladder: the registered aircraft the
/// relative balance guard compares against, or one ranked finalist.
enum LadderJob {
    Nominal(Option<Result<crate::acceptance::ReportingNominal, String>>),
    Finalist(Result<FinalistVerification, String>),
}
