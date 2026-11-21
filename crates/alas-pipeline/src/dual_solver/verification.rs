// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reporting analyses in objective-ordered, bounded parallel waves.

use std::time::Instant;

/// Join every admitted analysis before selecting a result or observing a stop.
///
/// The first `required` jobs always run, together, even when the
/// verification reserve has already expired; later waves are admitted only
/// while the reserve lasts. A wave holds at most `lanes` jobs (never fewer
/// than the outstanding required ones): a reporting analysis is several
/// times faster on a few threads than as one of many single-thread lanes
/// that share the machine's memory bandwidth, and the first candidates are
/// the ones most likely to be accepted, so a narrow first wave delivers
/// sooner and the later ones run only when it was rejected. Each lane
/// installs its nested VLM work in its own Rayon pool holding an equal share
/// of the workers, so a short wave does not leave the machine idle. The nested kernels are element-wise maps, so the
/// thread count does not change any result. `stop` sees every completed job
/// in order and ends the ladder after the wave it returns true for.
#[allow(clippy::too_many_arguments)] // the ladder's limits, its clock and the two job callbacks
pub(super) fn evaluate_ordered<T: Send>(
    count: usize,
    required: usize,
    workers: usize,
    lanes: usize,
    started: Instant,
    time_left_s: Option<f64>,
    scope: &alas_opt::CancelScope<'_>,
    evaluate: impl Fn(usize) -> T + Sync,
    stop: impl Fn(&[(usize, T)]) -> bool,
) -> Result<Vec<(usize, T)>, String> {
    if scope.requested() {
        return Err("Cancelled safely before reporting-fidelity verification".to_owned());
    }
    let workers = workers.max(1);
    let required = required.clamp(1, count.max(1));
    let mut completed = Vec::with_capacity(count);
    while completed.len() < count {
        let first = completed.len();
        let expired = time_left_s.is_some_and(|left| started.elapsed().as_secs_f64() >= left);
        if first >= required && (scope.requested() || expired) {
            break;
        }
        let outstanding = required.saturating_sub(first);
        let admitted = if expired {
            outstanding
        } else {
            lanes.clamp(1, workers).max(outstanding)
        }
        .min(count - first);
        let threads = (workers / admitted).max(1);
        let lanes = (0..admitted)
            .map(|_| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .map_err(|error| format!("reporting-fidelity evaluation pool: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        scope.enter(
            alas_opt::CancelPhase::ReportingFidelityVerification,
            first as u64,
        );
        let wave = std::thread::scope(|threads| {
            let handles = lanes
                .iter()
                .enumerate()
                .map(|(lane, pool)| {
                    let evaluate = &evaluate;
                    threads.spawn(move || {
                        let rank = first + lane;
                        (rank, pool.install(|| scope.evaluation(|| evaluate(rank))))
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| "reporting-fidelity verification worker panicked".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        completed.extend(wave);
        if stop(&completed) {
            break;
        }
    }
    Ok(completed)
}

/// Replay every candidate of `candidates` with `replay`, in parallel lanes
/// (at most `workers`, a lane taking the next candidate when it finishes
/// one, each holding an equal share of the workers for its nested VLM
/// work), and return the answers in candidate order.
///
/// A replay is one in-loop analysis (about 10 to 20 s on one thread), the
/// reported analysis and mission that follow it several times that, so the
/// whole screen costs about one replay of wall time on a desktop and spares
/// the reported analyses for candidates that hold on the configured model.
/// Each replay runs alone in its lane, and its nested kernels are
/// element-wise maps, so the thread count does not change any answer.
///
/// # Errors
///
/// A cancellation requested before the screen starts, or a lane that
/// panicked.
pub(super) fn replay_screen<I: Sync, T: Send>(
    candidates: &[I],
    workers: usize,
    scope: &alas_opt::CancelScope<'_>,
    replay: impl Fn(&I) -> Result<T, String> + Sync,
) -> Result<Vec<Result<T, String>>, String> {
    if scope.requested() {
        return Err("Cancelled safely before reporting-fidelity verification".to_owned());
    }
    let lanes = workers.clamp(1, candidates.len().max(1));
    let threads = (workers / lanes).max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let answers: Vec<std::sync::Mutex<Option<Result<T, String>>>> = candidates
        .iter()
        .map(|_| std::sync::Mutex::new(None))
        .collect();
    let pools = (0..lanes)
        .map(|_| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|error| format!("reporting-fidelity replay pool: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    scope.enter(alas_opt::CancelPhase::ReportingFidelityVerification, 0);
    std::thread::scope(|threads| {
        let handles: Vec<_> = pools
            .iter()
            .map(|pool| {
                let (next, answers, replay) = (&next, &answers, &replay);
                threads.spawn(move || {
                    pool.install(|| loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(design) = candidates.get(index) else {
                            break;
                        };
                        let answer = scope.evaluation(|| replay(design));
                        if let Ok(mut slot) = answers[index].lock() {
                            *slot = Some(answer);
                        }
                    });
                })
            })
            .collect();
        handles.into_iter().try_for_each(|handle| {
            handle
                .join()
                .map_err(|_| "reporting-fidelity replay worker panicked".to_owned())
        })
    })?;
    Ok(answers
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .ok()
                .flatten()
                .unwrap_or_else(|| Err("candidate was not replayed".to_owned()))
        })
        .collect())
}

// Tests construct every fixture they assert on, so a failed unwrap or
// expect is the assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Barrier;

    use super::*;

    #[test]
    fn parallel_verification_returns_objective_order_and_counts_the_whole_wave() {
        let barrier = Barrier::new(4);
        let completed = AtomicUsize::new(0);
        let results = evaluate_ordered(
            8,
            1,
            4,
            4,
            Instant::now(),
            None,
            &alas_opt::CancelScope::attach(None),
            |rank| {
                barrier.wait();
                completed.fetch_add(1, Ordering::SeqCst);
                rank
            },
            |done| done.iter().any(|(_, rank)| *rank == 1 || *rank == 3),
        )
        .expect("parallel verification");
        assert_eq!(completed.load(Ordering::SeqCst), 4);
        assert_eq!(results, vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
    }

    #[test]
    fn cancellation_joins_admitted_verifications_without_starting_another_wave() {
        let cancelled = AtomicBool::new(false);
        let barrier = Barrier::new(3);
        let completed = AtomicUsize::new(0);
        let results = evaluate_ordered(
            9,
            1,
            3,
            3,
            Instant::now(),
            None,
            &alas_opt::CancelScope::attach(Some(&cancelled)),
            |rank| {
                barrier.wait();
                cancelled.store(true, Ordering::Release);
                completed.fetch_add(1, Ordering::SeqCst);
                rank
            },
            |_| false,
        )
        .expect("joined cancellation");
        assert_eq!(completed.load(Ordering::SeqCst), 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn an_expired_reserve_runs_only_the_required_jobs_together() {
        let barrier = Barrier::new(2);
        let results = evaluate_ordered(
            8,
            2,
            1,
            1,
            Instant::now(),
            Some(0.0),
            &alas_opt::CancelScope::attach(None),
            |rank| {
                barrier.wait();
                rank
            },
            |_| false,
        )
        .expect("required jobs");
        assert_eq!(results, vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn the_replay_screen_answers_every_candidate_in_order_at_any_lane_count() {
        let candidates: Vec<alas_config::DesignVector> = (0..7)
            .map(|index| alas_config::DesignVector {
                span_m: f64::from(index),
                ..alas_config::DesignVector::default()
            })
            .collect();
        let replay = |design: &alas_config::DesignVector| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            if design.span_m == 3.0 {
                Err("not analysable".to_owned())
            } else {
                Ok(design.span_m)
            }
        };
        let expected: Vec<Result<f64, String>> = (0..7)
            .map(|index| {
                if index == 3 {
                    Err("not analysable".to_owned())
                } else {
                    Ok(f64::from(index))
                }
            })
            .collect();
        for workers in [1, 3, 16] {
            let scope = alas_opt::CancelScope::attach(None);
            let answers = replay_screen(&candidates, workers, &scope, replay).expect("screen ran");
            assert_eq!(answers, expected, "{workers} workers");
        }
        let cancelled = AtomicBool::new(true);
        let scope = alas_opt::CancelScope::attach(Some(&cancelled));
        let error = replay_screen(&candidates, 4, &scope, replay).expect_err("cancelled");
        assert!(error.starts_with("Cancelled safely"), "{error}");
    }
}
