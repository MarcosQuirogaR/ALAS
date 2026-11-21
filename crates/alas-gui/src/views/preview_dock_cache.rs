// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Hand the dock its tab-adjusted scene without copying the aircraft.
//!
//! `AppState::update_preview_scene` already stores the scene adjusted for the
//! visible tab behind an `Arc`, so the dock normally shares it. Copying it
//! instead cost a deep clone of every projected face on each camera frame
//! (about 3.4 ms for the A380-800's 72 000 elements, measured). A scene that
//! still needs the adjustment (a caller that set `preview_scene` directly)
//! is adjusted once per `(view_key, revision, tab)` and cached.

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

/// Whether `scene` already reads as `tab` shows it.
fn adjusted_for(scene: &Scene, tab: PreviewTab) -> bool {
    tab != PreviewTab::Cabin || (scene.title.is_none() && !scene.render_title)
}

/// Return the scene to display for `tab`: `source` itself when it is already
/// adjusted, else an adjusted copy cached for `(view_key, revision, tab)`.
pub(super) fn dock_scene(
    ctx: &Context,
    view_key: &str,
    revision: u64,
    tab: PreviewTab,
    source: &Arc<Scene>,
) -> Arc<Scene> {
    if adjusted_for(source, tab) {
        return Arc::clone(source);
    }
    let id = Id::new(("preview_dock_scene_cache", view_key));
    if let Some(cached) = ctx.data(|data| data.get_temp::<CachedDockScene>(id)) {
        if cached.revision == revision && cached.tab == tab {
            return cached.scene;
        }
    }
    let scene = Arc::new(preview_scene_for_tab((**source).clone(), tab));
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
    fn an_exterior_scene_is_shared_without_a_copy() {
        let ctx = Context::default();
        let mut scene = Scene::new(10.0, 10.0, None);
        scene.title = Some("Exterior".to_owned());
        let source = Arc::new(scene);

        let first = dock_scene(&ctx, "dock", 1, PreviewTab::Exterior, &source);
        let second = dock_scene(&ctx, "dock", 2, PreviewTab::Exterior, &source);

        assert!(Arc::ptr_eq(&first, &source));
        assert!(Arc::ptr_eq(&second, &source));
    }

    #[test]
    fn an_unadjusted_cabin_scene_is_adjusted_once_per_revision() {
        let ctx = Context::default();
        let mut scene = Scene::new(10.0, 10.0, None);
        scene.title = Some("Cabin / Payload \u{2013} 3D".to_owned());
        let source = Arc::new(scene);

        let first = dock_scene(&ctx, "dock", 1, PreviewTab::Cabin, &source);
        let again = dock_scene(&ctx, "dock", 1, PreviewTab::Cabin, &source);
        let next = dock_scene(&ctx, "dock", 2, PreviewTab::Cabin, &source);

        assert!(Arc::ptr_eq(&first, &again));
        assert!(!Arc::ptr_eq(&first, &next));
        assert!(first.title.is_none());
        assert!(!first.render_title);
    }

    #[test]
    fn an_adjusted_cabin_scene_is_shared() {
        let ctx = Context::default();
        let source = Arc::new(super::preview_scene_for_tab(
            Scene::new(10.0, 10.0, None),
            PreviewTab::Cabin,
        ));

        let shown = dock_scene(&ctx, "dock", 1, PreviewTab::Cabin, &source);

        assert!(Arc::ptr_eq(&shown, &source));
    }
}
