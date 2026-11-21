// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reporting-fidelity ladder's triage: the replay screen that chooses
//! which ranked candidates get a reported analysis, and the rule that ends
//! the ladder's waves.

use super::*;

/// Ranked candidates the verification replays with the configured in-loop
/// model before any reported analysis (one wave of single-thread replays on
/// a 16-thread desktop less the registered aircraft's lane). A fixed count,
/// never the worker count, so the delivered design does not depend on the
/// machine.
pub(super) const REPLAY_SCREEN: usize = 12;

/// Ranked candidates a clean sheet replays. Its refinement model differs
/// from the replay only in the empennage lattice (12 strips against the
/// configured 24); replayed on both, the 21 best designs of an A320-200
/// clean sheet kept the same verdict and the same cost to five
/// digits but one, the refinement's best, which missed the static-margin
/// floor by 0.0004 of the MAC on the finer lattice. The six best then
/// still hold five candidates for the [`VERIFIED_PER_WAVE`] verifications,
/// and the replays run on the spare threads (engineering choice from
/// measurement).
pub(super) const CLEAN_SHEET_REPLAY_SCREEN: usize = 6;

/// The ranked candidates `config`'s run replays.
fn replay_screen_size(config: &AlasConfig) -> usize {
    if config.optimizer.design_space.mode == alas_config::DesignMode::CleanSheet {
        CLEAN_SHEET_REPLAY_SCREEN
    } else {
        REPLAY_SCREEN
    }
}

/// Replayed candidates kept for the full reporting analysis, in rank order:
/// the finalist and one fallback.
pub(super) const VERIFIED_PER_WAVE: usize = 2;

/// What the replay screen leaves for the reported analyses.
pub(super) struct Screened {
    /// The ranked candidates replayed, best first.
    pub(super) candidates: Vec<DesignVector>,
    /// The candidates the replay kept (at most [`VERIFIED_PER_WAVE`]), with
    /// their search ranks, best replayed cost first; the flagged finalist
    /// when none was kept.
    pub(super) kept: Vec<(usize, crate::acceptance::Replayed)>,
    /// Why the replay rejected the finalist, when it did.
    pub(super) finalist_rejected_by: Vec<String>,
    pub(super) rejection_messages: Vec<String>,
    /// What the engine-start guard checks the delivered design against.
    pub(super) guard: EngineStartGuard,
}

/// The engine start the replays took (`SearchDiagnostics::engine_start`),
/// `(rank, replayed cost, hard-feasible)` of every kept candidate and, by
/// rank, the replays of the best-ranked screened candidates from the
/// selected engine that the screen's wave had idle lanes for.
pub(super) struct EngineStartGuard {
    start: Option<f64>,
    scores: Vec<(usize, f64, bool)>,
    cold: Vec<Option<Result<crate::acceptance::Replayed, String>>>,
}

/// Largest relative difference between the replayed costs of the delivered
/// design from the search's engine start and from the selected engine that
/// still counts as the same analysis: 2e-4, twice what the tight sizing band
/// leaves (`alas_opt` engine sizing, measured).
const ENGINE_START_COST_TOLERANCE: f64 = 2.0e-4;

impl EngineStartGuard {
    /// Check the delivered candidate against a replay that starts its engine
    /// sizing from the selected engine: the same cost within
    /// [`ENGINE_START_COST_TOLERANCE`] and the same hard verdict, or the
    /// delivery is the cold replay's verification (logged), held to the
    /// nominal like every verification. A no-op when the replays started from
    /// the selected engine already.
    ///
    /// # Errors
    ///
    /// A cancellation of the cold replay or its verification.
    pub(super) fn check(
        mut self,
        accepted: Option<(usize, FinalistVerification)>,
        config: &AlasConfig,
        (route, cancel): (
            Option<&crate::acceptance::AcceptanceRoute>,
            Option<&AtomicBool>,
        ),
        (nominal, masses): (
            Option<&crate::acceptance::ReportingNominal>,
            &crate::acceptance::MassNominals,
        ),
    ) -> Result<Option<(usize, FinalistVerification)>, String> {
        let (Some(start), Some((rank, verification))) = (self.start, accepted.as_ref()) else {
            return Ok(accepted);
        };
        let Some(&(_, warm_cost, warm_feasible)) = self.scores.iter().find(|(r, ..)| r == rank)
        else {
            return Ok(accepted);
        };
        let cold = self
            .cold
            .get_mut(*rank)
            .and_then(Option::take)
            .unwrap_or_else(|| {
                crate::acceptance::Replayed::run(config, &verification.design, cancel)
            });
        let cold = match cold {
            Err(error) if error.starts_with("Cancelled safely") => return Err(error),
            Err(error) => {
                tracing::warn!(%error, start, "engine-start guard: the cold replay failed");
                return Ok(accepted);
            }
            Ok(cold) => cold,
        };
        let difference = (cold.cost() - warm_cost).abs() / warm_cost.abs().max(f64::MIN_POSITIVE);
        if difference <= ENGINE_START_COST_TOLERANCE && cold.hard_feasible() == warm_feasible {
            tracing::info!(
                difference,
                start,
                "engine-start guard: the cold replay agrees"
            );
            return Ok(accepted);
        }
        tracing::warn!(
            difference,
            start,
            cold_feasible = cold.hard_feasible(),
            warm_feasible,
            "engine-start guard: the delivered design is reported from the selected engine"
        );
        let rank = *rank;
        match crate::acceptance::verify_replayed(cold, route) {
            Err(error) if error.starts_with("Cancelled safely") => Err(error),
            Err(error) => {
                tracing::warn!(%error, "engine-start guard: cold verification failed, warm kept");
                Ok(accepted)
            }
            Ok(mut cold) => {
                crate::acceptance::hold_to_nominal_at_mass(
                    &mut cold, config, nominal, route, cancel, masses,
                );
                Ok(cold.accepted().then_some((rank, cold)))
            }
        }
    }
}

/// Replay the best [`REPLAY_SCREEN`] ([`CLEAN_SHEET_REPLAY_SCREEN`] on a
/// clean sheet) ranked candidates of `optimization`
/// with the configured in-loop model and keep those that hold.
///
/// # Errors
///
/// A cancellation. A search with no feasible design replays nothing: the
/// ladder then delivers the registered aircraft or start, never a design
/// the search found infeasible.
pub(super) fn screen(
    optimization: &mut OptimizationResult,
    config: &AlasConfig,
    cancel: Option<&AtomicBool>,
    workers: usize,
    scope: &alas_opt::CancelScope<'_>,
) -> Result<Screened, String> {
    // First the replay screen: the best ranked candidates replayed with the
    // configured in-loop model at once (`verification::replay_screen`). A
    // staged search refines on a coarser model, so a candidate at a hard
    // limit may not hold on the replay; such a candidate is rejected here,
    // as the reporting analysis would reject it, at a fraction of its cost.
    let candidates = if optimization.history.n_valid() == 0 {
        Vec::new()
    } else {
        optimization.ranked_hard_feasible_candidates(replay_screen_size(config))
    };
    let engine_start = optimization
        .search_diagnostics
        .as_ref()
        .map_or((None, None), |diagnostics| {
            (diagnostics.engine_start, diagnostics.engine_slope)
        });
    // A run that starts its engines from the search's start also replays the
    // best-ranked candidates from the selected engine with the threads the
    // wave leaves idle: the delivery guard compares the delivered design
    // with that replay, which then adds no wall time after the verification.
    // Only as many as keep every replay lane's thread share
    // ([`super::verification::replay_screen`]): a cold replay that took
    // threads from the warm ones lengthened the A320-200 clean-sheet screen
    // by 47 s. A delivered design without a replay here gets
    // its own afterwards; a replay is the same whichever lane makes it, so
    // the delivery does not depend on the count.
    let count = candidates.len();
    let spare = if engine_start.0.is_some() {
        spare_replays(workers, count)
    } else {
        0
    };
    let jobs: Vec<(usize, bool)> = (0..count)
        .map(|rank| (rank, false))
        .chain((0..spare).map(|rank| (rank, true)))
        .collect();
    // The supervisor's live progress, if it asked: the replays of this wave.
    let progress = alas_opt::search_progress::progress_for(cancel);
    if let Some(progress) = &progress {
        let stage = alas_opt::search_progress::SearchStage::Verification;
        progress.enter(stage, jobs.len() as u64);
    }
    let mut replays =
        super::verification::replay_screen(&jobs, workers, scope, |&(rank, cold)| {
            let start = if cold { (None, None) } else { engine_start };
            let replayed =
                crate::acceptance::Replayed::run_from(config, &candidates[rank], cancel, start);
            if let Some(progress) = &progress {
                // Counted, not scored: a replay's cost is on another model.
                progress.finished(&[(f64::NAN, false)]);
            }
            replayed
        })?;
    let cold = replays.split_off(count).into_iter().map(Some).collect();
    let mut finalist_rejected_by = Vec::new();
    let mut rejection_messages: Vec<String> = Vec::new();
    let mut kept: Vec<(usize, crate::acceptance::Replayed)> = Vec::new();
    for (rank, replayed) in replays.into_iter().enumerate() {
        match replayed {
            Err(error) if error.starts_with("Cancelled safely") => return Err(error),
            Err(error) => {
                tracing::warn!(%error, rank, "candidate could not be replayed");
                optimization.record_verification(
                    rank,
                    &candidates[rank],
                    alas_opt::TraceClass::Failed,
                );
                if rank == 0 {
                    finalist_rejected_by = vec!["replay_failed".to_owned()];
                    rejection_messages = vec![error];
                }
            }
            Ok(replayed) if !replayed.hard_feasible() => {
                optimization.record_verification(
                    rank,
                    &candidates[rank],
                    alas_opt::TraceClass::Rejected,
                );
                if rank == 0 {
                    finalist_rejected_by = replayed
                        .violated_hard_ids()
                        .into_iter()
                        .map(|id| format!("hard:{id}"))
                        .collect();
                    rejection_messages = vec![replayed.not_hard_feasible()];
                }
            }
            Ok(replayed) => kept.push((rank, replayed)),
        }
    }
    // The candidates that hold are verified in the order of their replayed
    // cost, not the search's: the search ranked them on its own coarser
    // model, the replay on the configured one. A stable sort keeps the
    // search order between equal costs.
    order_by_replayed_cost(&mut kept, crate::acceptance::Replayed::cost);
    let kept_scores = kept
        .iter()
        .map(|(rank, replayed)| (*rank, replayed.cost(), replayed.hard_feasible()))
        .collect();
    Ok(Screened {
        guard: EngineStartGuard {
            start: engine_start.0,
            scores: kept_scores,
            cold,
        },
        candidates,
        kept,
        finalist_rejected_by,
        rejection_messages,
    })
}

/// Extra replays, at most `count`, a screen of `count` candidates on
/// `workers` threads runs without lowering the threads of any lane:
/// [`super::verification::replay_screen`] gives each of its lanes
/// `workers / lanes` threads, one lane per replay up to `workers`. With
/// more candidates than threads every lane runs one thread and several
/// replays in turn; the extras then fill the lanes the last turn leaves idle,
/// so the screen keeps its number of turns.
fn spare_replays(workers: usize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let lanes = workers.max(1);
    if count > lanes {
        return (count.div_ceil(lanes) * lanes - count).min(count);
    }
    let share = |jobs: usize| workers / jobs.clamp(1, workers.max(1));
    let threads = share(count);
    (0..=count)
        .take_while(|&extra| count + extra <= workers.max(count) && share(count + extra) == threads)
        .last()
        .unwrap_or(0)
}

/// Keep the best [`VERIFIED_PER_WAVE`] of the candidates that held on the
/// replay, ordered by their replayed `cost` (lower first); a stable sort, so
/// equal costs keep the search's rank order.
fn order_by_replayed_cost<T>(kept: &mut Vec<(usize, T)>, cost: impl Fn(&T) -> f64) {
    kept.sort_by(|(_, left), (_, right)| cost(left).total_cmp(&cost(right)));
    kept.truncate(VERIFIED_PER_WAVE);
}

/// Whether the ladder may stop after the jobs `done`: a finalist accepted
/// once held to the registered aircraft; a cancellation or a failed
/// registered analysis also ends it. A deliverable registered (or start)
/// analysis does not: it is retained only after the kept candidates were
/// verified, inside the reserve, so a rejected finalist is followed by the
/// next kept candidate (on the A320 clean sheet the finalist failed the
/// landing static-margin floor and the start was delivered unbeaten while
/// the next replayed candidates held about 3.6 % better).
pub(super) fn may_stop(
    done: &[(usize, LadderJob)],
    precomputed: Option<&crate::acceptance::ReportingNominal>,
    reference_adaptation: bool,
    (config, acceptance_route, cancel): (
        &AlasConfig,
        Option<&AcceptanceRoute>,
        Option<&AtomicBool>,
    ),
    mass_nominals: &crate::acceptance::MassNominals,
) -> bool {
    let nominal = precomputed.or_else(|| {
        done.iter().find_map(|(_, job)| match job {
            LadderJob::Nominal(Some(Ok(nominal))) => Some(nominal),
            _ => None,
        })
    });
    let guard = nominal.filter(|_| reference_adaptation);
    done.iter().any(|(_, job)| match job {
        LadderJob::Nominal(Some(Err(_))) => true,
        LadderJob::Nominal(_) => false,
        LadderJob::Finalist(Ok(verification)) => {
            let mut held = verification.clone();
            crate::acceptance::hold_to_nominal_at_mass(
                &mut held,
                config,
                guard,
                acceptance_route,
                cancel,
                mass_nominals,
            );
            held.accepted()
        }
        LadderJob::Finalist(Err(error)) => error.starts_with("Cancelled safely"),
    })
}

/// The most violated hard constraint of `design` under the search's own
/// model, with its value, limit and unit; the analysis failure, by its
/// cause, when the design cannot be analysed; `None` only on a cancellation.
pub(super) fn worst_hard_violation(
    config: &AlasConfig,
    design: &DesignVector,
    cancel: Option<&AtomicBool>,
) -> Option<alas_opt::ViolatedConstraint> {
    // A design the analysis cannot close violates the requirement to be
    // analysable: the failure is its worst violation, named by its cause, as
    // a failed check (actual 1 against limit 0). A cancellation names nothing.
    let assessment = match alas_opt::assess_product_candidate_cancellable(config, design, cancel) {
        Ok(assessment) => assessment,
        Err(reason) if reason == "cancelled" => return None,
        Err(reason) => {
            return Some(alas_opt::ViolatedConstraint {
                id: reason,
                actual: 1.0,
                limit: 0.0,
                unit: "bool".to_owned(),
                normalized_violation: 1.0,
            })
        }
    };
    let violated = assessment.violated_hard_ids();
    assessment
        .residuals
        .iter()
        .filter(|row| violated.contains(&row.id))
        .max_by(|left, right| {
            left.normalized_violation
                .total_cmp(&right.normalized_violation)
        })
        .map(|row| alas_opt::ViolatedConstraint {
            id: row.id.to_owned(),
            actual: row.actual,
            limit: row.limit,
            unit: row.unit.to_owned(),
            normalized_violation: row.normalized_violation,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kept_candidates_follow_the_replayed_cost_not_the_search_rank() {
        // Search ranks 0..4 whose replayed costs flip the order of 0 and 2.
        let mut kept: Vec<(usize, f64)> = vec![(0, 0.30), (1, 0.30), (2, 0.10), (4, 0.50)];
        order_by_replayed_cost(&mut kept, |cost| *cost);
        let ranks: Vec<usize> = kept.iter().map(|(rank, _)| *rank).collect();
        assert_eq!(
            ranks,
            vec![2, 0],
            "best replayed cost first, ties in search order"
        );
    }

    #[test]
    fn cold_replays_take_only_threads_no_warm_lane_would_have() {
        // Six clean-sheet replays on 15 threads run two threads each; a
        // seventh lane keeps that share, an eighth would halve it.
        assert_eq!(spare_replays(15, 6), 1);
        // Twelve on 15 run one thread each, with three lanes to spare.
        assert_eq!(spare_replays(15, 12), 3);
        // Fewer threads than replays: the second turn of six replays on four
        // one-thread lanes leaves two lanes idle, on five lanes four.
        assert_eq!(spare_replays(4, 6), 2);
        assert_eq!(spare_replays(5, 6), 4);
        assert_eq!(spare_replays(3, 6), 0);
        assert_eq!(spare_replays(15, 0), 0);
        // Seven on 15 run two threads each with one idle, too few for a lane.
        assert_eq!(spare_replays(15, 7), 0);
    }

    #[test]
    fn a_design_the_analysis_cannot_build_is_reported_by_its_failure() {
        let config = AlasConfig::default();
        let design = DesignVector {
            span_m: -1.0,
            ..DesignVector::default()
        };
        let Some(worst) = worst_hard_violation(&config, &design, None) else {
            panic!("an unanalysable design still names its worst violation");
        };
        assert!(!worst.id.is_empty());
        assert!(worst.normalized_violation > 0.0);
        assert!(worst.normalized_violation.is_finite());
    }
}
