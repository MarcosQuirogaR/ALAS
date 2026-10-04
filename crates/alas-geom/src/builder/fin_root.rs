// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the fin root meets the body.
//!
//! The configured fin is a straight-edged trapezoid whose root line,
//! `vstab_z_m`, is the line its drawn height and root chord are measured
//! from: usually the fuselage top line at the fin. A tailcone falls away
//! under the aft part of a root drawn at that height, which leaves the fin
//! floating above the body there. The builder therefore continues the
//! trapezoid's leading and trailing edges down, or trims them up, to the
//! **attachment height**: the height at which the root chord's lowest body
//! top equals the root itself. The root then touches the body at that
//! lowest point and lies inside it everywhere else, and the tip, which sets
//! the fin height, does not move.
//!
//! The body under the fin is the fuselage and any centreline nacelle (a
//! trijet's tail engine), whose top the fin stands on where it is the higher
//! surface. Frame: x metres aft of the nose tip, z metres up in the geometry
//! axes.

use alas_config::{DesignVector, WingShape};

use super::{AircraftBuilder, BuildError};
use crate::aircraft::fuselage::Fuselage;

/// Bisection steps for the attachment height: 2^-48 of a fin height is far
/// below a millimetre.
const BISECTION_STEPS: usize = 48;

impl AircraftBuilder {
    /// The seat of the fin on `fuselage` and on the centreline nacelles of
    /// design `dv`, which do not depend on the wing shape.
    pub(super) fn fin_seat(
        &self,
        dv: &DesignVector,
        fuselage: &Fuselage,
    ) -> Result<FinSeat, BuildError> {
        let positions = &self.geometry.engine.spanwise_positions_m;
        let nacelles = if positions.contains(&0.0) {
            self.build_engines(dv, WingShape::Ground)?
        } else {
            Vec::new()
        };
        let centreline = positions
            .iter()
            .zip(&nacelles)
            .filter(|(y, _)| **y == 0.0)
            .map(|(_, nacelle)| nacelle);
        Ok(FinSeat::new(std::iter::once(fuselage).chain(centreline)))
    }
}

/// The tops of the bodies on the plane of symmetry a fin can stand on.
pub(super) struct FinSeat {
    /// One `(x, z_top)` polyline per body, nose to tail.
    tops: Vec<Vec<[f64; 2]>>,
}

impl FinSeat {
    /// The seat formed by `bodies`.
    pub(super) fn new<'a>(bodies: impl IntoIterator<Item = &'a Fuselage>) -> Self {
        let tops = bodies
            .into_iter()
            .map(|body| {
                body.xsecs
                    .iter()
                    .map(|xsec| [xsec.xyz_c[0], xsec.xyz_c[2] + xsec.height / 2.0])
                    .collect::<Vec<_>>()
            })
            .filter(|top| top.len() >= 2)
            .collect();
        Self { tops }
    }

    /// The highest body top at `x_m`; `None` where no body lies under it.
    fn top_at(&self, x_m: f64) -> Option<f64> {
        self.tops
            .iter()
            .filter_map(|top| interpolate(top, x_m))
            .reduce(f64::max)
    }

    /// The lowest body top under `[x0_m, x1_m]`; `None` where no body lies
    /// under any of it.
    ///
    /// Every top is piecewise linear, so the lowest point of their upper
    /// envelope lies at an interval end, at a body station, where the tops
    /// of two bodies cross (a fuselage top falling aft over a nacelle top
    /// rising from its inlet), or just outside a body that starts or ends
    /// inside the interval, on the others (the fuselage top just ahead of a
    /// nacelle inlet): those candidates give it exactly.
    fn lowest_top(&self, x0_m: f64, x1_m: f64) -> Option<f64> {
        let inside = |x: &f64| *x > x0_m && *x < x1_m;
        let mut stations = vec![x0_m, x1_m];
        stations.extend(
            self.tops
                .iter()
                .flatten()
                .map(|point| point[0])
                .filter(inside),
        );
        for (index, first) in self.tops.iter().enumerate() {
            for second in &self.tops[index + 1..] {
                for a in first.windows(2) {
                    for b in second.windows(2) {
                        stations.extend(crossing(a, b).filter(inside));
                    }
                }
            }
        }
        let beside_ends = self.tops.iter().enumerate().flat_map(|(index, top)| {
            [top.first(), top.last()]
                .into_iter()
                .flatten()
                .map(|point| point[0])
                .filter(inside)
                .filter_map(move |x| self.top_without(x, index))
        });
        stations
            .into_iter()
            .filter_map(|x| self.top_at(x))
            .chain(beside_ends)
            .reduce(f64::min)
    }

    /// The highest top at `x_m` of every body but the one at `skipped`.
    fn top_without(&self, x_m: f64, skipped: usize) -> Option<f64> {
        self.tops
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != skipped)
            .filter_map(|(_, top)| interpolate(top, x_m))
            .reduce(f64::max)
    }

    /// The lowest point of any body top.
    fn lowest(&self) -> Option<f64> {
        self.tops
            .iter()
            .flatten()
            .map(|point| point[1])
            .reduce(f64::min)
    }
}

/// Where two top segments cross over their common stretch, if they do.
fn crossing(a: &[[f64; 2]], b: &[[f64; 2]]) -> Option<f64> {
    let (start, end) = (a[0][0].max(b[0][0]), a[1][0].min(b[1][0]));
    if start >= end {
        return None;
    }
    let gap = |x: f64| {
        interpolate(a, x)
            .zip(interpolate(b, x))
            .map(|(za, zb)| za - zb)
    };
    let (gap_start, gap_end) = (gap(start)?, gap(end)?);
    (gap_start * gap_end < 0.0).then(|| start + (end - start) * gap_start / (gap_start - gap_end))
}

/// Linear interpolation on a nose-to-tail polyline; `None` outside it.
fn interpolate(top: &[[f64; 2]], x_m: f64) -> Option<f64> {
    let (first, last) = (top.first()?, top.last()?);
    if !(first[0]..=last[0]).contains(&x_m) {
        return None;
    }
    top.windows(2).find(|pair| x_m <= pair[1][0]).map(|pair| {
        let span = pair[1][0] - pair[0][0];
        if span > 0.0 {
            pair[0][1] + (x_m - pair[0][0]) / span * (pair[1][1] - pair[0][1])
        } else {
            pair[0][1].max(pair[1][1])
        }
    })
}

/// A straight-edged fin in the aircraft frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FinTrapezoid {
    /// Root leading edge.
    pub root_le_m: [f64; 3],
    /// Root chord.
    pub root_chord_m: f64,
    /// Tip leading edge.
    pub tip_le_m: [f64; 3],
    /// Tip chord.
    pub tip_chord_m: f64,
}

impl FinTrapezoid {
    /// The section at height `z_m` on the straight edges, leading edge and
    /// chord.
    fn section_at(&self, z_m: f64) -> ([f64; 3], f64) {
        let s = (z_m - self.root_le_m[2]) / (self.tip_le_m[2] - self.root_le_m[2]);
        let along = |root: f64, tip: f64| root + s * (tip - root);
        (
            [
                along(self.root_le_m[0], self.tip_le_m[0]),
                along(self.root_le_m[1], self.tip_le_m[1]),
                z_m,
            ],
            along(self.root_chord_m, self.tip_chord_m),
        )
    }

    /// How far the root at `z_m` stands above the lowest body top under it;
    /// `None` where the chord is not positive or no body lies under it.
    fn clearance_at(&self, seat: &FinSeat, z_m: f64) -> Option<f64> {
        let (leading_edge, chord) = self.section_at(z_m);
        (chord > 0.0)
            .then(|| seat.lowest_top(leading_edge[0], leading_edge[0] + chord))
            .flatten()
            .map(|top| z_m - top)
    }

    /// This fin with its root moved to the attachment height on `seat`.
    ///
    /// The fin is returned unchanged when the attachment height cannot be
    /// bracketed between the lowest body top and the tip: no body lies under
    /// the root, or the body stands above the fin tip.
    pub(super) fn attached_to(&self, seat: &FinSeat) -> Self {
        let tip_z_m = self.tip_le_m[2];
        let Some(lowest) = seat.lowest() else {
            return *self;
        };
        let floats = |z_m: f64| self.clearance_at(seat, z_m).is_none_or(|gap| gap > 0.0);
        let (mut below, mut above) = (lowest.min(self.root_le_m[2]), tip_z_m);
        let bracketed = above.partial_cmp(&below) == Some(std::cmp::Ordering::Greater);
        if !bracketed || !floats(above) || floats(below) {
            return *self;
        }
        for _ in 0..BISECTION_STEPS {
            let mid = 0.5 * (below + above);
            if floats(mid) {
                above = mid;
            } else {
                below = mid;
            }
        }
        let (root_le_m, root_chord_m) = self.section_at(above);
        Self {
            root_le_m,
            root_chord_m,
            ..*self
        }
    }
}
