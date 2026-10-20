// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Concurrent MSES section polars.
//!
//! The root, kink and tip polars are independent solves: each MSES run owns a
//! unique scratch directory and a single-threaded process, and no polar reads
//! another's output. They therefore run side by side, bounded by a thread
//! budget, and the results are returned in request order so the run's data is
//! exactly what the serial loop produced. Only the pressure solve, which
//! starts from the root polar's checkpoint, stays after them.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::sync::PoisonError;

use alas_exec::tool_runtime::RuntimeSettings;

/// Section polars to run at once: the cores left by the analyses sharing the
/// machine (`ALAS_CONCURRENT_JOBS`), never below one and never above `tasks`.
pub(super) fn polar_parallelism(settings: &RuntimeSettings, tasks: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (cores / settings.concurrent_jobs.max(1))
        .max(1)
        .min(tasks.max(1))
}

/// Run task `0` (the root) and tasks `1..tasks` (outboard sections) with at
/// most `parallelism` running at once; results are indexed by task.
///
/// Task `0` always runs. A later task is skipped, leaving `None`, once
/// `cancelled` is true when it is about to start, and so is every task after
/// the first skipped one in request order, as the serial loop stopped at the
/// first cancellation. With `parallelism == 1` the tasks run in order on the
/// calling thread.
pub(super) fn run_polars<T: Send>(
    tasks: usize,
    parallelism: usize,
    run: &(impl Fn(usize) -> T + Sync),
    cancelled: &(impl Fn() -> bool + Sync),
) -> Vec<Option<T>> {
    let slots: Vec<Mutex<Option<T>>> = (0..tasks).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let worker = || loop {
        let index = next.fetch_add(1, Ordering::SeqCst);
        if index >= tasks {
            break;
        }
        if index > 0 && cancelled() {
            continue;
        }
        let value = run(index);
        *slots[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
    };
    let workers = parallelism.clamp(1, tasks.max(1));
    if workers == 1 {
        worker();
    } else {
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers).map(|_| scope.spawn(worker)).collect();
            for handle in handles {
                handle
                    .join()
                    .unwrap_or_else(|error| std::panic::resume_unwind(error));
            }
        });
    }
    let mut results: Vec<Option<T>> = slots
        .into_iter()
        .map(|slot| slot.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect();
    // Serial semantics: nothing after the first skipped section is kept.
    if let Some(first_skipped) = results.iter().skip(1).position(Option::is_none) {
        for later in results.iter_mut().skip(first_skipped + 2) {
            *later = None;
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn results_keep_request_order_at_any_parallelism() {
        for parallelism in [1, 2, 3, 8] {
            let results = run_polars(3, parallelism, &|index| index * 10, &|| false);
            assert_eq!(results, vec![Some(0), Some(10), Some(20)]);
        }
    }

    #[test]
    fn cancellation_stops_later_sections_but_never_the_root() {
        let flag = AtomicBool::new(true);
        let results = run_polars(3, 1, &|index| index, &|| flag.load(Ordering::SeqCst));
        assert_eq!(results, vec![Some(0), None, None]);
        let results = run_polars(3, 3, &|index| index, &|| flag.load(Ordering::SeqCst));
        assert_eq!(results[0], Some(0));
        assert!(results[1].is_none() && results[2].is_none());
    }

    #[test]
    fn a_skipped_section_drops_every_later_one() {
        // Serial order: section 1 skipped means section 2 is never started.
        let started = AtomicUsize::new(0);
        let results = run_polars(3, 1, &|index| index, &|| {
            started.fetch_add(1, Ordering::SeqCst) == 0
        });
        assert_eq!(results, vec![Some(0), None, None]);
    }

    #[test]
    fn the_budget_follows_the_concurrent_jobs_and_the_task_count() {
        let alone = RuntimeSettings::default();
        assert!(polar_parallelism(&alone, 3) >= 1);
        assert_eq!(polar_parallelism(&alone, 1), 1);
        let crowded = RuntimeSettings {
            concurrent_jobs: 1_000_000,
            ..RuntimeSettings::default()
        };
        assert_eq!(polar_parallelism(&crowded, 3), 1);
    }
}
