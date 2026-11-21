// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout rules of the form pages, rendered headless in the application's
//! theme at the widths of the review's windows: the padlock of a locked Route
//! field, the Wingbox centre-spar row, and the External tools label column.

use crate::state::AppState;
use crate::test_raster::{render, write_in, Shot};

/// The padlock bodies (a 10 x 6 point rounded rectangle) drawn in `shot`.
fn lock_bodies(shot: &Shot) -> Vec<egui::Rect> {
    shot.rects
        .iter()
        .copied()
        .filter(|r| (r.width() - 10.0).abs() < 0.1 && (r.height() - 6.0).abs() < 0.1)
        .collect()
}

/// A locked Route field's padlock ends at the field's right edge: over the box,
/// never in the gutter beside it.
#[test]
fn a_route_padlock_sits_over_its_field_not_beside_it() {
    for width in [1100.0_f32, 1600.0] {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let shot = render(width, 500.0, 4, 0, |ui| {
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let mut grid = crate::views::form::CellGrid::new(ui, "route_test_grid");
                super::route_override::derived_route_cells(&mut state, ui, &mut grid);
                grid.finish(ui);
            });
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("route-locks-{width:.0}.png"),
            &shot,
        );
        let locks = lock_bodies(&shot);
        assert!(locks.len() >= 2, "{width}: padlocks drawn: {locks:?}");
        let boxes: Vec<egui::Rect> = shot
            .rects
            .iter()
            .copied()
            .filter(|r| r.width() >= 200.0 && r.height() > 16.0 && r.height() < 40.0)
            .collect();
        assert!(!boxes.is_empty(), "{width}: field boxes drawn");
        for lock in &locks {
            let field = boxes
                .iter()
                .filter(|b| b.left() <= lock.center().x && lock.center().y < b.top())
                .filter(|b| b.top() - lock.center().y < 60.0)
                .max_by(|a, b| a.left().total_cmp(&b.left()))
                .unwrap_or_else(|| panic!("{width}: a field under the lock {lock:?}"));
            assert!(
                lock.right() <= field.right() + 0.5 && lock.left() >= field.left(),
                "{width}: lock {lock:?} is over the field {field:?}"
            );
        }
    }
}

/// The Structures page's Wingbox layout group at `width` points, with the
/// center spar on or off.
fn structures(width: f32, height: f32, center_spar: bool) -> Shot {
    let mut state = AppState::default();
    state.load_preset("A320-200");
    state.config_values["structures"]["center_spar_enabled"] = serde_json::json!(center_spar);
    state.shell.detail = crate::detail::Detail::Advanced;
    let page = crate::nav::page("structures").expect("structures page");
    render(width, height, 6, 0, |ui| {
        crate::detail::publish(ui.ctx(), crate::detail::Detail::Advanced);
        egui::ScrollArea::vertical().show(ui, |ui| {
            crate::views::form_page::show_form_page(&mut state, ui, page);
        });
    })
}

/// The "Add center spar" checkbox is a cell of the page grid, and the
/// "Center spar chord position" field it shows stands beside it on its row at
/// every column count; with the spar off the field is not drawn.
#[test]
fn the_center_spar_checkbox_is_level_with_the_chord_position_field() {
    for width in [800.0_f32, 1100.0, 1600.0, 2400.0] {
        let shot = structures(width, 1400.0, true);
        write_in(
            &crate::test_raster::render_dir(),
            &format!("structures-{width:.0}.png"),
            &shot,
        );
        let centre = |needle: &str| {
            shot.texts
                .iter()
                .find(|(text, _)| text.contains(needle))
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{width}: {needle} is painted"))
        };
        // The checkbox label stands on the label line of the row, beside the
        // label of the chord-position field.
        let checkbox = centre("Add center spar");
        let field = centre("Center spar chord position");
        assert!(
            (checkbox.y - field.y).abs() < 3.0 && field.x > checkbox.x,
            "{width}: checkbox label at {checkbox:?}, chord label at {field:?}"
        );
        let off = structures(width, 1400.0, false);
        assert!(
            off.texts
                .iter()
                .any(|(text, _)| text.contains("Add center spar"))
                && !off
                    .texts
                    .iter()
                    .any(|(text, _)| text.contains("Center spar chord position")),
            "{width}: the chord position is hidden while the center spar is off"
        );
    }
}

/// The External tools labels at `width` points, rendered tall enough to hold
/// the whole page.
fn tools(width: f32) -> Shot {
    let mut state = AppState::default();
    render(width, 3200.0, 6, 0, |ui| {
        crate::views::tools_view::show_tools_view(&mut state, ui);
    })
}

/// At a wide window every option label of the page stays on one line; in every
/// window the field boxes of a card share one left edge.
#[test]
fn external_tool_labels_stay_on_one_line_when_the_card_has_room() {
    let labels = [
        "Native OpenFOAM bin directory",
        "Native OpenFOAM project directory",
        "Open-core words (OCMEM)",
    ];
    for width in [1100.0_f32, 1600.0, 2400.0] {
        let shot = tools(width);
        write_in(
            &crate::test_raster::render_dir(),
            &format!("external-tools-{width:.0}.png"),
            &shot,
        );
        let line = shot
            .texts
            .iter()
            .find(|(text, _)| text == "Executable path")
            .map(|(_, rect)| rect.height())
            .expect("a one-line label is painted");
        for label in labels {
            let found: Vec<_> = shot
                .texts
                .iter()
                .filter(|(text, _)| text.contains(label))
                .collect();
            if width >= 2400.0 {
                assert!(!found.is_empty(), "{width}: {label} is painted");
            }
            for (_, rect) in found {
                if width >= 2400.0 {
                    assert!(
                        rect.height() < line * 1.4,
                        "{width}: {label} wraps: {rect:?}"
                    );
                }
            }
        }
        // The Browse buttons of one column of cards start at one x.
        let mut lefts: Vec<f32> = shot
            .texts
            .iter()
            .filter(|(text, _)| text == "Browse...")
            .map(|(_, rect)| (rect.left() * 2.0).round() / 2.0)
            .collect();
        lefts.sort_by(f32::total_cmp);
        lefts.dedup();
        assert!(lefts.len() <= 2, "{width}: Browse buttons at {lefts:?}");
    }
}
