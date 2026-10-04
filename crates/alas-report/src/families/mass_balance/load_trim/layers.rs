// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chart layers of the load-and-trim sheet that carry the loading result:
//! the four scoped limit sets.
//!
//! Limit line styles: ground dashed, takeoff solid, flight dotted, landing
//! dash-dot. The styles are shared with the panel key ([`limit_style`]).

use super::render::Ink;
use super::{limit_at, Frame, LimitVertex, LoadTrimSheetData};
use crate::scene::{Color, Point2D, Scene, SceneElement, Stroke};

/// Which limit set a line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LimitKind {
    Ground,
    Takeoff,
    Flight,
    Landing,
}

impl LimitKind {
    pub(super) const ALL: [Self; 4] = [Self::Ground, Self::Takeoff, Self::Flight, Self::Landing];

    /// The vertices of this set in `data`.
    pub(super) fn vertices(self, data: &LoadTrimSheetData) -> &[LimitVertex] {
        match self {
            Self::Ground => &data.ground_limits,
            Self::Takeoff => &data.takeoff_limits,
            Self::Flight => &data.flight_limits,
            Self::Landing => &data.landing_limits,
        }
    }
}

/// The stroke of a limit set: ground dashed, takeoff solid, flight dotted,
/// landing dash-dot.
pub(super) fn limit_style(kind: LimitKind, color: Color) -> Stroke {
    match kind {
        LimitKind::Ground => Stroke::dashed(color, 1.8, 7.0, 4.0),
        LimitKind::Takeoff => Stroke::new(color, 2.4),
        LimitKind::Flight => Stroke::dashed(color, 2.0, 1.5, 3.5),
        LimitKind::Landing => Stroke {
            color,
            width: 2.0,
            dash_array: Some(vec![8.0, 3.0, 1.5, 3.0]),
        },
    }
}

/// Draw the forward and aft line of every limit set over its own mass band,
/// clipped to the frame's weight range.
pub(super) fn draw_limit_sets(scene: &mut Scene, data: &LoadTrimSheetData, fr: &Frame, ink: &Ink) {
    for kind in LimitKind::ALL {
        let vertices = kind.vertices(data);
        let (Some(first), Some(last)) = (vertices.first(), vertices.last()) else {
            continue;
        };
        let lo = first.mass_kg.max(fr.w_range_kg.0);
        let hi = last.mass_kg.min(fr.w_range_kg.1);
        if hi <= lo {
            continue;
        }
        let n = 60;
        for fwd in [true, false] {
            let points: Vec<Point2D> = (0..=n)
                .map(|k| {
                    let m = lo + (hi - lo) * f64::from(k) / f64::from(n);
                    fr.map(data.index_at(m, limit_at(vertices, m, fwd)), m)
                })
                .collect();
            scene.add(SceneElement::Polyline {
                points,
                stroke: limit_style(kind, ink.text),
            });
        }
    }
}
