// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where a point mass may be put: anywhere in the airframe envelope, not only
//! in the cabin and the holds.
//!
//! The envelope is the union of
//! - the fuselage section at each station (an ellipse of the outer width and
//!   height, which is how the body is lofted),
//! - a belly strip below it, [`EXTERNAL_M`] deep and half the body width wide
//!   (a radar, a camera or a centreline store), and
//! - a band under the main wing, [`EXTERNAL_M`] deep below the wing's
//!   leading edge and out to the wing tip (pylon stores, extra tanks).
//!
//! [`EXTERNAL_M`] is the editor's reach for external carriage, not a
//! clearance rule: it only bounds where a click or a drag may put an item.
//! Side-view clicks resolve the lateral position `y` that makes the point
//! lie in the envelope (0 where the fuselage is, the wing station under a
//! wing).

use super::side_view::SideView;

/// How far below the belly or the wing an item may hang, m.
pub(super) const EXTERNAL_M: f64 = 2.0;
/// Headroom above the wing band, m.
const WING_ABOVE_M: f64 = 0.3;
/// Lateral step when searching the span for a side-view station, m.
const Y_STEP_M: f64 = 0.25;

impl SideView {
    /// The x extent of the main wing's leading edge and root chord.
    fn wing_x_span(&self) -> Option<[f64; 2]> {
        let first = self.wing_le.first()?;
        let (low, high) = self
            .wing_le
            .iter()
            .fold((first[0], first[0] + self.wing_chord), |(low, high), p| {
                (low.min(p[0]), high.max(p[0] + self.wing_chord * 0.25))
            });
        Some([low, high])
    }

    /// Whether station `x` is within the wing's longitudinal reach.
    fn under_wing_stations(&self, x: f64) -> bool {
        self.wing_x_span()
            .is_some_and(|[w0, w1]| x >= w0 && x <= w1)
    }

    /// Half span of the wing, m (0 without one).
    pub(super) fn semi_span(&self) -> f64 {
        self.wing_le.iter().map(|p| p[1]).fold(0.0, f64::max)
    }

    /// The wing's leading-edge height at spanwise station `y`, linearly
    /// interpolated; `None` past the tip or without a wing.
    pub(super) fn wing_z_at(&self, y: f64) -> Option<f64> {
        let y = y.abs();
        self.wing_le.windows(2).find_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            (y >= a[1].min(b[1]) && y <= a[1].max(b[1])).then(|| {
                let t = if (b[1] - a[1]).abs() > 1.0e-9 {
                    (y - a[1]) / (b[1] - a[1])
                } else {
                    0.0
                };
                a[2] + t * (b[2] - a[2])
            })
        })
    }

    /// The vertical interval the envelope covers at station `x` (over every
    /// lateral position), `None` outside the body's length.
    pub(super) fn z_bounds(&self, x: f64) -> Option<(f64, f64)> {
        let [x0, x1] = self.x_range();
        if x < x0 || x > x1 {
            return None;
        }
        let mut low = self.at(&self.bottom, x) - EXTERNAL_M;
        let mut high = self.at(&self.top, x);
        if self.under_wing_stations(x) {
            for point in &self.wing_le {
                low = low.min(point[2] - EXTERNAL_M);
                high = high.max(point[2] + WING_ABOVE_M);
            }
        }
        Some((low, high))
    }

    /// Whether the point `(x, y, z)` lies in the airframe envelope.
    pub(super) fn in_envelope(&self, x: f64, y: f64, z: f64) -> bool {
        let Some((low, high)) = self.z_bounds(x) else {
            return false;
        };
        if z < low || z > high {
            return false;
        }
        let top = self.at(&self.top, x);
        let bottom = self.at(&self.bottom, x);
        let half_width = (self.at(&self.widths, x) / 2.0).max(1.0e-3);
        let half_height = ((top - bottom) / 2.0).max(1.0e-3);
        let centre = f64::midpoint(top, bottom);
        let fuselage = (y / half_width).powi(2) + ((z - centre) / half_height).powi(2) <= 1.0;
        let belly = y.abs() <= 0.5 * half_width && z <= bottom && z >= bottom - EXTERNAL_M;
        let under_wing = self.under_wing_stations(x)
            && self
                .wing_z_at(y)
                .is_some_and(|wz| z >= wz - EXTERNAL_M && z <= wz + WING_ABOVE_M);
        fuselage || belly || under_wing
    }

    /// The lateral position (m, `>= 0`) that puts `(x, z)` in the envelope:
    /// the centreline where the fuselage is, otherwise the first span station
    /// outboard that is under the wing. `None` when no `y` does.
    pub(super) fn lateral_for(&self, x: f64, z: f64) -> Option<f64> {
        if self.in_envelope(x, 0.0, z) {
            return Some(0.0);
        }
        let reach = self.semi_span().max(self.at(&self.widths, x) / 2.0);
        (1..)
            .map(|n| f64::from(n) * Y_STEP_M)
            .take_while(|y| *y <= reach)
            .find(|y| self.in_envelope(x, *y, z))
    }

    /// A side-view position held in the envelope: `x` and `z` clamped to the
    /// body's reach, `y` kept when it still fits and otherwise re-resolved.
    pub(super) fn clamp_side(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        let [x0, x1] = self.x_range();
        let x = x.clamp(x0, x1);
        let (low, high) = self.z_bounds(x).unwrap_or((z, z));
        let z = z.clamp(low, high);
        if self.in_envelope(x, y, z) {
            return (x, y, z);
        }
        (x, self.lateral_for(x, z).unwrap_or(0.0), z)
    }

    /// A front-view position held in the envelope at station `x`: `z` clamped
    /// to the reach there, `y` pulled in toward the centreline until it fits.
    pub(super) fn clamp_front(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        let (low, high) = self.z_bounds(x).unwrap_or((z, z));
        let z = z.clamp(low, high);
        if self.in_envelope(x, y, z) {
            return (y, z);
        }
        let (mut inside, mut outside) = (0.0_f64, 1.0_f64);
        for _ in 0..24 {
            let middle = f64::midpoint(inside, outside);
            if self.in_envelope(x, y * middle, z) {
                inside = middle;
            } else {
                outside = middle;
            }
        }
        (y * inside, z)
    }

    /// The fuselage section at station `x` as a closed outline of `(y, z)`.
    pub(super) fn section_outline(&self, x: f64) -> Vec<(f64, f64)> {
        let top = self.at(&self.top, x);
        let bottom = self.at(&self.bottom, x);
        let (a, b) = (self.at(&self.widths, x) / 2.0, (top - bottom) / 2.0);
        let centre = f64::midpoint(top, bottom);
        (0..48)
            .map(|n| {
                let angle = f64::from(n) / 48.0 * std::f64::consts::TAU;
                (a * angle.cos(), centre + b * angle.sin())
            })
            .collect()
    }

    /// The half width, m, of the fuselage section at station `x` and height
    /// `z` (the chord of the elliptical section there); `0.0` above or below
    /// the section.
    pub(super) fn section_half_width_at(&self, x: f64, z: f64) -> f64 {
        let top = self.at(&self.top, x);
        let bottom = self.at(&self.bottom, x);
        let half_height = ((top - bottom) / 2.0).max(1.0e-3);
        let t = (z - f64::midpoint(top, bottom)) / half_height;
        if t.abs() >= 1.0 {
            return 0.0;
        }
        self.at(&self.widths, x) / 2.0 * (1.0 - t * t).sqrt()
    }

    /// The x run around `x` over which height `z` stays in the side envelope
    /// (what a movable point mass of the span may slide over).
    pub(super) fn x_run_at(&self, x: f64, z: f64) -> [f64; 2] {
        let inside = |station: f64| self.lateral_for(station, z).is_some();
        let n = self.stations.len();
        let at = self
            .stations
            .partition_point(|&s| s < x)
            .min(n.saturating_sub(1));
        let mut low = at;
        while low > 0 && inside(self.stations[low - 1]) {
            low -= 1;
        }
        let mut high = at;
        while high + 1 < n && inside(self.stations[high + 1]) {
            high += 1;
        }
        match (self.stations.get(low), self.stations.get(high)) {
            (Some(&a), Some(&b)) if a <= x && x <= b => [a, b],
            _ => [x, x],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::payload_editor::side_view::test_body;

    /// The rectangular test body with a wing from x = 14 m, 20 m out, rising
    /// from z = -5 m at the root to -1 m at the tip.
    fn winged() -> SideView {
        let mut view = test_body();
        view.widths = vec![4.0; view.stations.len()];
        view.wing_le = vec![[14.0, 0.0, -5.0], [18.0, 20.0, -1.0]];
        view.wing_chord = 6.0;
        view
    }

    #[test]
    fn the_belly_the_nose_and_the_wing_underside_are_in_the_envelope() {
        let view = winged();
        assert!(view.in_envelope(10.0, 0.0, 0.5), "cabin");
        assert!(view.in_envelope(10.0, 0.0, -3.5), "belly pod");
        assert!(!view.in_envelope(10.0, 0.0, -4.5), "below the belly reach");
        assert!(view.in_envelope(0.0, 0.0, 0.0), "nose station");
        assert!(view.in_envelope(16.0, 10.0, -3.5), "under the wing");
        assert!(!view.in_envelope(16.0, 25.0, -3.5), "past the wing tip");
        assert!(!view.in_envelope(45.0, 0.0, 0.0), "past the tail");
    }

    #[test]
    fn a_side_view_point_under_the_wing_resolves_its_span_station() {
        let view = winged();
        assert_eq!(view.lateral_for(10.0, 0.5), Some(0.0));
        let y = view.lateral_for(16.0, -4.5).expect("under the wing");
        assert!(y > 0.0 && view.in_envelope(16.0, y, -4.5));
        assert_eq!(view.lateral_for(5.0, -7.0), None);
    }

    #[test]
    fn a_front_view_drag_is_pulled_back_into_the_envelope() {
        let view = winged();
        let (y, z) = view.clamp_front(16.0, 40.0, -2.0);
        assert!(view.in_envelope(16.0, y, z));
        assert!(y <= 20.0 + 1e-6);
        let (y, _) = view.clamp_front(30.0, 9.0, 0.0);
        assert!(y <= 2.0 + 1e-6, "outside the wing, only the fuselage: {y}");
    }
}
