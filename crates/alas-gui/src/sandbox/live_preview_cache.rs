// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keep the lofted exterior across camera updates in both guided previews.
//!
//! The dock asks for the model on the UI thread at every edit. Lofting and
//! partitioning an aircraft takes 40-70 ms (A320-200, measured), so once the
//! app turns on [`build_in_background`] the dock's [`model_for_dock`] queues
//! a new model on a worker and keeps drawing the previous one until it
//! arrives. The other callers (the wing front view, already built on a
//! preview worker, and the framed backgrounds) build in place.

use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::{cell::RefCell, rc::Rc};

use alas_config::{DesignVector, GeometryConfig};
use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::SandboxSceneModel;

#[derive(Clone, PartialEq)]
struct ModelKey {
    geometry: GeometryConfig,
    design: DesignVector,
    airfoil_generation: u64,
}

/// A model being built on a worker for `key`.
struct Pending {
    key: ModelKey,
    rx: Receiver<Option<SandboxSceneModel>>,
}

#[derive(Default)]
struct ModelCache {
    entry: Option<(ModelKey, Rc<SandboxSceneModel>)>,
    pending: Option<Pending>,
    /// The last key whose aircraft did not build: asked again, it stays
    /// without a model instead of queueing the same failure every frame.
    failed: Option<ModelKey>,
}

fn build_model(key: &ModelKey) -> Option<SandboxSceneModel> {
    let plane = AircraftBuilder::new(Some(key.geometry.clone()))
        .build(Some(&key.design), true)
        .ok()?;
    Some(super::build_sandbox_model(&plane))
}

impl ModelCache {
    fn get(&mut self, key: ModelKey) -> Option<Rc<SandboxSceneModel>> {
        if let Some((previous, model)) = &self.entry {
            if *previous == key {
                return Some(Rc::clone(model));
            }
        }
        let model = Rc::new(build_model(&key)?);
        self.entry = Some((key, Rc::clone(&model)));
        Some(model)
    }

    /// The model for `key` when it is built; meanwhile the previous model (or
    /// none on the first request), with the build queued on a worker that
    /// asks `ctx` for a repaint when it is done.
    fn get_in_background(
        &mut self,
        key: ModelKey,
        ctx: &egui::Context,
    ) -> Option<Rc<SandboxSceneModel>> {
        let stale = self.entry.as_ref().map(|(_, model)| Rc::clone(model));
        if self
            .entry
            .as_ref()
            .is_some_and(|(previous, _)| *previous == key)
        {
            return stale;
        }
        if self.failed.as_ref() == Some(&key) {
            return None;
        }
        if let Some(pending) = &self.pending {
            if pending.key == key {
                match pending.rx.try_recv() {
                    Err(TryRecvError::Empty) => return stale,
                    Ok(Some(model)) => {
                        self.pending = None;
                        let model = Rc::new(model);
                        self.entry = Some((key, Rc::clone(&model)));
                        return Some(model);
                    }
                    Ok(None) | Err(TryRecvError::Disconnected) => {
                        self.pending = None;
                        self.failed = Some(key);
                        return None;
                    }
                }
            }
        }
        // A newer key replaces the build in flight; its result is dropped.
        let (tx, rx) = channel();
        let job = key.clone();
        let ctx = ctx.clone();
        let spawned = crate::worker_stack::spawn_analysis_thread("alas-live-model", move || {
            let model =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_model(&job)))
                    .ok()
                    .flatten();
            let _ = tx.send(model);
            ctx.request_repaint();
        });
        if spawned.is_err() {
            // No thread: build in place, as before.
            self.pending = None;
            return self.get(key);
        }
        self.pending = Some(Pending { key, rx });
        stale
    }

    /// Take the worker's model if it arrived; whether the dock must ask again.
    fn poll(&mut self) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        match pending.rx.try_recv() {
            Err(TryRecvError::Empty) => false,
            Ok(Some(model)) => {
                let Some(Pending { key, .. }) = self.pending.take() else {
                    return false;
                };
                self.entry = Some((key, Rc::new(model)));
                true
            }
            Ok(None) | Err(TryRecvError::Disconnected) => {
                self.failed = self.pending.take().map(|pending| pending.key);
                true
            }
        }
    }
}

thread_local! {
    // A single bounded entry is sufficient for the dock and its detached
    // window, which share one aircraft. Exact input equality also makes reuse
    // safe across separate AppStates on this UI thread, without relying on a
    // revision increment at every public configuration mutation site.
    static CACHE: RefCell<ModelCache> = RefCell::new(ModelCache::default());
    /// Set by the app on its UI thread: the dock's model is built on workers.
    static BACKGROUND: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

/// Build the dock's model on worker threads from now on (this thread only).
/// The app calls it every frame; unit tests keep the in-place build.
pub(crate) fn build_in_background(ctx: &egui::Context) {
    BACKGROUND.with(|background| {
        let mut background = background.borrow_mut();
        if background.is_none() {
            *background = Some(ctx.clone());
        }
    });
}

/// Store the dock's model when its worker is done; `true` when the dock must
/// rebuild its scene. The app calls it once per frame.
pub(crate) fn poll_background() -> bool {
    CACHE.with(|cache| cache.borrow_mut().poll())
}

/// Whether the dock's model is being built on a worker.
pub(crate) fn background_pending() -> bool {
    CACHE.with(|cache| cache.borrow().pending.is_some())
}

/// [`model`] for the dock: on a worker once [`build_in_background`] is on.
pub(super) fn model_for_dock(
    geometry: GeometryConfig,
    design: DesignVector,
) -> Option<Rc<SandboxSceneModel>> {
    let Some(ctx) = BACKGROUND.with(|background| background.borrow().clone()) else {
        return model(geometry, design);
    };
    let key = ModelKey {
        geometry,
        design,
        airfoil_generation: alas_geom::airfoil_io::generation(),
    };
    CACHE.with(|cache| cache.borrow_mut().get_in_background(key, &ctx))
}

pub(super) fn model(
    geometry: GeometryConfig,
    design: DesignVector,
) -> Option<Rc<SandboxSceneModel>> {
    let key = ModelKey {
        geometry,
        design,
        // Imported airfoils can change the loft without changing their names.
        airfoil_generation: alas_geom::airfoil_io::generation(),
    };
    CACHE.with(|cache| cache.borrow_mut().get(key))
}

#[cfg(test)]
// Tests assert on known-good fixtures, where a panic is the failure report.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn key() -> ModelKey {
        ModelKey {
            geometry: GeometryConfig::default(),
            design: DesignVector::default(),
            airfoil_generation: 0,
        }
    }

    #[test]
    fn camera_reprojections_reuse_the_lofted_model() {
        let mut cache = ModelCache::default();
        let first = cache.get(key()).expect("aircraft builds");
        let second = cache.get(key()).expect("aircraft builds");
        assert!(Rc::ptr_eq(&first, &second));
        let options = super::super::live_preview_options();
        let original = first.render(None, None, &options).0;
        let rotated = second
            .render(
                Some(alas_report::scene::Camera3D {
                    azim_deg: 120.0,
                    ..Default::default()
                }),
                None,
                &options,
            )
            .0;
        assert_ne!(original, rotated);
    }

    /// The dock keeps the previous model while the new one builds, and takes
    /// the new one when the worker is done.
    #[test]
    fn a_background_build_keeps_the_previous_model_until_it_arrives() {
        let ctx = egui::Context::default();
        let mut cache = ModelCache::default();
        assert!(
            cache.get_in_background(key(), &ctx).is_none(),
            "nothing yet"
        );
        let first = loop {
            if let Some(model) = cache.get_in_background(key(), &ctx) {
                break model;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let mut edit = key();
        edit.design.span_m += 1.0;
        let meanwhile = cache
            .get_in_background(edit.clone(), &ctx)
            .expect("the previous model");
        assert!(Rc::ptr_eq(&first, &meanwhile));
        let edited = loop {
            let model = cache
                .get_in_background(edit.clone(), &ctx)
                .expect("a model");
            if !Rc::ptr_eq(&first, &model) {
                break model;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        // The same key again is the cached model, with no new build.
        let again = cache.get_in_background(edit, &ctx).expect("a model");
        assert!(Rc::ptr_eq(&edited, &again));
        assert!(cache.pending.is_none());
    }

    #[test]
    fn design_geometry_and_airfoil_edits_each_rebuild_the_model() {
        let mut cache = ModelCache::default();
        let first = cache.get(key()).expect("aircraft builds");
        let mut design_edit = key();
        design_edit.design.span_m += 1.0;
        let edited = cache.get(design_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));

        let first = cache.get(key()).expect("aircraft builds");
        let mut geometry_edit = key();
        geometry_edit.geometry.wing.n_subdivisions += 1;
        let edited = cache.get(geometry_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));

        let first = cache.get(key()).expect("aircraft builds");
        let mut import_edit = key();
        import_edit.airfoil_generation += 1;
        let edited = cache.get(import_edit).expect("edited aircraft builds");
        assert!(!Rc::ptr_eq(&first, &edited));
    }
}
