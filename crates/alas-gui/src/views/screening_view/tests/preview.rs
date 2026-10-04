// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tests for the candidate outline preview.

use super::super::preview::*;
use super::super::*;
use egui::vec2;

#[test]
fn screening_preview_filter_cache_invalidates_without_changing_selection() {
    let mut preview = crate::screening::ScreeningPreview::default();
    preview.update_filter("");
    let all_count = preview.filtered_names().len();
    assert!(all_count > 1000);
    let cached = preview.filtered_names().as_ptr();
    preview.update_filter("");
    assert_eq!(cached, preview.filtered_names().as_ptr());
    preview.select("rae2822");
    let geometry = preview.coordinates().unwrap().to_vec();
    for query in ["naca", "SC2*", "rae2822, sc20412", "no-such-section", ""] {
        preview.update_filter(query);
        assert_eq!(
            preview.filtered_names(),
            alas_screen::runner::filter_names(
                &alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils(),
                query
            )
        );
        assert_eq!(preview.selected(), Some("rae2822"));
        assert_eq!(preview.coordinates().unwrap(), geometry);
    }
    assert_eq!(preview.filtered_names().len(), all_count);
}

#[test]
fn screening_preview_virtual_selector_paints_visible_rows_and_selects_geometry() {
    let ctx = egui::Context::default();
    let mut state = crate::screening::ScreeningState::default();
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(760.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| show_screening_preview(&mut state, ui));
            },
        )
    };
    let locate = |output: &egui::FullOutput, name: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == name => {
                    Some(text.pos + vec2(5.0, 5.0))
                }
                _ => None,
            })
            .unwrap()
    };
    let click = |pos, pressed| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            },
        ]
    };
    let output = frame(vec![]);
    let pos = locate(&output, "2032c");
    frame(click(pos, true));
    frame(click(pos, false));
    let output = frame(vec![]);
    let names = alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils();
    let rows = output
        .shapes
        .iter()
        .filter(|shape| match &shape.shape {
            egui::Shape::Text(text) => names.contains(&text.galley.job.text.as_str()),
            _ => false,
        })
        .count();
    assert!(
        (2..40).contains(&rows),
        "only viewport rows painted, got {rows}"
    );
    let target = names[1];
    let pos = locate(&output, target);
    frame(click(pos, true));
    frame(click(pos, false));
    assert_eq!(state.preview.selected(), Some(target));
    assert_eq!(
        state.preview.coordinates().unwrap(),
        alas_geom::airfoil_library::AirfoilLibrary::get(target)
            .unwrap()
            .coordinates
    );
}

#[test]
fn screening_preview_renders_before_results_in_all_themes_and_widths() {
    use crate::theme::{apply_theme, AppTheme};
    for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
        for width in [320.0, 760.0, 1400.0] {
            let ctx = egui::Context::default();
            apply_theme(theme, &ctx);
            let mut screening = crate::screening::ScreeningState::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_screening_preview(&mut screening, ui));
                },
            );
            assert!(screening.preview.coordinates().is_some());
            let outline = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) if path.points.len() > 20 => Some(path),
                    _ => None,
                })
                .expect("resolved outline is painted before any sweep");
            assert!(outline.points.iter().all(|point| point.x >= 0.0
                && point.x <= width
                && point.y >= 0.0
                && point.y <= 900.0));
        }
    }
}

#[test]
fn screening_preview_is_first_card_and_has_no_removed_subtitle() {
    let ctx = egui::Context::default();
    let mut state = crate::state::AppState::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1400.0, 1200.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show_screening_content(&mut state, ui));
        },
    );

    let text_position = |text: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text_shape) if text_shape.galley.job.text == text => {
                    Some(text_shape.pos)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing rendered text: {text}"))
    };

    assert!(text_position("Airfoil outline").y < text_position("Options").y);
    assert!(text_position("Airfoil outline").y < text_position("Run screening").y);
    assert!(!output.shapes.iter().any(|shape| {
        matches!(
            &shape.shape,
            egui::Shape::Text(text_shape)
                if text_shape.galley.job.text
                    == "Inspect a library section without changing the aircraft. Coordinates are x/c and y/c at equal scale."
        )
    }));
}

#[test]
fn screening_preview_uses_library_geometry_before_and_during_run() {
    let mut state = crate::screening::ScreeningState::default();
    state.preview.select("rae2822");
    let expected = alas_geom::airfoil_library::AirfoilLibrary::get("rae2822").unwrap();
    assert_eq!(
        state.preview.coordinates().unwrap(),
        expected.coordinates.as_slice()
    );
    assert!(state.result.is_none());
    state.running = true;
    state.preview.select("sc20412");
    assert_eq!(state.preview.selected(), Some("sc20412"));
    state.preview.select("missing-section-for-preview-test");
    assert!(state.preview.coordinates().is_none());
}

#[test]
fn screening_preview_selection_does_not_mutate_configuration() {
    let mut state = AppState::default();
    let before = serde_json::to_value(state.typed_config().unwrap()).unwrap();
    let design = state.design_values.clone();
    state.screening.preview.select("rae2822");
    state.screening.preview.select("sc20412");
    assert_eq!(
        before,
        serde_json::to_value(state.typed_config().unwrap()).unwrap()
    );
    assert_eq!(design, state.design_values);
}

#[test]
fn screening_preview_projection_has_equal_axes_and_positive_y_up() {
    let points = screening_outline_points(
        &[(0.0, 0.0), (1.0, 0.0), (0.0, 0.2)],
        egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, 100.0)),
    );
    assert!((points[1].x - points[0].x - 300.0).abs() < 1e-4);
    assert!((points[0].y - points[2].y - 60.0).abs() < 1e-4);
}

#[test]
fn screening_preview_remains_selected_when_results_reorder_or_clear() {
    use alas_screen::types::{AirfoilCandidateResult, AirfoilScreeningResult};
    let mut state = crate::screening::ScreeningState::default();
    state.preview.select("rae2822");
    state.result = Some(AirfoilScreeningResult {
        candidates: vec![
            AirfoilCandidateResult {
                name: "rae2822".into(),
                ..Default::default()
            },
            AirfoilCandidateResult {
                name: "sc20412".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    state.result.as_mut().unwrap().candidates.reverse();
    assert_eq!(state.preview.selected(), Some("rae2822"));
    state.result = None;
    assert_eq!(state.preview.selected(), Some("rae2822"));
}
