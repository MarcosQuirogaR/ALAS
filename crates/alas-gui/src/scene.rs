// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure-scene construction for the live preview, the per-page side previews,
//! and the results gallery.
//!
//! Kept out of [`crate::state`] so that module stays the data hub: these
//! functions read a whole [`AppState`] and return the [`Scene`] a viewport then
//! draws. Everything here degrades to `None` rather than panicking: a figure
//! that needs a completed run, or a geometry that will not build mid-edit, is a
//! blank slot, not a crash.

mod dispatch_ids;
mod localization;
mod preview;
mod results;
mod solver;
#[cfg(test)]
mod tests;

pub use dispatch_ids::{PREVIEW_DISPATCH_IDS, RESULT_DISPATCH_IDS, SCREENING_DISPATCH_IDS};
pub use preview::{build_page_preview, build_page_preview_with_camera, build_preview_scene};
pub use results::{
    build_result_figure, build_result_figure_with_camera, build_result_scene,
    build_screening_figure,
};

use alas_config::AlasConfig;
use alas_report::scene::{Scene, SceneElement};

use localization::localize_scene_text;

/// Translate a report scene for the active desktop language before display.
///
/// The report crate remains language-neutral so exported reports may select
/// their own language. The GUI applies this final display boundary to titles,
/// labels and supported dynamic footers.
pub fn localize_scene_for_display(mut scene: Scene) -> Scene {
    if let Some(title) = &mut scene.title {
        *title = localize_scene_text(title);
    }
    for element in &mut scene.elements {
        match element {
            SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                *text = localize_scene_text(text);
            }
            _ => {}
        }
    }
    scene
}

/// Build the identity of a rendered figure cache entry.
///
/// A figure is not identified by its display id alone: a new run can publish
/// different data under the same id, and changing configuration or theme must
/// invalidate the prior scene.
pub fn figure_cache_key(
    run_identity: u64,
    config: &AlasConfig,
    theme: &str,
    figure_id: &str,
) -> String {
    let configuration = match serde_json::to_string(config) {
        Ok(value) => value,
        Err(_) => "<unserializable-config>".to_owned(),
    };
    let language = alas_i18n::get_language();
    format!(
        "run={run_identity};config={configuration};theme={theme};language={language};figure={figure_id}"
    )
}
