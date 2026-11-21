// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The native VLM optimization branch.

use super::*;

#[path = "search_evidence.rs"]
mod search_evidence;

#[path = "verification.rs"]
mod verification;

#[path = "replay_triage.rs"]
mod replay_triage;

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
///   accepted, the registered aircraft (or a clean sheet's start) is
///   delivered with its own reporting analysis and findings, and the run says
///   so; a design the search found infeasible is never delivered. No limit,
///   residual or tolerance is weakened anywhere in that ladder.
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
    // The registered aircraft's reporting analysis does not depend on the
    // search, so it runs beside it on one thread of its own instead of in
    // the verification wave after it: the post-search verification is then
    // the finalists alone. A failed or cancelled search stops it.
    let registered_stop = AtomicBool::new(false);
    let (searched, registered) = std::thread::scope(|threads| {
        let registered = threads.spawn(|| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .ok()?;
            Some(pool.install(|| {
                if config.optimizer.design_space.mode
                    == alas_config::DesignMode::ReferenceAdaptation
                {
                    crate::acceptance::ReportingNominal::evaluate(
                        &config,
                        acceptance_route,
                        Some(&registered_stop),
                    )
                } else {
                    Some(crate::acceptance::ReportingNominal::of_baseline(
                        &config,
                        &nominal,
                        acceptance_route,
                        Some(&registered_stop),
                    ))
                }
            }))
        });
        let searched = optimizer.run_cancellable_with_evidence(
            bounds,
            Some(&nominal),
            None,
            cancel,
            Some(&mut persist_evidence),
        );
        if !matches!(&searched, Ok(result) if !result.was_cancelled()) {
            registered_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        while !registered.is_finished() {
            if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
                registered_stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        (searched, registered.join().ok().flatten())
    });
    let mut optimization = match searched {
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

    let LadderOutcome {
        delivered,
        reporting_nominal,
    } = match run_reporting_ladder(
        ReportingLadder {
            solver: SolverKind::Vlm,
            config: &config,
            effective_config: &effective_config,
            nominal: &nominal,
            acceptance_route,
            cancel,
            registered,
        },
        &mut optimization,
    ) {
        Ok(outcome) => outcome,
        Err(error) => return SolverOptimizationResult::failed(SolverKind::Vlm, output_dir, error),
    };
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
pub(super) enum LadderJob {
    Nominal(Option<Result<crate::acceptance::ReportingNominal, String>>),
    Finalist(Result<FinalistVerification, String>),
}

/// Inputs of the reporting-fidelity ladder shared by the VLM and AVL branches.
pub(super) struct ReportingLadder<'a> {
    pub(super) solver: SolverKind,
    /// The configuration the published analysis uses.
    pub(super) config: &'a AlasConfig,
    /// The seeded configuration the search ran with.
    pub(super) effective_config: &'a AlasConfig,
    pub(super) nominal: &'a DesignVector,
    pub(super) acceptance_route: Option<&'a AcceptanceRoute>,
    pub(super) cancel: Option<&'a AtomicBool>,
    /// The registered aircraft's reporting analysis when the caller made it
    /// beside the search (`ReportingNominal::evaluate`'s answer); `None`
    /// runs it in the ladder's own wave.
    pub(super) registered: Option<Option<Result<crate::acceptance::ReportingNominal, String>>>,
}

/// What the ladder delivers: the verified (or best-rejected) reporting
/// analysis and the registered aircraft or start it was compared against.
pub(super) struct LadderOutcome {
    pub(super) delivered: FinalistVerification,
    pub(super) reporting_nominal: Option<crate::acceptance::ReportingNominal>,
}

/// Re-evaluate the search finalist (and, if rejected, the next hard-feasible
/// candidates) at reporting fidelity and record the result on `optimization`
/// as its delivered acceptance. Shared by every optimization branch so no
/// branch can report `Completed` on in-loop validity alone.
pub(super) fn run_reporting_ladder(
    ladder: ReportingLadder<'_>,
    optimization: &mut OptimizationResult,
) -> Result<LadderOutcome, String> {
    let ReportingLadder {
        solver,
        config,
        effective_config,
        nominal,
        acceptance_route,
        cancel,
        registered,
    } = ladder;
    let nominal = *nominal;
    let label = match solver {
        SolverKind::Vlm => "VLM",
        SolverKind::Avl => "AVL",
    };
    // The ladder below marks its own phase per candidate; entering it here
    // as well would put two identical records at the same timestamp.
    let scope = alas_opt::CancelScope::attach(cancel);
    let started = Instant::now();
    let mut phases: Vec<alas_opt::LadderPhase> = Vec::new();
    let mut lap = started;
    // The ladder runs inside the refinement's verification reserve: its
    // evaluations less the baseline and the final analysis, and the stage
    // time the search left. The finalist always runs.
    let reserve = alas_opt::verification_reserve(&effective_config.optimizer.solver);
    // The registered aircraft at reporting fidelity, once per run, for the
    // relative balance guard of every finalist. It is the baseline analysis
    // when the baseline is the registered vector, else one more analysis.
    // The caller runs it beside the search when it can (`registered`); else
    // it is job 0 of the verification wave, beside the finalists. The guard
    // is applied once both are known.
    let mut analyses = 0usize;
    let registered_design = (config.optimizer.design_space.mode
        == alas_config::DesignMode::ReferenceAdaptation)
        .then(|| alas_config::presets::get(&config.preset).ok())
        .flatten()
        .map(|preset| config.band_design(preset.design_vector));
    let baseline_is_nominal = registered_design == Some(nominal);
    let time_left_s = reserve.time_s.map(|_| {
        optimization
            .search_diagnostics
            .iter()
            .flat_map(|diagnostics| &diagnostics.stages)
            .find(|stage| stage.stage == "refinement")
            .map_or(0.0, |stage| stage.time_limit_s - stage.wall_time_s)
    });
    let workers = effective_config.optimizer.solver.resolved_workers();
    let replay_triage::Screened {
        candidates,
        kept,
        mut finalist_rejected_by,
        mut rejection_messages,
        guard,
    } = replay_triage::screen(optimization, config, cancel, workers, &scope)?;
    ladder_lap(&mut phases, &mut lap, "replay_screen");
    let mut evaluated = 0usize;
    let mut accepted: Option<(usize, FinalistVerification)> = None;
    let ranks: Vec<usize> = kept.iter().map(|(rank, _)| *rank).collect();
    let slots: Vec<std::sync::Mutex<Option<crate::acceptance::Replayed>>> = kept
        .into_iter()
        .map(|(_, replayed)| std::sync::Mutex::new(Some(replayed)))
        .collect();
    // Job 0 is the registered aircraft unless analysed beside the search. The
    // finalist runs on every worker (LJ60: alone 50 s, two on half 92 s); the
    // next kept candidate follows a rejected one before a retention.
    let reference_adaptation =
        config.optimizer.design_space.mode == alas_config::DesignMode::ReferenceAdaptation;
    let mass_nominals = crate::acceptance::MassNominals::default();
    let offset = usize::from(registered.is_none());
    let precomputed = registered
        .as_ref()
        .and_then(Option::as_ref)
        .and_then(|evaluated| evaluated.as_ref().ok());
    let jobs = verification::evaluate_ordered(
        slots.len() + offset,
        1 + offset,
        workers,
        1 + offset,
        started,
        time_left_s,
        &scope,
        |job| {
            if job < offset {
                return LadderJob::Nominal(crate::acceptance::ReportingNominal::evaluate(
                    config,
                    acceptance_route,
                    cancel,
                ));
            }
            let replayed = slots[job - offset]
                .lock()
                .ok()
                .and_then(|mut slot| slot.take());
            LadderJob::Finalist(replayed.map_or_else(
                || Err("reporting-fidelity job ran twice".to_owned()),
                |replayed| crate::acceptance::verify_replayed(replayed, acceptance_route),
            ))
        },
        |done| {
            replay_triage::may_stop(
                done,
                precomputed,
                reference_adaptation,
                (config, acceptance_route, cancel),
                &mass_nominals,
            )
        },
    )?;
    ladder_lap(&mut phases, &mut lap, "verification");
    let mut registered = registered;
    let mut verifications = Vec::with_capacity(jobs.len());
    for (job, result) in jobs {
        match result {
            LadderJob::Nominal(evaluated) => registered = Some(evaluated),
            LadderJob::Finalist(result) => verifications.push((ranks[job - offset], result)),
        }
    }
    // Outside a reference adaptation the analysis made beside the search is
    // the run's own baseline (`ReportingNominal::of_baseline`): what the
    // delivery is compared with and retained against, never a relative
    // balance nominal.
    let mut reporting_nominal = None;
    let mut baseline_analysis = None;
    match registered.flatten() {
        None => {}
        Some(Ok(evaluated)) => {
            analyses += 1;
            if reference_adaptation {
                reporting_nominal = Some(evaluated);
            } else {
                baseline_analysis = Some(evaluated);
            }
        }
        Some(Err(error)) => return Err(error),
    }
    // A clean sheet from a registered preset is held to the registered
    // aircraft as a reference adaptation is: its
    // baseline analysis is the registered vector's when the run starts there.
    let hold_nominal = reporting_nominal.as_ref().or_else(|| {
        baseline_analysis.as_ref().filter(|made| {
            config.holds_to_registered_aircraft()
                && alas_config::presets::get(&config.preset)
                    .is_ok_and(|preset| made.design().same_geometry(&preset.design_vector))
        })
    });
    for (_, result) in &mut verifications {
        if let Ok(verification) = result {
            crate::acceptance::hold_to_nominal_at_mass(
                verification,
                config,
                hold_nominal,
                acceptance_route,
                cancel,
                &mass_nominals,
            );
        }
    }
    ladder_lap(&mut phases, &mut lap, "hold_to_nominal");
    analyses += candidates.len();
    evaluated += verifications
        .iter()
        .filter(|(_, result)| result.is_ok())
        .count();
    for (rank, result) in verifications {
        let verification = match result {
            Ok(verification) => verification,
            Err(error) if error.starts_with("Cancelled safely") => {
                return Err(error);
            }
            Err(error) => {
                tracing::warn!(%error, rank, "candidate could not be re-evaluated");
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
    }
    let held = (hold_nominal, &mass_nominals);
    ladder_lap(&mut phases, &mut lap, "ranking");
    let accepted = guard.check(accepted, config, (acceptance_route, cancel), held)?;
    ladder_lap(&mut phases, &mut lap, "engine_start_guard");
    // The registered aircraft is delivered when no candidate was accepted or
    // the accepted one is worse than it at reporting fidelity
    // (`acceptance::retention`): when it is worse, only if the run started
    // from it; when nothing was accepted, always.
    let source = reporting_nominal
        .as_ref()
        .filter(|_| baseline_is_nominal || accepted.is_none())
        .or(baseline_analysis.as_ref());
    let retained = source.and_then(|registered| {
        registered.retained_over(
            accepted.as_ref().map(|(_, verification)| verification),
            config.optimizer.objective.kind,
        )
    });
    let registered_retained = retained.is_some();
    // A retained aircraft with a violated hard residual is delivered, not verified.
    let retained_clean = source.is_some_and(|registered| registered.deliverable().is_some());
    let (delivered, delivered_is_search_finalist) = match (retained, accepted) {
        (Some(registered), _) => (registered, false),
        (None, Some((rank, verification))) => (verification, rank == 0),
        (None, None) => {
            // Nothing to deliver: the error names the physical cause, read
            // off the search's least-violating design (never delivered).
            let cause =
                match crate::acceptance::Replayed::run(config, &optimization.best_design, cancel) {
                    Err(error) => error,
                    Ok(replayed) => replayed.not_hard_feasible(),
                };
            let nothing =
                "no feasible design verified at reporting fidelity, no registered analysis";
            return Err(format!(
                "{label} optimization failed: {nothing}; the least-violating design: {cause}"
            ));
        }
    };
    let verified = delivered.accepted() && (retained_clean || !registered_retained);
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
            let made = reporting_nominal.as_ref().or(baseline_analysis.as_ref());
            analyses += usize::from(made.is_none_or(|made| *made.design() != nominal));
            crate::acceptance::compare_with_baseline(
                config,
                &nominal,
                &delivered,
                acceptance_route,
                made,
                cancel,
            )
        })
        .flatten();
    ladder_lap(&mut phases, &mut lap, "baseline");
    let worst_violation = (optimization.history.n_valid() == 0 && !scope.requested())
        .then(|| {
            replay_triage::worst_hard_violation(effective_config, &optimization.best_design, cancel)
        })
        .flatten();
    ladder_lap(&mut phases, &mut lap, "worst_violation");
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
        worst_violation,
        phases,
    });
    if registered_retained {
        let registered = alas_config::presets::get(&effective_config.preset)
            .is_ok_and(|preset| nominal.same_geometry(&preset.design_vector));
        if registered {
            optimization.record_registered_design_retained(nominal, verified);
        } else {
            optimization.record_start_design_retained(nominal, verified);
        }
    }

    // The start's analysis is the downstream baseline comparison as well.
    let reporting_nominal =
        reporting_nominal.or_else(|| baseline_analysis.filter(|made| *made.design() == nominal));
    Ok(LadderOutcome {
        delivered,
        reporting_nominal,
    })
}

/// Close the ladder phase that started at `lap`: record its wall time under
/// `phase` and start the next one now.
fn ladder_lap(phases: &mut Vec<alas_opt::LadderPhase>, lap: &mut Instant, phase: &str) {
    let now = Instant::now();
    phases.push(alas_opt::LadderPhase {
        phase: phase.to_owned(),
        wall_time_s: now.duration_since(*lap).as_secs_f64(),
    });
    *lap = now;
}
