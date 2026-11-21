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

/// Top-left origin for a legend box of `rows` rows (see `draw_legend`: row pitch
/// `font_size + 6`) that overlaps the fewest plotted samples. The four corners of
/// `rect` are tried in the order top-left, top-right, bottom-left, bottom-right
/// and the first of the least-cluttered wins, so an uncluttered figure keeps its
/// conventional top-left legend. `lines_px` are polylines in scene coordinates;
/// each segment is sampled every 3 px so a long straight run still counts.
pub(super) fn least_cluttered_legend_origin(
    rect: (f64, f64, f64, f64),
    rows: usize,
    font_size: f64,
    box_width: f64,
    lines_px: &[Vec<[f64; 2]>],
) -> [f64; 2] {
    let (x, y, w, h) = rect;
    let box_h = rows as f64 * (font_size + 6.0) + 4.0;
    let margin = 10.0;
    let candidates = [
        [x + margin, y + margin],
        [x + w - margin - box_width, y + margin],
        [x + margin, y + h - margin - box_h],
        [x + w - margin - box_width, y + h - margin - box_h],
    ];
    let mut best = (usize::MAX, candidates[0]);
    for origin in candidates {
        let inside = |p: [f64; 2]| {
            p[0] >= origin[0] - 4.0
                && p[0] <= origin[0] + box_width + 4.0
                && p[1] >= origin[1] - 4.0
                && p[1] <= origin[1] + box_h + 4.0
        };
        let mut hits = 0_usize;
        for line in lines_px {
            for pair in line.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let steps = (length / 3.0).ceil().max(1.0) as usize;
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    if inside([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]) {
                        hits += 1;
                    }
                }
            }
        }
        if hits < best.0 {
            best = (hits, origin);
        }
    }
    best.1
}

/// A footnote wrapped to `width_px` and stacked upwards from `bottom_y`, one
/// text element per line, centred on `centre_x`; nothing is cut at the canvas edge.
pub(super) fn wrapped_footnote(
    scene: &mut Scene,
    pal: &crate::theme::Palette,
    text: &str,
    font_size: f64,
    centre_x: f64,
    bottom_y: f64,
    width_px: f64,
) {
    let text = crate::text_localizer::localize(text);
    let wrapped = crate::scene::wrap_text_to_width(&text, font_size, width_px);
    let lines: Vec<&str> = wrapped.lines().collect();
    let pitch = font_size * 1.55;
    for (i, line) in lines.iter().enumerate() {
        let from_bottom = (lines.len() - 1 - i) as f64;
        scene.add(SceneElement::Text {
            text: (*line).to_owned(),
            pos: [centre_x, bottom_y - from_bottom * pitch],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_moves_to_the_corner_without_data() {
        let rect = (0.0, 0.0, 400.0, 300.0);
        // A curve running along the top-left and bottom-left: the legend goes right.
        let curve = vec![vec![
            [0.0, 5.0],
            [150.0, 20.0],
            [0.0, 280.0],
            [150.0, 290.0],
        ]];
        let origin = least_cluttered_legend_origin(rect, 3, 8.0, 120.0, &curve);
        assert!(origin[0] > 200.0, "{origin:?}");
        // No data at all keeps the conventional top-left corner.
        assert_eq!(
            least_cluttered_legend_origin(rect, 3, 8.0, 120.0, &[]),
            [10.0, 10.0]
        );
    }
}
