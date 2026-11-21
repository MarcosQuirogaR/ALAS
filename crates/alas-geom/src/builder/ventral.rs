// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ventral fin pair under the tail cone, from drawn stations.
//!
//! Each section joins its drawn leading and trailing edge points. The root station is
//! held inside the body, so part of the surface lies in it: only what stands outside is
//! skin. The builder measures that on the built body, sampling each panel's surface
//! against the body's half-width at the station and height of every sample, and keeps as
//! the surface's reference planform the sections outboard of a cut placed where the plan
//! inboard of it equals the plan inside the body. Wetted area, drag and paint read that
//! planform ([`Wing::skin_planform`]); exports and renders show the built sections.
//!
//! Frame: x metres aft of the nose tip, y to starboard, z up, aircraft axes.

use alas_config::DesignVector;

use super::{tail_attachment, AircraftBuilder, BuildError};
use crate::aircraft::fuselage::Fuselage;
use crate::aircraft::wing::{Wing, WingXSec, VENTRAL_FIN_NAME};

/// Section of a ventral fin: a thin plate on the drawings, thick enough to read in a view.
const VENTRAL_SECTION: &str = "naca0004";

/// Samples along the chord and along the span of each panel in the exposure integral:
/// a 1/48 strip of a fin panel is a few centimetres, under the drawing's reading error.
const EXPOSURE_SAMPLES: usize = 48;

/// Chord of a level cut through a single corner of a drawn outline: a pointed tip drawn
/// to a line's width, under the drawing's reading error.
const POINTED_TIP_CHORD_M: f64 = 0.02;

/// Bisection steps for the cut: 2^-40 of a panel span.
const CUT_BISECTION_STEPS: usize = 40;

impl AircraftBuilder {
    /// The ventral fin pair of `dv` from its drawn stations under `body`, or `None`
    /// without two of them. Each section of a pair joins its leading and trailing edge
    /// points: rotating the chord by its twist about the local span keeps the drawn run
    /// in the plane of symmetry to within the cosine of the fin's cant. Stations all at
    /// y 0 draw one fin on the centreline from level cuts of the drawn side outline.
    pub(super) fn ventral_fin(
        &self,
        dv: &DesignVector,
        body: &Fuselage,
    ) -> Result<Option<Wing>, BuildError> {
        let g = &self.geometry.empennage;
        if g.ventral_fin_stations.len() < 2 {
            return Ok(None);
        }
        let x_fin = tail_attachment::vstab_x(g, dv);
        let section = Self::resolve(VENTRAL_SECTION)?;
        // Stations all on the plane of symmetry draw one fin on the centreline; any
        // other stations draw a mirrored pair.
        let centreline = g
            .ventral_fin_stations
            .iter()
            .all(|station| station.y_m == 0.0);
        let sections = if centreline {
            // A section of a surface on the plane of symmetry lies across its span, here
            // the z axis, so its chord is level: the drawn side outline is cut at the
            // height of each of its corners from the root leading edge down.
            let outline: Vec<[f64; 2]> = g
                .ventral_fin_stations
                .iter()
                .map(|station| [station.le_x_m, station.le_z_m])
                .chain(
                    g.ventral_fin_stations
                        .iter()
                        .rev()
                        .map(|station| [station.te_x_m, station.te_z_m]),
                )
                .collect();
            level_cuts(&outline)
                .into_iter()
                .map(|(x_le, z, chord)| {
                    WingXSec::new(
                        [x_fin + x_le, 0.0, g.vstab_z_m + z],
                        chord,
                        0.0,
                        section.clone(),
                    )
                })
                .collect()
        } else {
            g.ventral_fin_stations
                .iter()
                .map(|station| {
                    let (dx, dz) = (
                        station.te_x_m - station.le_x_m,
                        station.te_z_m - station.le_z_m,
                    );
                    WingXSec::new(
                        [
                            x_fin + station.le_x_m,
                            station.y_m,
                            g.vstab_z_m + station.le_z_m,
                        ],
                        dx.hypot(dz),
                        (-dz).atan2(dx).to_degrees(),
                        section.clone(),
                    )
                })
                .collect()
        };
        let mut wing = Wing::new(VENTRAL_FIN_NAME, sections, !centreline);
        wing.reference_xsecs = exposed_planform(&wing, body);
        Ok(Some(wing))
    }
}

/// Level cuts `(x of the leading edge, z, chord)` of the closed side `outline` (x, z),
/// listed as the leading edge points from the root followed by the trailing edge points
/// from the tip: one at the height of each corner. The drawn root runs along the body, so
/// it is not level: the root cut is at the higher of its two ends, the lower end's edge
/// carried up to it, which keeps the whole root inside the body the drawn root lay in. A
/// cut through a single corner keeps [`POINTED_TIP_CHORD_M`].
fn level_cuts(outline: &[[f64; 2]]) -> Vec<(f64, f64, f64)> {
    let mut outline = outline.to_vec();
    let last = outline.len() - 1;
    let top = outline[0][1].max(outline[last][1]);
    let carry = |end: [f64; 2], next: [f64; 2]| {
        if (end[1] - top).abs() < 1e-12 || (end[1] - next[1]).abs() < 1e-12 {
            end
        } else {
            [
                end[0] + (top - end[1]) / (end[1] - next[1]) * (end[0] - next[0]),
                top,
            ]
        }
    };
    outline[0] = carry(outline[0], outline[1]);
    outline[last] = carry(outline[last], outline[last - 1]);
    let mut levels: Vec<f64> = outline.iter().map(|point| point[1]).collect();
    levels.sort_by(|a, b| b.total_cmp(a));
    levels.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    levels
        .into_iter()
        .map(|z| {
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for (index, a) in outline.iter().enumerate() {
                let b = outline[(index + 1) % outline.len()];
                let (z_min, z_max) = (a[1].min(b[1]), a[1].max(b[1]));
                if z < z_min - 1e-9 || z > z_max + 1e-9 {
                    continue;
                }
                let xs = if (b[1] - a[1]).abs() < 1e-12 {
                    [a[0], b[0]]
                } else {
                    let x = a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                    [x, x]
                };
                for x in xs {
                    low = low.min(x);
                    high = high.max(x);
                }
            }
            let chord = (high - low).max(POINTED_TIP_CHORD_M);
            (low, z, chord)
        })
        .collect()
}

/// The share of each panel's surface (one side) that lies inside `body`.
pub(super) fn buried_shares(wing: &Wing, body: &Fuselage) -> Vec<f64> {
    let n = EXPOSURE_SAMPLES;
    let edges = |index: usize| {
        (
            wing.xyz_of_xsec(index, 0.0, 0.0),
            wing.xyz_of_xsec(index, 1.0, 0.0),
        )
    };
    (0..wing.xsecs.len().saturating_sub(1))
        .map(|index| {
            let ((le_a, te_a), (le_b, te_b)) = (edges(index), edges(index + 1));
            let (mut total, mut buried) = (0.0, 0.0);
            for i in 0..n {
                let u = (i as f64 + 0.5) / n as f64;
                let on_a = lerp3(le_a, te_a, u);
                let on_b = lerp3(le_b, te_b, u);
                let span = sub3(on_b, on_a);
                for k in 0..n {
                    let v = (k as f64 + 0.5) / n as f64;
                    let chord = lerp3(sub3(te_a, le_a), sub3(te_b, le_b), v);
                    let area = norm3(cross3(chord, span));
                    let point = lerp3(on_a, on_b, v);
                    total += area;
                    if point[1].abs() < body.half_width_at(point[0], point[2]) {
                        buried += area;
                    }
                }
            }
            if total > 0.0 {
                buried / total
            } else {
                0.0
            }
        })
        .collect()
}

/// The reference planform outboard of the cut whose inboard plan equals the plan inside
/// `body`, or `None` when none of the surface is inside it.
fn exposed_planform(wing: &Wing, body: &Fuselage) -> Option<Vec<WingXSec>> {
    let panel = |a: &WingXSec, b: &WingXSec, t: f64| {
        let span = (b.xyz_le[1] - a.xyz_le[1]).hypot(b.xyz_le[2] - a.xyz_le[2]);
        span * t * (a.chord + (a.chord + t * (b.chord - a.chord))) / 2.0
    };
    let shares = buried_shares(wing, body);
    let mut buried: f64 = wing
        .xsecs
        .windows(2)
        .zip(&shares)
        .map(|(pair, share)| share * panel(&pair[0], &pair[1], 1.0))
        .sum();
    if buried <= 0.0 {
        return None;
    }
    for (index, pair) in wing.xsecs.windows(2).enumerate() {
        let (a, b) = (&pair[0], &pair[1]);
        let whole = panel(a, b, 1.0);
        if buried >= whole && index + 2 < wing.xsecs.len() {
            buried -= whole;
            continue;
        }
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..CUT_BISECTION_STEPS {
            let mid = 0.5 * (low + high);
            if panel(a, b, mid) < buried {
                low = mid;
            } else {
                high = mid;
            }
        }
        let t = 0.5 * (low + high);
        let cut = WingXSec::new(
            lerp3(a.xyz_le, b.xyz_le, t),
            a.chord + t * (b.chord - a.chord),
            a.twist + t * (b.twist - a.twist),
            a.airfoil.clone(),
        );
        return Some(
            std::iter::once(cut)
                .chain(wing.xsecs[index + 1..].iter().cloned())
                .collect(),
        );
    }
    None
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
