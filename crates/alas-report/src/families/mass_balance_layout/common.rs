// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;

pub(super) const BAR_HEIGHT: f64 = 0.5;

/// A titled canvas carrying centered placeholder text instead of a diagram,
/// for a figure whose governing datum is missing.
///
/// The same convention `mass_balance`'s `no_data_scene` uses: the figure
/// states what it does not have rather than drawing a substitute for it.
pub(super) fn missing_datum_scene(
    width: f64,
    height: f64,
    title: &str,
    pal: &Palette,
    message: &str,
) -> Scene {
    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [width * 0.5, height * 0.5],
        font_size: 12.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
pub(super) use crate::families::common::assign_label_rows;

pub(super) fn draw_hbar(
    scene: &mut Scene,
    axes: &Axes2D,
    row: f64,
    left: f64,
    val: f64,
    fill: Color,
) {
    let p_tl = axes.map_point(left, row + BAR_HEIGHT / 2.0);
    let p_br = axes.map_point(left + val, row - BAR_HEIGHT / 2.0);
    scene.add(SceneElement::Rect {
        x: p_tl[0],
        y: p_tl[1],
        width: (p_br[0] - p_tl[0]).max(0.5),
        height: p_br[1] - p_tl[1],
        rx: 0.0,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(Color::rgb(255, 255, 255), 1.0)),
    });
}

/// Label ink for a bar segment: white or near-black, whichever contrasts more
/// with the segment `fill`, so a light neutral bar is as legible as a
/// saturated one.
pub(super) fn bar_label_ink(fill: Color) -> Color {
    let white = Color::rgb(255, 255, 255);
    let dark = Color::rgb(24, 24, 24);
    if white.contrast_against(fill) >= dark.contrast_against(fill) {
        white
    } else {
        dark
    }
}

/// Two centered, stacked text lines (name + value) inside a bar segment
/// filled with `fill`.
// Bar geometry, two text lines, font and fill are all independent inputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn bar_label(
    scene: &mut Scene,
    axes: &Axes2D,
    cx: f64,
    row: f64,
    line1: &str,
    line2: &str,
    font: f64,
    fill: Color,
) {
    let center = axes.map_point(cx, row);
    let ink = bar_label_ink(fill);
    // Font sizes are points, the scene is in pixels: the two lines are one rendered line height
    // apart, centered on the bar, so they cannot touch at any font size.
    let pitch = font * crate::scene::CSS_PIXELS_PER_POINT * 1.2;
    scene.add(SceneElement::Text {
        text: line1.to_owned(),
        pos: [center[0], center[1] - pitch * 0.5],
        font_size: font,
        color: ink,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: line2.to_owned(),
        pos: [center[0], center[1] + pitch * 0.5],
        font_size: font,
        color: ink,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
}
