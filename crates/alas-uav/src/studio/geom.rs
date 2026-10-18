// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stations on a planform chain. Positions are body axes, m.

use crate::airframe::geometry::Planform;
use crate::airframe::inertia::Vec3;

/// A point on the leading edge with its local chord.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Leading-edge position, m.
    pub le_m: Vec3,
    /// Local chord, m.
    pub chord_m: f64,
}

impl Station {
    /// Point at chord fraction `f` (0 leading edge, 1 trailing edge), on the
    /// chord line (no twist or camber), m.
    pub fn at_chord_fraction(&self, f: f64) -> Vec3 {
        [self.le_m[0] + f * self.chord_m, self.le_m[1], self.le_m[2]]
    }
}

/// Station at distance `s` (m) from the root along the span axis: `y` for a
/// horizontal planform (projected), `z` for a fin. `None` outside the chain.
pub fn station(pf: &Planform, s: f64) -> Option<Station> {
    let mut acc = 0.0;
    for (i, p) in pf.panels.iter().enumerate() {
        let last = i + 1 == pf.panels.len();
        if s <= acc + p.span_m + 1e-12 || last {
            let local = (s - acc).clamp(0.0, p.span_m);
            if s > acc + p.span_m + 1e-9 {
                return None;
            }
            let chord = p.chord_root_m + (p.chord_tip_m - p.chord_root_m) * local / p.span_m;
            let x = p.root_le_m[0] + local * p.sweep_le_rad.tan();
            let le = if pf.vertical {
                [x, p.root_le_m[1], p.root_le_m[2] + local]
            } else {
                [
                    x,
                    p.root_le_m[1] + local,
                    p.root_le_m[2] + local * p.dihedral_rad.tan(),
                ]
            };
            return Some(Station {
                le_m: le,
                chord_m: chord,
            });
        }
        acc += p.span_m;
    }
    None
}

/// Half-extent of the chain along the span axis (half span of a mirrored
/// horizontal surface, height of a fin), m.
pub fn extent(pf: &Planform) -> f64 {
    pf.panels.iter().map(|p| p.span_m).sum()
}

/// Axis-aligned box of a point cloud: `(min, max)`.
pub fn bounds(points: &[Vec3]) -> Option<(Vec3, Vec3)> {
    let first = *points.first()?;
    Some(points.iter().fold((first, first), |(lo, hi), p| {
        (
            [lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])],
            [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])],
        )
    }))
}
