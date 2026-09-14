// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    figure_gallery_layout, format_cg_pct_mac, fullscreen_camera_key, fullscreen_id,
    fullscreen_open, fullscreen_slot_key, fullscreen_view_key, open_fullscreen_result,
    responsive_card_layout, scene_has_external_images, set_fullscreen, unavailable_reason,
    CARD_GAP, CARD_MIN_WIDTH,
};
use crate::state::{AppState, PreviewCamera};
use alas_report::scene::{Scene, SceneElement};
use egui::Context;

#[test]
fn baseline_cg_fraction_is_displayed_once_as_percent_mac() {
    assert_eq!(format_cg_pct_mac(25.359), "25.4% MAC");
}

#[test]
fn a_missing_pipeline_run_has_an_actionable_figure_reason() {
    assert_eq!(
        unavailable_reason(&AppState::default(), "mission_profile"),
        "Not available: no completed pipeline run exists."
    );
}

#[test]
fn result_fullscreen_state_is_keyed_by_the_figure_cache_identity() {
    let ctx = Context::default();
    set_fullscreen(&ctx, "run=1;figure=mass_breakdown", true);

    assert!(fullscreen_open(&ctx, "run=1;figure=mass_breakdown"));
    assert!(!fullscreen_open(&ctx, "run=1;figure=mass_distribution"));
    assert_ne!(
        fullscreen_id("run=1;figure=mass_breakdown"),
        fullscreen_id("run=1;figure=mass_distribution")
    );
}

#[test]
fn result_fullscreen_state_keeps_one_slot_when_display_language_or_theme_changes() {
    let original = "run=1;solver=Vlm;theme=Dark;language=en;figure=mission_route_3d";
    let translated = "run=1;solver=Vlm;theme=Light;language=es;figure=mission_route_3d";
    let ctx = Context::default();
    set_fullscreen(&ctx, original, true);

    assert!(fullscreen_open(&ctx, translated));
    assert_eq!(
        fullscreen_slot_key(original),
        fullscreen_slot_key(translated)
    );
    assert_eq!(
        fullscreen_view_key(original),
        fullscreen_view_key(translated)
    );
}

#[test]
fn fullscreen_orbit_state_is_copied_without_mutating_the_card() {
    let ctx = Context::default();
    let mut state = AppState::default();
    let view_key = "run=1;figure=mission_route_3d";
    let camera_key = "result_camera::run=1;figure=mission_route_3d";
    state.view_state_mut(view_key).pan = egui::vec2(12.0, -8.0);
    *state.result_camera_mut(camera_key) = PreviewCamera::top();

    open_fullscreen_result(&mut state, &ctx, view_key, camera_key, true);
    state.view_state_mut(fullscreen_view_key(view_key)).pan = egui::vec2(33.0, 5.0);
    state
        .result_camera_mut(fullscreen_camera_key(camera_key))
        .zoom = 3.0;

    assert_eq!(state.view_state_mut(view_key).pan, egui::vec2(12.0, -8.0));
    assert_eq!(
        state.result_camera_mut(camera_key).zoom,
        PreviewCamera::top().zoom
    );
}

#[test]
fn result_cards_add_columns_only_when_the_minimum_width_fits() {
    assert_eq!(responsive_card_layout(640.0).0, 1);
    assert_eq!(responsive_card_layout(652.0).0, 2);
    assert_eq!(responsive_card_layout(984.0).0, 3);
}

#[test]
fn result_card_rows_use_all_available_width_without_a_gutter() {
    for available in [240.0, 320.0, 652.0, 760.0, 984.0, 1_280.0] {
        let (columns, card_width) = responsive_card_layout(available);
        let row_width = columns as f32 * card_width + (columns - 1) as f32 * CARD_GAP;
        assert_eq!(card_width >= CARD_MIN_WIDTH, available >= CARD_MIN_WIDTH);
        assert!((row_width - available).abs() < 0.01);
    }
}

#[test]
fn result_card_frame_padding_is_counted_inside_the_gallery_width() {
    for available in [320.0, 652.0, 984.0, 1_280.0] {
        let (columns, outer_width) = responsive_card_layout(available);
        let content_width = crate::theme::card_content_width(outer_width);
        let row_width = columns as f32 * (content_width + 2.0 * crate::theme::CARD_INNER_MARGIN_X)
            + (columns - 1) as f32 * CARD_GAP;
        assert!((row_width - available).abs() < 0.01);
    }
}

#[test]
fn model_comparison_uses_the_full_gallery_and_available_viewport_height() {
    let (columns, card_width, canvas_height) = figure_gallery_layout(1_200.0, 590.0, true);
    assert_eq!(columns, 1);
    assert_eq!(card_width, 1_200.0);
    assert_eq!(canvas_height, 538.0);

    let (_, _, short_canvas) = figure_gallery_layout(640.0, 250.0, true);
    assert_eq!(short_canvas, 320.0);
}

#[test]
fn patran_scene_images_take_the_gui_raster_display_path() {
    let mut scene = Scene::new(500.0, 500.0, None);
    scene.add(SceneElement::Image {
        source: "C:/renders/pull-up.png".to_owned(),
        x: 0.0,
        y: 0.0,
        width: 500.0,
        height: 470.0,
        source_rect: None,
    });
    assert!(scene_has_external_images(&scene));
}

#[test]
fn embedded_blue_marble_stays_on_the_scene_display_path() {
    let mut scene = Scene::new(500.0, 500.0, None);
    scene.add(SceneElement::Image {
        source: "embedded://nasa-blue-marble".to_owned(),
        x: 0.0,
        y: 0.0,
        width: 500.0,
        height: 250.0,
        source_rect: None,
    });
    assert!(!scene_has_external_images(&scene));
    scene.add(SceneElement::SphericalImage {
        source: "embedded://nasa-blue-marble".to_owned(),
        center: [250.0, 250.0],
        radius: 200.0,
        camera: alas_report::scene::Camera3D::front(),
        mirror_longitude: false,
    });
    assert!(!scene_has_external_images(&scene));
}
