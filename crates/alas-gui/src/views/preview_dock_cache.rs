// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reuse the dock's tab-adjusted scene across idle repaints.
//!
//! `AppState::preview_scene` is a plain `Option<Scene>`, so displaying it
//! requires either holding a borrow of `state` across the calls that also
//! need `state` mutably (the viewport state, the camera), or cloning it.
//! [`super::preview_scene_for_tab`] took the clone-every-frame path. Most
//! repaints are idle (cursor blink, hover, unrelated animation) and leave
//! `preview_scene_revision` and the visible tab unchanged, so caching the
//! adjusted scene behind that revision turns most frames into an `Arc`
//! clone instead of a deep copy of the aircraft geometry. A camera drag or
//! configuration edit still bumps the revision and rebuilds it once.

use std::sync::Arc;

use alas_report::scene::Scene;
use egui::{Context, Id};

use super::preview_scene_for_tab;
use crate::state::PreviewTab;

#[derive(Clone)]
struct CachedDockScene {
    revision: u64,
    tab: PreviewTab,
    scene: Arc<Scene>,
}

/// Return the scene to display for `tab`, cloning `source` only when the
/// cache is missing or stale for `(view_key, revision, tab)`.
pub(super) fn dock_scene(
    ctx: &Context,
    view_key: &str,
    revision: u64,
    tab: PreviewTab,
    source: &Scene,
) -> Arc<Scene> {
    let id = Id::new(("preview_dock_scene_cache", view_key));
    if let Some(cached) = ctx.data(|data| data.get_temp::<CachedDockScene>(id)) {
        if cached.revision == revision && cached.tab == tab {
            return cached.scene;
        }
    }
    let scene = Arc::new(preview_scene_for_tab(source.clone(), tab));
    let entry = CachedDockScene {
        revision,
        tab,
        scene: scene.clone(),
    };
    ctx.data_mut(|data| data.insert_temp(id, entry));
    scene
}

#[cfg(test)]
mod tests {
    use super::dock_scene;
    use crate::state::PreviewTab;
    use alas_report::scene::Scene;
    use egui::Context;
    use std::sync::Arc;

    #[test]
    fn a_repeat_call_with_the_same_key_reuses_the_cached_arc_without_recloning() {
        let ctx = Context::default();
        let mut scene = Scene::new(10.0, 10.0, None);
        scene.title = Some("Exterior".to_owned());

        let first = dock_scene(&ctx, "dock", 1, PreviewTab::Exterior, &scene);
        let second = dock_scene(&ctx, "dock", 1, PreviewTab::Exterior, &scene);

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_revision_change_rebuilds_the_cached_scene() {
        let ctx = Context::default();
        let scene = Scene::new(10.0, 10.0, None);

        let first = dock_scene(&ctx, "dock", 1, PreviewTab::Exterior, &scene);
        let second = dock_scene(&ctx, "dock", 2, PreviewTab::Exterior, &scene);

        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_tab_change_rebuilds_and_hides_the_generated_heading_for_cabin() {
        let ctx = Context::default();
        let mut scene = Scene::new(10.0, 10.0, None);
        scene.title = Some("Cabin / Payload - 3D".to_owned());

        let exterior = dock_scene(&ctx, "dock", 1, PreviewTab::Exterior, &scene);
        let cabin = dock_scene(&ctx, "dock", 1, PreviewTab::Cabin, &scene);

        assert_eq!(exterior.title.as_deref(), Some("Cabin / Payload - 3D"));
        assert!(cabin.title.is_none());
        assert!(!cabin.render_title);
    }
}
