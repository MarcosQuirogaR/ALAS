// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Marks painted over a rendered view (a viewport marker, a pick ring): the
//! accent drawn over an outline of the colour it contrasts most with, so a
//! mark reads on whatever the render puts under it, light or dark.

use egui::{Color32, Painter, Pos2, Stroke};

use super::colour::contrast_ratio;

/// Width the outline shows on each side of a mark's stroke, points.
const OUTLINE: f32 = 1.5;

/// One stroke of a mark.
#[derive(Clone, Copy, Debug)]
pub enum Part {
    /// A circle: its centre, radius and stroke width, points.
    Ring(Pos2, f32, f32),
    /// A straight line: its two ends and stroke width, points.
    Line([Pos2; 2], f32),
}

/// Paint `parts` in `accent` over their outline, both at `opacity` (0 to 1).
/// Every outline goes down first, so where two parts cross the accent of
/// both stays whole.
pub fn paint(painter: &Painter, accent: Color32, opacity: f32, parts: &[Part]) {
    let opacity = opacity.clamp(0.0, 1.0);
    let outline =
        if contrast_ratio(accent, Color32::BLACK) >= contrast_ratio(accent, Color32::WHITE) {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
    let layers = [
        (outline.gamma_multiply(0.85 * opacity), 2.0 * OUTLINE),
        (accent.gamma_multiply(opacity), 0.0),
    ];
    for (colour, extra) in layers {
        for part in parts {
            match *part {
                Part::Ring(centre, radius, width) => {
                    painter.circle_stroke(centre, radius, Stroke::new(width + extra, colour));
                }
                Part::Line(ends, width) => {
                    painter.line_segment(ends, Stroke::new(width + extra, colour));
                }
            }
        }
    }
}
