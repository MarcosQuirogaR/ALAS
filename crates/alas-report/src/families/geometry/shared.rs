// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`_draw_planform`)
// Reference: alas @ rust-port-baseline.

//! Helpers shared by every `geometry` figure: the top-view planform outline
//! (`_draw_planform` upstream), and an equal-aspect axis-range fitter that
//! substitutes for Matplotlib's `ax.set_aspect("equal")`: [`crate::scene::Axes2D`]
//! maps its X and Y ranges independently onto a fixed pixel box and has no
//! notion of an equal data-to-pixel scale, so every multi-panel geometry
//! figure computes one here instead of leaving the aircraft visibly
//! stretched.

use alas_geom::aircraft::airplane::Airplane;

use crate::scene::{Color, Fill, Point2D, Scene, SceneElement, Stroke};

pub(super) use crate::families::common::equal_aspect_ranges;

/// Data-space bounding box of every wing planform and fuselage silhouette on
/// `plane`, mirrored across `y = 0` where a wing is symmetric: used to size
/// every geometry figure's axes from the real aircraft rather than a fixed
/// magic-number range.
///
/// Returns `(x_min, x_max, y_min, y_max, z_min, z_max)`. Wings contribute
/// their leading- and trailing-edge corners; fuselages contribute
/// `xyz_c +/- width/2` in Y and `xyz_c +/- height/2` in Z, matching how
/// [`super::planform::figure_geometry`] fills their silhouettes.
pub(super) fn airplane_bbox(plane: &Airplane) -> (f64, f64, f64, f64, f64, f64) {
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut z_min, mut z_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut grow = |x: f64, y: f64, z: f64| {
        x_min = x_min.min(x);
        x_max = x_max.max(x);
        y_min = y_min.min(y);
        y_max = y_max.max(y);
        z_min = z_min.min(z);
        z_max = z_max.max(z);
    };

    for wing in &plane.wings {
        for sec in &wing.xsecs {
            let [lx, ly, lz] = sec.xyz_le;
            grow(lx, ly, lz);
            grow(lx + sec.chord, ly, lz);
            if wing.symmetric {
                grow(lx, -ly, lz);
                grow(lx + sec.chord, -ly, lz);
            }
        }
    }
    for fus in &plane.fuselages {
        for sec in &fus.xsecs {
            let [cx, cy, cz] = sec.xyz_c;
            grow(cx, cy - sec.width / 2.0, cz - sec.height / 2.0);
            grow(cx, cy + sec.width / 2.0, cz + sec.height / 2.0);
        }
    }

    if !x_min.is_finite() {
        return (0.0, 1.0, -1.0, 1.0, -1.0, 1.0);
    }
    (x_min, x_max, y_min, y_max, z_min, z_max)
}

/// Draw every wing's top-view planform outline on `plane`: `_draw_planform`.
///
/// `fill_alpha`, when set, fills each planform (used by the design-evolution
/// montage and baseline/optimized overlays); otherwise the outline is
/// stroked, dashed when `dashed`. `invert_y`, when true, negates the
/// longitudinal coordinate before mapping: the effect of Matplotlib's
/// `ax.invert_yaxis()`, which [`crate::scene::Axes2D`] has no flag for; the
/// caller's axes must have been built over the negated `(-x_hi, -x_lo)`
/// range to match.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_planform(
    scene: &mut Scene,
    axes: &crate::scene::Axes2D,
    plane: &Airplane,
    color: Color,
    stroke_width: f64,
    dashed: bool,
    fill_alpha: Option<f64>,
    invert_y: bool,
) {
    let map = |y: f64, x: f64| -> Point2D {
        if invert_y {
            axes.map_point(y, -x)
        } else {
            axes.map_point(y, x)
        }
    };

    for wing in &plane.wings {
        if wing.xsecs.len() < 2 {
            continue;
        }
        let le: Vec<(f64, f64)> = wing
            .xsecs
            .iter()
            .map(|s| (s.xyz_le[1], s.xyz_le[0]))
            .collect();
        let te: Vec<(f64, f64)> = wing
            .xsecs
            .iter()
            .map(|s| (s.xyz_le[1], s.xyz_le[0] + s.chord))
            .collect();

        let sides: &[f64] = if wing.symmetric { &[1.0, -1.0] } else { &[1.0] };
        for &side in sides {
            let mut pts: Vec<Point2D> = le.iter().map(|&(y, x)| map(y * side, x)).collect();
            pts.extend(te.iter().rev().map(|&(y, x)| map(y * side, x)));

            if let Some(alpha) = fill_alpha {
                let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                scene.add(SceneElement::Polygon {
                    points: pts,
                    fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, a))),
                    stroke: None,
                });
            } else {
                let mut closed = pts.clone();
                closed.push(pts[0]);
                let stroke = if dashed {
                    Stroke::dashed(color, stroke_width, 6.0, 4.0)
                } else {
                    Stroke::new(color, stroke_width)
                };
                scene.add(SceneElement::Polyline {
                    points: closed,
                    stroke,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_aspect_matches_pixel_scale_in_both_directions() {
        let ((x_lo, x_hi), (y_lo, y_hi)) =
            equal_aspect_ranges(0.0, 10.0, 400.0, -5.0, 5.0, 200.0, 0.0);
        let scale_x = 400.0 / (x_hi - x_lo);
        let scale_y = 200.0 / (y_hi - y_lo);
        assert!((scale_x - scale_y).abs() < 1e-9);
    }

    #[test]
    fn bbox_of_an_empty_airplane_is_a_finite_fallback_not_infinities() {
        let plane = Airplane {
            name: "empty".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let (x_min, x_max, y_min, y_max, z_min, z_max) = airplane_bbox(&plane);
        for v in [x_min, x_max, y_min, y_max, z_min, z_max] {
            assert!(v.is_finite());
        }
    }
}
