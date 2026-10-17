// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Large-stack threads for analysis and optimisation work.
//!
//! The pipeline holds large stack frames (full-analysis reports, FE and
//! mission stages). The Windows main thread has a 1 MiB stack and Rust's
//! default spawned thread 2 MiB; the clean-sheet runs overflowed the former
//! (final pass 2026-10-16). Every thread that runs a pipeline, optimiser or
//! analysis uses [`ANALYSIS_STACK_BYTES`]. Stack memory is reserved address
//! space, committed only when touched. The GUI event loop itself must stay on
//! the main thread (winit), so only workers use these helpers.

use std::io;
use std::thread::{Builder, JoinHandle};

/// Stack size of analysis and optimisation threads: 256 MiB.
pub const ANALYSIS_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Spawn a named worker thread with [`ANALYSIS_STACK_BYTES`] of stack.
///
/// # Errors
///
/// The operating-system error when the thread cannot be created.
pub fn spawn_analysis_thread<F, T>(name: &str, work: F) -> io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Builder::new()
        .name(name.to_owned())
        .stack_size(ANALYSIS_STACK_BYTES)
        .spawn(work)
}

/// Run `work` on a large-stack thread and wait for it. A panic in `work`
/// is re-raised on the caller; a failure to create the thread runs `work` on
/// the calling thread instead.
pub fn run_on_large_stack<T, F>(work: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
    let shared = std::sync::Arc::clone(&slot);
    let spawned = spawn_analysis_thread("alas-main-work", move || {
        let work = shared.lock().ok().and_then(|mut guard| guard.take());
        work.map(|work| work())
    });
    match spawned {
        Ok(handle) => match handle.join() {
            Ok(Some(value)) => value,
            Ok(None) => unreachable_missing_work(),
            Err(payload) => std::panic::resume_unwind(payload),
        },
        Err(_) => {
            let work = slot.lock().ok().and_then(|mut guard| guard.take());
            match work {
                Some(work) => work(),
                None => unreachable_missing_work(),
            }
        }
    }
}

#[cold]
fn unreachable_missing_work() -> ! {
    panic!("the large-stack work closure was consumed twice")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deep(depth: usize) -> usize {
        // Roughly 1 KiB per frame, kept alive by the black box.
        let buffer = [1_u8; 1024];
        let below = if depth == 0 { 0 } else { deep(depth - 1) };
        below + usize::from(std::hint::black_box(buffer)[0])
    }

    #[test]
    fn work_with_a_deep_stack_completes_on_the_large_stack_thread() {
        // 20 000 frames of about 1 KiB exceed the 2 MiB default stack.
        assert_eq!(run_on_large_stack(|| deep(20_000)), 20_001);
    }

    #[test]
    fn a_panic_in_the_work_reaches_the_caller() {
        let result = std::panic::catch_unwind(|| run_on_large_stack(|| -> u8 { panic!("boom") }));
        assert!(result.is_err());
    }
}
