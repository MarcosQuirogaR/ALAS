// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reporting analyses in objective-ordered, bounded parallel waves.

use std::time::Instant;

/// Join every admitted analysis before selecting a result or observing a stop.
///
/// The first `required` jobs always run, together, even when the
/// verification reserve has already expired; later waves are admitted only
/// while the reserve lasts. Each lane installs its nested VLM work in its own
/// Rayon pool holding an equal share of the workers, so a short wave does not
/// leave the machine idle. The nested kernels are element-wise maps, so the
/// thread count does not change any result. `stop` sees every completed job
/// in order and ends the ladder after the wave it returns true for.
#[allow(clippy::too_many_arguments)] // the ladder's limits, its clock and the two job callbacks
pub(super) fn evaluate_ordered<T: Send>(
    count: usize,
    required: usize,
    workers: usize,
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
            workers.max(outstanding)
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
}
