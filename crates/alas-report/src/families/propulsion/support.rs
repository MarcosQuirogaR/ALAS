// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared drawing helpers kept separate so the three propulsion sweep
//! implementations stay below the repository's source-size limit.

use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};

/// Common X/Y axis-label placement shared by the carpet plot and the
/// efficiency-decomposition figure.
pub(super) fn axis_labels(
    scene: &mut Scene,
    pal: &crate::theme::Palette,
    rect: (f64, f64, f64, f64),
    x_label: &str,
    y_label: &str,
) {
    let (x, y, w, h) = rect;
    scene.add(SceneElement::Text {
        text: x_label.to_owned(),
        // Leave a complete glyph height below the label on the 420 px
        // efficiency card; 26 px put its descenders against the SVG edge.
        pos: [x + w * 0.5, y + h + 18.0],
        font_size: 9.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: y_label.to_owned(),
        pos: [x - 45.0, y + h * 0.5],
        font_size: 9.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}
