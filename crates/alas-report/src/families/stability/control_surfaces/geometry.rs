// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (_le_chord_at_span,
// _span_stations, _cs_surface_patch, _cs_surface_area; L1664-1732)
// Reference: alas @ rust-port-baseline.

use crate::scene::{Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::WingXSec;
/// Interpolated `(leading_edge_x, chord)` at `span_val` along `xsecs`' span
/// axis (`span_idx`: 1 for a wing/h-stab's Y, 2 for a v-stab's Z):
/// `_le_chord_at_span`.
pub(super) fn le_chord_at_span(xsecs: &[WingXSec], span_val: f64, span_idx: usize) -> (f64, f64) {
    let vals: Vec<f64> = xsecs.iter().map(|xs| xs.xyz_le[span_idx]).collect();
    for i in 0..xsecs.len().saturating_sub(1) {
        let (v0, v1) = (vals[i], vals[i + 1]);
        let (lo, hi) = (v0.min(v1), v0.max(v1));
        if lo - 1e-9 <= span_val && span_val <= hi + 1e-9 {
            let f = if v1 != v0 {
                (span_val - v0) / (v1 - v0)
            } else {
                0.0
            };
            let x_le = xsecs[i].xyz_le[0] + f * (xsecs[i + 1].xyz_le[0] - xsecs[i].xyz_le[0]);
            let chord = xsecs[i].chord + f * (xsecs[i + 1].chord - xsecs[i].chord);
            return (x_le, chord);
        }
    }
    let last = xsecs.len() - 1;
    let edge = if (span_val - vals[last]).abs() < (span_val - vals[0]).abs() {
        &xsecs[last]
    } else {
        &xsecs[0]
    };
    (edge.xyz_le[0], edge.chord)
}

/// Span values to sample between `s0` and `s1`, including any xsec break
/// station strictly in between: `_span_stations`.
pub(super) fn span_stations(xsecs: &[WingXSec], span_idx: usize, s0: f64, s1: f64) -> Vec<f64> {
    let (lo, hi) = (s0.min(s1), s0.max(s1));
    let mut inner: Vec<f64> = xsecs
        .iter()
        .map(|xs| xs.xyz_le[span_idx])
        .filter(|&v| lo + 1e-9 < v && v < hi - 1e-9)
        .collect();
    inner.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut stations = Vec::with_capacity(inner.len() + 2);
    stations.push(lo);
    stations.append(&mut inner);
    stations.push(hi);
    if s0 > s1 {
        stations.reverse();
    }
    stations
}

/// Polygon (span, chordwise-x) points between chordwise fractions
/// `[frac_lo, frac_hi]` (0=LE, 1=TE) and span positions `s0..s1`:
/// `_cs_surface_patch`. The caller decides plot-axis order.
pub(super) fn cs_surface_patch(
    xsecs: &[WingXSec],
    span_idx: usize,
    s0: f64,
    s1: f64,
    frac_lo: f64,
    frac_hi: f64,
) -> Vec<(f64, f64)> {
    let stations = span_stations(xsecs, span_idx, s0, s1);
    let le_chord: Vec<(f64, f64)> = stations
        .iter()
        .map(|&s| le_chord_at_span(xsecs, s, span_idx))
        .collect();
    let mut poly: Vec<(f64, f64)> = stations
        .iter()
        .zip(&le_chord)
        .map(|(&s, &(x, c))| (s, x + c * frac_lo))
        .collect();
    let back = stations
        .iter()
        .zip(&le_chord)
        .map(|(&s, &(x, c))| (s, x + c * frac_hi));
    poly.extend(back.rev());
    poly
}

/// One control surface's true planform area, in m^2: `_cs_surface_area`.
/// See the module doc for why `figure_control_surfaces` computes but does
/// not display it, matching upstream.
pub(super) fn cs_surface_area(
    xsecs: &[WingXSec],
    span_idx: usize,
    s0: f64,
    s1: f64,
    frac_lo: f64,
    frac_hi: f64,
    mirror: bool,
) -> f64 {
    let (_, c0) = le_chord_at_span(xsecs, s0, span_idx);
    let (_, c1) = le_chord_at_span(xsecs, s1, span_idx);
    let chord_frac = frac_hi - frac_lo;
    let area = 0.5 * (c0 * chord_frac + c1 * chord_frac) * (s1 - s0).abs();
    if mirror {
        area * 2.0
    } else {
        area
    }
}

/// A top-view axes with the longitudinal (X) data axis negated on the way
/// in, so the nose reads at the top of the canvas, see the module doc.
pub(super) struct TopAxes(pub(super) Axes2D);

impl TopAxes {
    pub(super) fn new(
        rect: (f64, f64, f64, f64),
        y_range: (f64, f64),
        x_range: (f64, f64),
    ) -> Self {
        // The planform is a physical drawing: one metre in span must occupy
        // the same number of pixels as one metre longitudinally. Expanding
        // the shorter displayed range preserves that relationship without
        // cropping any of the aircraft geometry.
        Self(Axes2D::new(rect, y_range, (-x_range.1, -x_range.0)).with_equal_aspect())
    }

    pub(super) fn point(&self, span_y: f64, long_x: f64) -> Point2D {
        self.0.map_point(span_y, -long_x)
    }

    pub(super) fn polygon(&self, pts: &[(f64, f64)]) -> Vec<Point2D> {
        pts.iter().map(|&(y, x)| self.point(y, x)).collect()
    }
}

/// The top-view background: every wing's planform outline, filled, mirrored
/// if symmetric: `_draw_planform` at `fill=True`.
pub(super) fn draw_planform_fill(
    scene: &mut Scene,
    axes: &TopAxes,
    plane: &Airplane,
    color: Color,
) {
    for wing in &plane.wings {
        let mut poly: Vec<(f64, f64)> = wing
            .xsecs
            .iter()
            .map(|xs| (xs.xyz_le[1], xs.xyz_le[0]))
            .collect();
        poly.extend(
            wing.xsecs
                .iter()
                .rev()
                .map(|xs| (xs.xyz_le[1], xs.xyz_le[0] + xs.chord)),
        );
        let sides: &[f64] = if wing.symmetric { &[1.0, -1.0] } else { &[1.0] };
        for &sign in sides {
            let pts: Vec<(f64, f64)> = poly.iter().map(|&(y, x)| (y * sign, x)).collect();
            scene.add(SceneElement::Polygon {
                points: axes.polygon(&pts),
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
    }
}

/// Draw one control-surface patch (mirrored if `mirror`) and register its
/// legend entry if not already present: the body of `add_top_patch`.
///
/// Eleven parameters: the same list upstream's own `add_top_patch` closes
/// over (span/chord bounds, styling, and the mutable scene/legend state it
/// writes into). A struct would only relocate this list, not shorten it, and
/// every call site below spells every argument out, so there is nowhere for
/// a mismatched field to hide silently.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_top_patch(
    scene: &mut Scene,
    axes: &TopAxes,
    legend: &mut Vec<(String, Color)>,
    xsecs: &[WingXSec],
    span_lo: f64,
    span_hi: f64,
    frac_lo: f64,
    frac_hi: f64,
    color_hex: &str,
    name: &str,
    mirror: bool,
) {
    let poly = cs_surface_patch(xsecs, 1, span_lo, span_hi, frac_lo, frac_hi);
    let color = Color::from_hex(color_hex);
    let edge = Stroke::new(Color::rgb(0, 0, 0), 0.6);
    let sides: &[f64] = if mirror { &[1.0, -1.0] } else { &[1.0] };
    for &sign in sides {
        let pts: Vec<(f64, f64)> = poly.iter().map(|&(s, x)| (s * sign, x)).collect();
        scene.add(SceneElement::Polygon {
            points: axes.polygon(&pts),
            fill: Some(Fill::new(color)),
            stroke: Some(edge.clone()),
        });
    }
    if !legend.iter().any(|(n, _)| n == name) {
        legend.push((name.to_owned(), color));
    }
}

/// `Vh`/`Vv` info line: `"<name> = <val> (target lo-hi) [OK|OUT OF RANGE]"`,
/// or `"<name>: n/a"`: `_fmt`.
pub(super) fn fmt_volume_coef(name: &str, val: Option<f64>, lo: f64, hi: f64) -> String {
    match val {
        None => format!("{name}: n/a"),
        Some(v) => {
            let mark = if lo <= v && v <= hi {
                "OK"
            } else {
                "OUT OF RANGE"
            };
            format!("{name} = {v:.3}  (target {lo:.2}-{hi:.2})  [{mark}]")
        }
    }
}

/// The `(y_min, y_max, x_min, x_max)` bounding box of every wing's planform
/// (span Y, mirrored if symmetric; longitudinal X, leading edge to trailing
/// edge), with a fixed margin: the data `_draw_planform`'s loop covers,
/// read once up front to size the top-view axes.
pub(super) fn planform_bounds(plane: &Airplane) -> (f64, f64, f64, f64) {
    let (mut y_min, mut y_max) = (0.0f64, 0.0f64);
    let (mut x_min, mut x_max) = (0.0f64, 0.0f64);
    for wing in &plane.wings {
        for xs in &wing.xsecs {
            let y = xs.xyz_le[1];
            let x0 = xs.xyz_le[0];
            let x1 = x0 + xs.chord;
            x_min = x_min.min(x0);
            x_max = x_max.max(x1);
            let ys = if wing.symmetric { [y, -y] } else { [y, y] };
            for v in ys {
                y_min = y_min.min(v);
                y_max = y_max.max(v);
            }
        }
    }
    (y_min - 2.0, y_max + 2.0, x_min - 2.0, x_max + 2.0)
}
