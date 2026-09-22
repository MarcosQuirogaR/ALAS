// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ordered, cumulative snapshots from independently completing analyses.

use std::sync::Mutex;

/// Serializes merging and publication, so a slow worker cannot overwrite a
/// newer snapshot with an older one. Callbacks must enqueue promptly and must
/// not reenter this publisher. Non-preview runs allocate no snapshot or clones.
pub(super) struct SnapshotPublisher<'a, T> {
    state: Option<Mutex<T>>,
    publish: Option<&'a (dyn Fn(T) + Sync)>,
}

impl<'a, T: Clone> SnapshotPublisher<'a, T> {
    pub(super) fn new(
        publish: Option<&'a (dyn Fn(T) + Sync)>,
        initial: impl FnOnce() -> T,
    ) -> Self {
        let state = publish.map(|publish| {
            let value = initial();
            publish(value.clone());
            Mutex::new(value)
        });
        Self { state, publish }
    }

    pub(super) fn update(&self, update: impl FnOnce(&mut T)) {
        if let (Some(state), Some(publish)) = (&self.state, self.publish) {
            // A callback panic is already a failed pipeline worker. Do not
            // continue publishing a potentially inconsistent poisoned state.
            let Ok(mut state) = state.lock() else { return };
            update(&mut state);
            publish(state.clone());
        }
    }
}

#[cfg(test)]
// In a test module a failing unwrap or expect is the assertion failing, not a
// library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn fast_analysis_is_visible_before_slow_analysis_finishes() {
        let (tx, rx) = mpsc::channel();
        let publish = |value| tx.send(value).unwrap();
        let snapshots = SnapshotPublisher::new(Some(&publish), || [false; 2]);
        assert_eq!(rx.recv().unwrap(), [false, false]);
        let (release, blocked) = mpsc::channel();
        std::thread::scope(|scope| {
            let snapshots = &snapshots;
            scope.spawn(move || {
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                snapshots.update(|state| state[0] = true);
            });
            scope.spawn(move || snapshots.update(|state| state[1] = true));
            // Release the slow worker even when the assertion fails, avoiding
            // a blocked scoped join during failure unwinding.
            let early = rx.recv_timeout(Duration::from_secs(2));
            release.send(()).unwrap();
            assert_eq!(early.unwrap(), [false, true]);
        });
        assert_eq!(rx.recv().unwrap(), [true, true]);
    }

    #[test]
    fn concurrent_updates_never_lose_completed_data_or_regress() {
        let delivered = Mutex::new(Vec::new());
        let publish = |value| delivered.lock().unwrap().push(value);
        let snapshots = SnapshotPublisher::new(Some(&publish), || [false; 16]);
        std::thread::scope(|scope| {
            for index in 0..16 {
                let snapshots = &snapshots;
                scope.spawn(move || snapshots.update(|state| state[index] = true));
            }
        });
        let values = delivered.into_inner().unwrap();
        assert_eq!(values.len(), 17);
        for (count, value) in values.iter().enumerate() {
            assert_eq!(value.iter().filter(|ready| **ready).count(), count);
        }
        assert_eq!(values.last(), Some(&[true; 16]));
    }

    #[test]
    fn ordinary_runs_do_not_build_or_clone_preview_data() {
        let snapshots = SnapshotPublisher::<Vec<u8>>::new(None, || {
            panic!("disabled preview must not build data")
        });
        snapshots.update(|_| panic!("disabled preview must not update data"));
    }
}
