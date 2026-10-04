// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Frame-cost memoization for `AppState`'s derived, expensive-to-rebuild
//! values: the typed configuration, the Advanced form live preview, and the
//! Results gallery figures.
//!
//! Split out of [`crate::state`] to keep that module under the project's line
//! limit; everything here is a free function or an `impl AppState` block.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alas_config::AlasConfig;
use alas_report::scene::Scene;

use crate::state::AppState;

/// A content fingerprint of a `serde_json::Value`, used to memoize
/// [`AppState::typed_config`] without an owned copy of `config_values`.
///
/// `serde_json::Map` is a `BTreeMap` in this workspace (the `preserve_order`
/// feature is not enabled), so its iteration order is already a
/// deterministic function of the key content: two values that are `==` by
/// `serde_json::Value`'s own structural equality always hash the same way
/// here, independent of the order fields were inserted or mutated in.
pub(crate) fn config_fingerprint(value: &serde_json::Value) -> u64 {
    fn hash_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
        match value {
            serde_json::Value::Null => 0_u8.hash(hasher),
            serde_json::Value::Bool(b) => {
                1_u8.hash(hasher);
                b.hash(hasher);
            }
            serde_json::Value::Number(n) => {
                2_u8.hash(hasher);
                n.to_string().hash(hasher);
            }
            serde_json::Value::String(s) => {
                3_u8.hash(hasher);
                s.hash(hasher);
            }
            serde_json::Value::Array(items) => {
                4_u8.hash(hasher);
                items.len().hash(hasher);
                for item in items {
                    hash_value(item, hasher);
                }
            }
            serde_json::Value::Object(map) => {
                5_u8.hash(hasher);
                map.len().hash(hasher);
                for (key, item) in map {
                    key.hash(hasher);
                    hash_value(item, hasher);
                }
            }
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_value(value, &mut hasher);
    hasher.finish()
}

/// A content fingerprint of `design_values`, the companion key
/// [`crate::config_edit_design_space::AppState::enforce_design_space_fixed_variables`]
/// memoizes on alongside [`config_fingerprint`]. `BTreeMap` iteration is
/// already key-ordered, so this is deterministic independent of edit order.
pub(crate) fn design_values_fingerprint(
    design_values: &std::collections::BTreeMap<String, f64>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (key, value) in design_values {
        key.hash(&mut hasher);
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

impl AppState {
    /// The typed configuration the JSON edit buffer currently represents.
    ///
    /// Returns `None` while the buffer is transiently unreadable (a numeric
    /// field left mid-edit, say).
    ///
    /// Memoized behind a content fingerprint of `config_values` (measured
    /// cheaper than the clone+decode it guards): `config_values` is
    /// `pub` and mutated directly from dozens of call sites with no single
    /// setter to hook a revision counter into, so a fingerprint recomputed
    /// from the actual current value, rather than a counter that depends on
    /// every mutation site remembering to bump it, cannot silently
    /// desynchronize from the data it describes.
    pub fn typed_config(&self) -> Option<AlasConfig> {
        let fingerprint = config_fingerprint(&self.config_values);
        if let Some((cached_fingerprint, cached)) = self.typed_config_memo.borrow().as_ref() {
            if *cached_fingerprint == fingerprint {
                return Some((**cached).clone());
            }
        }
        let config: AlasConfig = if self
            .config_values
            .pointer("/optimizer/objective/mtow_sizing")
            .is_none()
        {
            AlasConfig::from_value(&self.config_values).ok()?
        } else {
            serde_json::from_value(self.config_values.clone()).ok()?
        };
        *self.typed_config_memo.borrow_mut() = Some((fingerprint, Arc::new(config.clone())));
        Some(config)
    }

    /// The Advanced form live preview for `id`, memoized against every input
    /// `crate::scene::build_page_preview` reads.
    ///
    /// Returns the built scene together with a revision suitable for
    /// `SceneView::cache_revision`: the key fingerprint itself, since
    /// `build_page_preview` is a deterministic function of exactly the
    /// fingerprinted inputs, so equal fingerprints are equal scenes and a
    /// changed fingerprint is always a genuinely different scene.
    pub fn cached_page_preview(&self, id: &str) -> Option<(Arc<Scene>, u64)> {
        let camera = self.preview_cameras.get(id).copied().unwrap_or_default();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_fingerprint(&self.config_values).hash(&mut hasher);
        design_values_fingerprint(&self.design_values).hash(&mut hasher);
        // Airfoil names resolve through the runtime registry, which a custom
        // import can change without touching `config_values`.
        alas_geom::airfoil_io::generation().hash(&mut hasher);
        // A completed run for this form swaps the preview's mass source from
        // the pre-run draft to the sized optimized report.
        self.run_identity.hash(&mut hasher);
        self.pipeline_result_complete.hash(&mut hasher);
        id.hash(&mut hasher);
        self.theme.figure_theme_name().hash(&mut hasher);
        alas_i18n::get_language().hash(&mut hasher);
        camera.pitch_deg.to_bits().hash(&mut hasher);
        camera.yaw_deg.to_bits().hash(&mut hasher);
        camera.zoom.to_bits().hash(&mut hasher);
        let key = hasher.finish();

        if let Some((cached_key, cached)) = self.page_preview_cache.borrow().as_ref() {
            if *cached_key == key {
                return Some((Arc::clone(cached), key));
            }
        }
        let scene = Arc::new(crate::scene::build_page_preview(self, id)?);
        *self.page_preview_cache.borrow_mut() = Some((key, Arc::clone(&scene)));
        Some((scene, key))
    }

    /// Build a result figure once per run/theme/id and reuse it between repaints.
    pub fn cached_result_figure(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
    ) -> Option<Arc<Scene>> {
        self.cached_result_figure_with_camera(key, id, config, theme, None)
    }

    /// Build a result figure with an explicit projection camera and cache it.
    pub fn cached_result_figure_with_camera(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
        camera: Option<alas_report::scene::Camera3D>,
    ) -> Option<Arc<Scene>> {
        if let Some((scene, _)) = self.result_figure_cache.get(key) {
            return scene.clone();
        }
        let scene = crate::scene::build_result_figure_with_camera(self, id, config, theme, camera)
            .flatten()
            .map(Arc::new);
        self.insert_result_figure(key, scene.clone());
        scene
    }

    /// Replace one cached result scene after its projection camera changes.
    pub fn rebuild_result_figure_with_camera(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
        camera: alas_report::scene::Camera3D,
    ) -> Option<Arc<Scene>> {
        let scene =
            crate::scene::build_result_figure_with_camera(self, id, config, theme, Some(camera))
                .flatten()
                .map(Arc::new);
        self.insert_result_figure(key, scene.clone());
        scene
    }

    /// Store a freshly built (or unavailable) result figure under a fresh
    /// revision. The counter never resets, so a revision is never reused for
    /// two different scene contents at the same cache key.
    fn insert_result_figure(&mut self, key: &str, scene: Option<Arc<Scene>>) {
        self.result_figure_revision_counter = self.result_figure_revision_counter.wrapping_add(1);
        self.result_figure_cache
            .insert(key.to_owned(), (scene, self.result_figure_revision_counter));
    }

    /// The revision a cached result figure was last built at, for
    /// `SceneView::cache_revision`. Zero for a key with no cached entry
    /// (never a revision a real build produces, since the counter starts at
    /// one), so a missed cache miss cannot alias a real scene's revision.
    pub fn result_figure_revision(&self, key: &str) -> u64 {
        self.result_figure_cache
            .get(key)
            .map(|(_, revision)| *revision)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod typed_config_memo_tests {
    use super::AppState;

    #[test]
    fn a_direct_config_values_edit_invalidates_the_typed_config_memo() {
        let mut state = AppState::default();
        let first = state.typed_config().expect("default config decodes");
        assert!(state.typed_config_memo.borrow().is_some());

        // `config_values` is `pub` and mutated directly all over the crate
        // (see the module doc on `typed_config_memo`); this is that pattern.
        state.config_values["requirements"]["mtow_kg"] =
            serde_json::json!(first.requirements.mtow_kg + 500.0);

        let second = state.typed_config().expect("edited config decodes");
        assert!(
            (second.requirements.mtow_kg - (first.requirements.mtow_kg + 500.0)).abs() < 1e-9,
            "typed_config must reflect the direct mutation, not a stale cached value"
        );
    }

    #[test]
    fn repeated_reads_of_an_unchanged_buffer_return_equal_configs() {
        let state = AppState::default();
        let first = state.typed_config().expect("default config decodes");
        let second = state.typed_config().expect("cached read decodes");
        assert_eq!(first, second);
    }
}

#[cfg(test)]
mod cached_page_preview_tests {
    use super::AppState;

    #[test]
    fn repeat_calls_with_nothing_changed_return_the_same_cached_scene() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let generation = alas_geom::airfoil_io::generation();
        let (first, first_revision) = state
            .cached_page_preview("geometry")
            .expect("the default configuration previews its own geometry");

        let (second, second_revision) = state
            .cached_page_preview("geometry")
            .expect("an unchanged read must keep previewing");

        // Another test may re-register airfoils in between, which rightly
        // invalidates the preview; only an untouched registry must reuse it.
        if alas_geom::airfoil_io::generation() != generation {
            return;
        }
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "an unchanged read must reuse the cached Arc<Scene>, not rebuild it"
        );
        assert_eq!(first_revision, second_revision);
    }

    #[test]
    fn an_airfoil_registry_change_invalidates_the_cached_preview() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let (_, revision_before) = state
            .cached_page_preview("geometry")
            .expect("the default configuration previews its own geometry");

        // Re-registering the same records leaves `config_values` untouched
        // but is a registry mutation, as a same-name re-import would be.
        alas_geom::airfoil_io::replace_records(alas_geom::airfoil_io::records())
            .expect("re-registering the current records is valid");

        let (_, revision_after) = state
            .cached_page_preview("geometry")
            .expect("the default configuration previews its own geometry");
        assert_ne!(revision_before, revision_after);
    }

    #[test]
    fn a_config_edit_invalidates_the_cached_preview() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let (_, revision_before) = state
            .cached_page_preview("geometry")
            .expect("the default configuration previews its own geometry");

        state.config_values["requirements"]["mtow_kg"] = serde_json::json!(
            state.config_values["requirements"]["mtow_kg"]
                .as_f64()
                .unwrap_or(50_000.0)
                + 500.0
        );

        let (_, revision_after) = state
            .cached_page_preview("geometry")
            .expect("the edited configuration still previews");
        assert_ne!(
            revision_before, revision_after,
            "a configuration edit must invalidate the cached preview"
        );
    }

    #[test]
    fn a_different_preview_id_is_cached_and_built_separately() {
        let mut state = AppState::default();
        state.enforce_design_space_fixed_variables();
        let (_, geometry_revision) = state
            .cached_page_preview("geometry")
            .expect("geometry previews");
        let (_, threeview_revision) = state
            .cached_page_preview("threeview")
            .expect("the three-view preview");
        assert_ne!(
            geometry_revision, threeview_revision,
            "different preview ids must not collide in the single-slot memo key"
        );
    }
}

#[cfg(test)]
mod result_figure_revision_tests {
    use super::AppState;
    use alas_config::AlasConfig;
    use alas_report::scene::Camera3D;

    #[test]
    fn a_rebuild_gets_a_new_revision_but_a_cache_hit_keeps_its_revision() {
        let mut state = AppState::default();
        let config = AlasConfig::default();
        let key = "test-key";
        let id = "mass";

        let _ = state.cached_result_figure(key, id, &config, "Dark");
        let revision_after_build = state.result_figure_revision(key);
        assert_ne!(
            revision_after_build, 0,
            "a built entry (even an unavailable None scene) must have a real revision"
        );

        // Same key, unchanged inputs: the existing cache entry must be
        // reused, and its revision must not move, or every static result
        // card would get a new (and needlessly re-rasterized) GPU texture
        // every frame despite showing the same scene.
        let _ = state.cached_result_figure(key, id, &config, "Dark");
        assert_eq!(
            state.result_figure_revision(key),
            revision_after_build,
            "an unmodified cache hit must not bump the revision"
        );

        // An explicit rebuild at the same key (a camera change) must be
        // treated as new content even though the key is unchanged, since a
        // snapshot mid-run can replace a figure without the run identity
        // (part of the key) changing.
        let _ =
            state.rebuild_result_figure_with_camera(key, id, &config, "Dark", Camera3D::default());
        assert_ne!(
            state.result_figure_revision(key),
            revision_after_build,
            "an explicit rebuild must get a new revision even at the same key"
        );
    }

    #[test]
    fn a_missing_cache_entry_has_revision_zero_which_no_real_build_produces() {
        let state = AppState::default();
        assert_eq!(state.result_figure_revision("never-built"), 0);
    }
}
