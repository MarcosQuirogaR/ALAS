// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result-gallery geometry: responsive card columns, and each card's scene
//! with the canvas height that shows its drawing without empty bands.

use crate::state::AppState;
use crate::views::result_3d;
use alas_report::scene::Scene;

/// Minimum width of a result card before another responsive column is added.
pub(crate) const CARD_MIN_WIDTH: f32 = 320.0;
/// Horizontal space between adjacent result cards.
pub(crate) const CARD_GAP: f32 = 12.0;

/// Return a stable column count and card width for the current content pane.
///
/// The width is calculated from the whole available row, rather than from a
/// fixed card width, so a two-column row cannot leave a permanent right gutter.
pub(crate) fn responsive_card_layout(available_width: f32) -> (usize, f32) {
    let width = available_width.max(1.0);
    let columns = ((width + CARD_GAP) / (CARD_MIN_WIDTH + CARD_GAP))
        .floor()
        .clamp(1.0, 3.0) as usize;
    let card_width = (width - CARD_GAP * (columns - 1) as f32) / columns as f32;
    (columns, card_width.max(1.0))
}

/// Model Comparison is a single, information-dense overlay. Giving it the
/// whole gallery row keeps its axes and legend readable and avoids wasting the
/// results viewport below a generic 320 px canvas.
pub(super) fn figure_gallery_layout(
    available_width: f32,
    available_height: f32,
    full_width: bool,
) -> (usize, f32, f32) {
    if full_width {
        return (
            1,
            available_width.max(1.0),
            (available_height - 52.0).clamp(320.0, 640.0),
        );
    }
    let (columns, tile_width) = responsive_card_layout(available_width);
    (columns, tile_width, 320.0)
}

/// One result card's scene and the keys it is cached and viewed under.
pub(super) struct TileScene {
    pub(super) scene: Option<std::sync::Arc<Scene>>,
    pub(super) view_key: String,
    pub(super) camera_key: String,
    pub(super) orbitable: bool,
}

pub(super) fn tile_scene(
    state: &mut AppState,
    config: &alas_config::AlasConfig,
    descriptor: &alas_report::FigureDescriptor,
) -> TileScene {
    let id = descriptor.id;
    let theme = state.theme.figure_theme_name().to_owned();
    let language = alas_i18n::get_language();
    let view_key = format!(
        "run={};solver={:?};theme={theme};language={language};figure={id}",
        state.run_identity, state.selected_solver_view
    );
    let orbitable = result_3d::is_orbitable_result(id);
    let camera_key = result_3d::result_camera_key(state.run_identity, id);
    let scene = if orbitable {
        let camera = result_3d::result_camera(state, &camera_key);
        state.cached_result_figure_with_camera(&view_key, id, config, &theme, Some(camera))
    } else {
        state.cached_result_figure(&view_key, id, config, &theme)
    };
    TileScene {
        scene,
        view_key,
        camera_key,
        orbitable,
    }
}

/// Bounds of a card canvas fitted to its drawing, in points.
pub(super) const FITTED_CANVAS_MIN: f32 = 200.0;
pub(super) const FITTED_CANVAS_MAX: f32 = 560.0;

/// The canvas height that shows a static drawing at `canvas_width` with no
/// empty band: the scene's own aspect, bounded. Orbit views, external images
/// and unavailable figures keep `default`.
pub(super) fn fitted_canvas_height(tile: &TileScene, canvas_width: f32, default: f32) -> f32 {
    match tile.scene.as_deref() {
        Some(scene)
            if !tile.orbitable
                && !super::images::scene_has_external_images(scene)
                && scene.width > 0.0
                && scene.height > 0.0 =>
        {
            (canvas_width * (scene.height / scene.width) as f32)
                .clamp(FITTED_CANVAS_MIN, FITTED_CANVAS_MAX)
        }
        _ => default,
    }
}
