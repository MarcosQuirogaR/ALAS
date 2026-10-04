// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded parallel execution and Stage 3 status marking.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

use crate::types::AirfoilCandidateResult;

/// Run indexed jobs with a bounded worker count and return results by index.
///
/// Workers claim indices atomically and only the coordinator invokes the
/// progress callback. This keeps cancellation and callback ordering stable,
/// while sorting the collected results prevents completion timing from
/// changing the ranked candidate order.
pub(super) fn run_bounded_indexed<T, F, P>(
    job_count: usize,
    worker_limit: usize,
    cancellation: Arc<AtomicBool>,
    cancel_check: Option<&(dyn Fn() -> bool + Sync)>,
    job: F,
    mut on_result: P,
) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
    P: FnMut(usize, &T),
{
    if job_count == 0 || cancellation.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let workers = job_count.min(worker_limit.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = channel();
    let mut output = Vec::with_capacity(job_count);
    // Stops the cancellation monitor once the workers have drained. A flag
    // of its own, not `cancellation`: callers read `cancellation` afterwards
    // to learn whether the sweep was actually cancelled.
    let finished = AtomicBool::new(false);
    thread::scope(|scope| {
        let job_ref = &job;
        if let Some(cancel_check) = cancel_check {
            let cancellation = cancellation.clone();
            let finished = &finished;
            scope.spawn(move || {
                while !cancellation.load(Ordering::Relaxed) && !finished.load(Ordering::Relaxed) {
                    if cancel_check() {
                        cancellation.store(true, Ordering::Relaxed);
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(20));
                }
            });
        }
        for _ in 0..workers {
            let sender = sender.clone();
            let next = next.clone();
            let cancellation = cancellation.clone();
            scope.spawn(move || loop {
                if cancellation.load(Ordering::Relaxed) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= job_count {
                    break;
                }
                let value = job_ref(index);
                if sender.send((index, value)).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        for (index, value) in receiver {
            on_result(index, &value);
            output.push((index, value));
        }
        // Stop the scoped cancellation monitor after all workers have
        // drained. Without this release edge a non-cancelled sweep would
        // keep the monitor alive until `thread::scope` tried to join it.
        finished.store(true, Ordering::Relaxed);
    });
    output.sort_by_key(|(index, _)| *index);
    output
}

/// Preserve the ranking while making a selected-but-unresolved Stage 3 visible.
pub(super) fn mark_mses_not_configured(candidates: &mut [AirfoilCandidateResult]) {
    for candidate in candidates.iter_mut().filter(|candidate| candidate.refined) {
        candidate.mses_status = Some("not_configured".to_owned());
        candidate.mses_error = Some(
            "MSES verification was selected, but no MSES installation was resolved. Configure Setup > External Tools."
                .to_owned(),
        );
    }
}
