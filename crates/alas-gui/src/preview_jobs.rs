// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Page previews built off the UI thread.
//!
//! The first visit to the Aircraft, Analyses and Geometry pages held the UI
//! thread 0.3-0.6 s (A320-200, measured): the wing front view (about 0.4 s),
//! the structures preview (about 0.26 s) and the mass and CG sheet (about
//! 0.4 s) were built inside the frame. A card that misses its preview now
//! queues the build here, keeps drawing the previous scene of that card (or
//! a placeholder on the very first visit), and stores the scene when the
//! worker sends it back ([`PreviewJobs::poll`]).
//!
//! A slot is one card ("page_preview::structures", the wing front view); a
//! key is the hash of everything its build reads. One build per slot is in
//! flight; a request for a newer key replaces it, and a result for any key
//! but the slot's latest request is dropped on arrival.
//!
//! Thread safety: a build owns its inputs (configuration, design vector,
//! theme, result snapshot); the only thread-local state it reads is the
//! display language, set per job as the result-figure workers do.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use alas_report::scene::Scene;

struct Done {
    slot: String,
    key: u64,
    scene: Option<Scene>,
}

/// What a card gets for its slot and key.
pub(crate) enum PreviewLookup {
    /// Built for this key (`None`: the build has no figure for these inputs).
    Ready(Option<Arc<Scene>>),
    /// Building; the previous scene of the slot, if any, to draw meanwhile.
    Pending(Option<(Arc<Scene>, u64)>),
}

/// The in-flight builds and the finished scenes, by slot.
pub(crate) struct PreviewJobs {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    ctx: Option<egui::Context>,
    enabled: bool,
    /// The key each slot last asked for.
    requested: HashMap<String, u64>,
    /// The latest finished build of each slot.
    done: HashMap<String, (u64, Option<Arc<Scene>>)>,
}

impl Default for PreviewJobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            ctx: None,
            enabled: false,
            requested: HashMap::new(),
            done: HashMap::new(),
        }
    }
}

impl PreviewJobs {
    /// Called every frame by the shell. Unit tests keep building on the
    /// calling thread unless a test enables the workers explicitly.
    pub(crate) fn attach(&mut self, ctx: &egui::Context) {
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
            self.enabled = !cfg!(test);
        }
    }

    /// Whether previews are built on worker threads.
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    #[cfg(test)]
    pub(crate) fn enable_for_test(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
        self.enabled = true;
    }

    /// The scene of `slot` for `key` when it is built or building, without
    /// queuing anything: `None` means [`Self::lookup`] must be given a build.
    pub(crate) fn peek(&self, slot: &str, key: u64) -> Option<PreviewLookup> {
        if let Some((done_key, scene)) = self.done.get(slot) {
            if *done_key == key {
                return Some(PreviewLookup::Ready(scene.clone()));
            }
        }
        (self.requested.get(slot) == Some(&key)).then(|| PreviewLookup::Pending(self.stale(slot)))
    }

    /// The latest finished scene of `slot`, whatever its key.
    fn stale(&self, slot: &str) -> Option<(Arc<Scene>, u64)> {
        self.done
            .get(slot)
            .and_then(|(done_key, scene)| scene.as_ref().map(|s| (Arc::clone(s), *done_key)))
    }

    /// The scene of `slot` for `key`, queuing `build` on a worker when it is
    /// neither built nor building.
    pub(crate) fn lookup(
        &mut self,
        slot: &str,
        key: u64,
        build: impl FnOnce() -> Option<Scene> + Send + 'static,
    ) -> PreviewLookup {
        if let Some((done_key, scene)) = self.done.get(slot) {
            if *done_key == key {
                return PreviewLookup::Ready(scene.clone());
            }
        }
        if self.requested.get(slot) != Some(&key) {
            self.requested.insert(slot.to_owned(), key);
            let tx = self.tx.clone();
            let ctx = self.ctx.clone();
            let slot_name = slot.to_owned();
            let language = alas_i18n::get_language();
            let spawned = crate::worker_stack::spawn_analysis_thread("alas-preview", move || {
                alas_i18n::set_language(Some(&language));
                // A preview builder that panics must not take the card with
                // it: the slot reports no figure, as an unbuildable design does.
                let scene = std::panic::catch_unwind(std::panic::AssertUnwindSafe(build))
                    .ok()
                    .flatten();
                let _ = tx.send(Done {
                    slot: slot_name,
                    key,
                    scene,
                });
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
            if spawned.is_err() {
                // No thread: the card keeps its previous scene and asks again.
                self.requested.remove(slot);
            }
        }
        PreviewLookup::Pending(self.stale(slot))
    }

    /// Store the builds the workers finished. Call once per frame; returns
    /// whether any arrived.
    pub(crate) fn poll(&mut self) -> bool {
        let mut arrived = false;
        while let Ok(done) = self.rx.try_recv() {
            // Only the slot's latest request is kept: an older build finishing
            // late must not replace a newer one.
            if self.requested.get(&done.slot) != Some(&done.key) {
                continue;
            }
            self.done
                .insert(done.slot, (done.key, done.scene.map(Arc::new)));
            arrived = true;
        }
        arrived
    }

    /// Wait for every requested build (headless callers and tests).
    #[cfg(test)]
    pub(crate) fn drain(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
        while self
            .requested
            .iter()
            .any(|(slot, key)| self.done.get(slot).map(|(k, _)| k) != Some(key))
        {
            assert!(
                std::time::Instant::now() < deadline,
                "a preview build did not finish within 300 s"
            );
            if let Ok(done) = self.rx.recv_timeout(std::time::Duration::from_millis(50)) {
                if self.requested.get(&done.slot) == Some(&done.key) {
                    self.done
                        .insert(done.slot, (done.key, done.scene.map(Arc::new)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(width: f64) -> Scene {
        let mut scene = Scene::new(width, 100.0, None);
        scene.title = Some(format!("w{width}"));
        scene
    }

    #[test]
    fn a_preview_is_built_once_per_key_and_a_late_older_build_is_dropped() {
        let ctx = egui::Context::default();
        let mut jobs = PreviewJobs::default();
        jobs.enable_for_test(&ctx);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        // The first build waits until released, so the second request
        // overtakes it.
        assert!(matches!(
            jobs.lookup("slot", 1, move || {
                let _ = release_rx.recv();
                Some(scene(1.0))
            }),
            PreviewLookup::Pending(None)
        ));
        assert!(matches!(
            jobs.lookup("slot", 2, || Some(scene(2.0))),
            PreviewLookup::Pending(None)
        ));
        let _ = release_tx.send(());
        jobs.drain();
        // Give the released first build time to arrive, then poll it away.
        std::thread::sleep(std::time::Duration::from_millis(200));
        jobs.poll();
        match jobs.lookup("slot", 2, || panic!("key 2 is built")) {
            PreviewLookup::Ready(Some(built)) => assert_eq!(built.width, 2.0),
            _ => panic!("key 2 should be ready"),
        }
        // A newer key shows the previous scene while it builds.
        match jobs.lookup("slot", 3, || Some(scene(3.0))) {
            PreviewLookup::Pending(Some((stale, key))) => {
                assert_eq!(stale.width, 2.0);
                assert_eq!(key, 2);
            }
            _ => panic!("key 3 should be pending with the key-2 scene"),
        }
        jobs.drain();
    }
}
