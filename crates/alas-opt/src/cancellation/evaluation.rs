// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Owned cancellation and borrowed-flag forwarding for coupled evaluations.
use super::*;

/// Owned cancellation shared by a coordinator and its native evaluation workers.
#[derive(Debug, Clone)]
pub(crate) struct EvaluationCancellation(Arc<CancelWatch>);
impl PartialEq for EvaluationCancellation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl EvaluationCancellation {
    pub(crate) fn new() -> Self {
        Self(CancelWatch::new())
    }
    pub(crate) fn request(&self) {
        self.0.request_cancellation();
    }
    pub(crate) fn requested(&self) -> bool {
        self.0.flag.load(Ordering::Acquire) && CancelScope::attach(Some(self.0.flag())).requested()
    }
}

/// Forward a borrowed flag during direct finalist replay. Completion wakes the
/// scoped monitor on success or panic; no worker or borrowed pointer escapes.
pub(crate) fn forward_evaluation_cancellation<T>(
    flag: Option<&AtomicBool>,
    token: &EvaluationCancellation,
    work: impl FnOnce() -> T,
) -> T {
    let Some(flag) = flag else {
        return work();
    };
    let completed = (std::sync::Mutex::new(false), std::sync::Condvar::new());
    struct Completion<'a>(&'a (std::sync::Mutex<bool>, std::sync::Condvar));
    impl Drop for Completion<'_> {
        fn drop(&mut self) {
            *self
                .0
                 .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = true;
            self.0 .1.notify_one();
        }
    }
    std::thread::scope(|threads| {
        threads.spawn(|| {
            let scope = CancelScope::attach(Some(flag));
            loop {
                if scope.requested() {
                    token.request();
                    return;
                }
                let done = completed
                    .0
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                if *done {
                    return;
                }
                let (done, _) = completed
                    .1
                    .wait_timeout(done, std::time::Duration::from_millis(25))
                    .unwrap_or_else(|poison| poison.into_inner());
                if *done {
                    return;
                }
            }
        });
        let _complete = Completion(&completed);
        if CancelScope::attach(Some(flag)).requested() {
            token.request();
        }
        work()
    })
}
