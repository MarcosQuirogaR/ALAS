// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Payload page rendered with the software rasteriser (no GPU) at the sizes of the review's
//! screenshots, side and front, for a look and for the layout rules: no text over the airframe,
//! no page taller than its viewport, the seats drawn at their true size.
//!
//! Sizes are the content area of the page (the window less the rail and, at D, the 3-D preview),
//! in points: A 1100x700, B 1750x1000, C 2580x1000, D 560x1000, E 560x540.

use super::*;
use crate::test_raster::{render, write, Shot};

/// The review's sizes: name, width, height.
const SIZES: [(&str, f32, f32); 5] = [
    ("A", 1_100.0, 700.0),
    ("B", 1_750.0, 1_000.0),
    ("C", 2_580.0, 1_000.0),
    ("D", 560.0, 1_000.0),
    ("E", 560.0, 540.0),
];

/// The page in `mode` at `width` x `height`, after the airframe render has landed.
fn page(mode: ViewMode, fuselage: bool, width: f32, height: f32) -> (Shot, AppState) {
    let mut state = AppState::default();
    let mut first = true;
    let shot = render(width, height, 14, 160, |ui| {
        if first {
            let mut editor: EditorUi = ui
                .ctx()
                .data_mut(|data| data.get_temp(ui_id()))
                .unwrap_or_default();
            editor.view_mode = mode;
            editor.front = super::front_camera::Camera {
                fuselage,
                ..Default::default()
            };
            ui.ctx().data_mut(|data| data.insert_temp(ui_id(), editor));
            first = false;
        }
        egui::ScrollArea::vertical().show(ui, |ui| show(&mut state, ui));
    });
    (shot, state)
}

#[test]
fn the_side_and_front_pages_are_drawn_at_the_review_sizes() {
    for (name, width, height) in SIZES {
        for (mode, fuselage, label) in [
            (ViewMode::Side, false, "side"),
            (ViewMode::Front, false, "front"),
            (ViewMode::Front, true, "front-fuselage"),
        ] {
            let (shot, _) = page(mode, fuselage, width, height);
            write(&format!("payload-{label}-{name}.png"), &shot);
        }
    }
}

#[test]
fn the_footer_note_stays_inside_the_card_at_every_size() {
    for (name, width, height) in SIZES {
        for mode in [ViewMode::Side, ViewMode::Front] {
            let (shot, _) = page(mode, false, width, height);
            for (text, rect) in &shot.texts {
                if text.starts_with("Seats and fixed masses") {
                    assert!(
                        rect.right() <= width - 14.0,
                        "{name}: the footer {rect:?} runs past the card (page {width} pt wide)"
                    );
                }
            }
        }
    }
}
