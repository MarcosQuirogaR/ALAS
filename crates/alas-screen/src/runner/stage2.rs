// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stage 2 of the screening: the 3-D refinement of the shortlist.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use super::workers::run_bounded_indexed;
use crate::types::AirfoilCandidateResult;

/// Refine `results[i]` for every `i` in `shortlist` with `refine`, one
/// candidate per core, and report whether the sweep was cancelled.
///
/// Each refinement does its own geometry build, mass passes and trim, and
/// touches nothing shared, so running them concurrently changes no number.
/// Completions are drained through an index-ordered buffer, so the progress
/// messages (and their running `ok` count) are the ones the sequential loop
/// produced, whichever worker finishes first.
pub(super) fn refine_shortlist(
    results: &mut [AirfoilCandidateResult],
    shortlist: &[usize],
    refine: impl Fn(&mut AirfoilCandidateResult) + Sync,
    progress_callback: &mut Option<&mut dyn FnMut(&str)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> bool {
    let workers = thread::available_parallelism().map_or(4, |n| n.get());
    let cancellation = Arc::new(AtomicBool::new(false));
    if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
        cancellation.store(true, Ordering::Relaxed);
    }
    let mut completed = 0;
    let mut n_refined = 0;
    let mut ordered: Vec<Option<bool>> = vec![None; shortlist.len()];
    let snapshot: &[AirfoilCandidateResult] = results;
    let refined = run_bounded_indexed(
        shortlist.len(),
        workers,
        cancellation.clone(),
        should_cancel,
        |job_index| {
            let mut candidate = snapshot[shortlist[job_index]].clone();
            refine(&mut candidate);
            candidate
        },
        |job_index, candidate| {
            ordered[job_index] = Some(candidate.refined);
            while let Some(is_refined) = ordered.get_mut(completed).and_then(Option::take) {
                completed += 1;
                if is_refined {
                    n_refined += 1;
                }
                if let Some(cb) = progress_callback.as_mut() {
                    let total = shortlist.len();
                    cb(&format!(
                        "Stage 2 (3-D wing): {completed}/{total} re-simulated ({n_refined} ok)"
                    ));
                }
            }
            if should_cancel.is_some_and(|cancel_fn| cancel_fn()) {
                cancellation.store(true, Ordering::Relaxed);
            }
        },
    );
    for (job_index, candidate) in refined {
        results[shortlist[job_index]] = candidate;
    }
    cancellation.load(Ordering::Relaxed)
}
