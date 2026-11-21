// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Station-label placement for the T-s preview.

use crate::scene::{Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};

/// Puts station labels beside their dots without covering a dot, another
/// label, a drawn curve or leaving the plot.
pub(super) struct LabelPlacer {
    plot: (f64, f64, f64, f64),
    dots: Vec<[f64; 2]>,
    placed: Vec<[f64; 4]>,
    curves: Vec<[[f64; 2]; 2]>,
}

/// Whether the segment `a`-`b` passes through (or lies in) `rect` = `[x0, y0, x1, y1]`
/// (Liang-Barsky clipping).
pub(super) fn segment_crosses(a: [f64; 2], b: [f64; 2], rect: [f64; 4]) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a[0] - rect[0]),
        (dx, rect[2] - a[0]),
        (-dy, a[1] - rect[1]),
        (dy, rect[3] - a[1]),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

impl LabelPlacer {
    pub(super) fn new(plot: (f64, f64, f64, f64), dots: Vec<[f64; 2]>) -> Self {
        Self {
            plot,
            dots,
            placed: Vec::new(),
            curves: Vec::new(),
        }
    }

    /// Treat the polylines (in canvas pixels) as obstacles a label must not be struck through by.
    pub(super) fn with_curves<'a>(
        mut self,
        polylines: impl Iterator<Item = &'a [[f64; 2]]>,
    ) -> Self {
        for points in polylines {
            self.curves
                .extend(points.windows(2).map(|pair| [pair[0], pair[1]]));
        }
        self
    }

    /// Draw `text` for the dot at `anchor`. `prefer` is the side (`true` =
    /// right of the dot) and the vertical offset tried first.
    pub(super) fn add(
        &mut self,
        scene: &mut Scene,
        text: &str,
        anchor: [f64; 2],
        prefer: (bool, f64),
        color: &str,
        size: f64,
    ) {
        let px = crate::scene::CSS_PIXELS_PER_POINT;
        let (width, height) = (
            text.chars().count() as f64 * size * px * crate::scene::CONSERVATIVE_ADVANCE_EM,
            size * px * crate::scene::TEXT_LINE_HEIGHT_EM,
        );
        let rect_for = |right: bool, dy: f64, gap: f64| {
            let x = if right {
                anchor[0] + gap
            } else {
                anchor[0] - gap - width
            };
            [
                x,
                anchor[1] + dy - height / 2.0,
                x + width,
                anchor[1] + dy + height / 2.0,
            ]
        };
        let (left, top, plot_w, plot_h) = self.plot;
        // A rectangle is allowed if it stays in the plot and clear of the placed labels and dots;
        // among those, the fewer curve segments run through it the better.
        let allowed = |rect: [f64; 4]| {
            rect[0] >= left
                && rect[2] <= left + plot_w
                && rect[1] >= top
                && rect[3] <= top + plot_h
                && !self.placed.iter().any(|other| {
                    rect[0] < other[2]
                        && other[0] < rect[2]
                        && rect[1] < other[3]
                        && other[1] < rect[3]
                })
                && !self.dots.iter().any(|dot| {
                    dot[0] > rect[0] - 7.0
                        && dot[0] < rect[2] + 7.0
                        && dot[1] > rect[1] - 7.0
                        && dot[1] < rect[3] + 7.0
                })
        };
        let curve_hits = |rect: [f64; 4]| {
            let padded = [rect[0] - 1.5, rect[1] - 1.5, rect[2] + 1.5, rect[3] + 1.5];
            self.curves
                .iter()
                .filter(|segment| segment_crosses(segment[0], segment[1], padded))
                .count()
        };
        let step = height + 2.0;
        // Nearest gap first, then a wider one that clears a curve running beside the dot.
        let candidates: Vec<(bool, [f64; 4])> = [10.0, 26.0]
            .into_iter()
            .flat_map(|gap| {
                (0..7).flat_map(move |k| {
                    let shift = k as f64 * step;
                    [
                        (prefer.0, prefer.1 - shift),
                        (prefer.0, prefer.1 + shift),
                        (!prefer.0, prefer.1 - shift),
                        (!prefer.0, prefer.1 + shift),
                    ]
                    .into_iter()
                    .map(move |(right, dy)| (right, dy, gap))
                })
            })
            .map(|(right, dy, gap)| (right, rect_for(right, dy, gap)))
            .collect();
        let rect = candidates
            .iter()
            .find(|(_, rect)| allowed(*rect) && curve_hits(*rect) == 0)
            .or_else(|| {
                candidates
                    .iter()
                    .filter(|(_, rect)| allowed(*rect))
                    .min_by_key(|(_, rect)| curve_hits(*rect))
            })
            .copied()
            .unwrap_or((prefer.0, rect_for(prefer.0, prefer.1, 10.0)));
        self.placed.push(rect.1);
        // A label that could not stand beside its own dot (the dot is packed
        // among others, or the free slot is further up or down the curve) is
        // joined to it by a thin leader, so it never reads as the name of a
        // neighbouring dot.
        let label_y = (rect.1[1] + rect.1[3]) / 2.0;
        let edge_x = if rect.0 {
            rect.1[0] - 2.0
        } else {
            rect.1[2] + 2.0
        };
        let beside =
            (label_y - anchor[1]).abs() <= height * 0.6 && (edge_x - anchor[0]).abs() <= 14.0;
        if !beside {
            let (dx, dy) = (edge_x - anchor[0], label_y - anchor[1]);
            let length = dx.hypot(dy).max(1.0);
            scene.add(SceneElement::Line {
                p1: [anchor[0] + dx / length * 3.5, anchor[1] + dy / length * 3.5],
                p2: [edge_x, label_y],
                stroke: Stroke::new(Color::from_hex(color), 0.6),
            });
        }
        scene.add(SceneElement::Text {
            text: text.into(),
            pos: [
                if rect.0 { rect.1[0] } else { rect.1[2] },
                (rect.1[1] + rect.1[3]) / 2.0,
            ],
            font_size: size,
            color: Color::from_hex(color),
            align: if rect.0 {
                TextAlign::Left
            } else {
                TextAlign::Right
            },
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

pub(super) fn label(scene: &mut Scene, text: &str, pos: [f64; 2], color: &str, size: f64) {
    scene.add(SceneElement::Text {
        text: text.into(),
        pos,
        font_size: size,
        color: Color::from_hex(color),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(scene: &Scene) -> usize {
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Line { .. }))
            .count()
    }

    #[test]
    fn a_label_beside_its_dot_has_no_leader() {
        let mut scene = Scene::new(400.0, 300.0, None);
        let mut placer = LabelPlacer::new((20.0, 20.0, 360.0, 260.0), vec![[200.0, 150.0]]);
        placer.add(
            &mut scene,
            "4",
            [200.0, 150.0],
            (true, 0.0),
            "#ffffff",
            11.0,
        );
        assert_eq!(lines(&scene), 0);
    }

    #[test]
    fn a_label_pushed_away_from_a_packed_dot_is_joined_to_it_by_a_leader() {
        let mut scene = Scene::new(400.0, 300.0, None);
        // Three stations on one spot (a duct that changes pressure, not
        // temperature): the first two labels take the right and left side, the
        // third can only stand above or below, away from its dot.
        let dot = [200.0, 150.0];
        let mut placer = LabelPlacer::new((20.0, 20.0, 360.0, 260.0), vec![dot, dot, dot]);
        for text in ["5 \u{b7} LPT", "6 \u{b7} Tailpipe", "9 \u{b7} Nozzle exit"] {
            placer.add(&mut scene, text, dot, (true, 0.0), "#ffffff", 11.0);
        }
        assert_eq!(lines(&scene), 1, "only the displaced label needs a leader");
    }
}
