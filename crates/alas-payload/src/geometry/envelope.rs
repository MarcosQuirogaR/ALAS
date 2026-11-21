// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wall-inset elliptical envelope containment checks for [`super::CabinGeometry`].

use super::{CabinGeometry, DeckSpec, InteriorEnvelopeError};
use crate::numeric::interp;

/// Numerical allowance used when testing a point against the inner ellipse.
const ENVELOPE_TOLERANCE: f64 = 1e-9;

/// Maximum longitudinal spacing between strict envelope evaluations.
const MAX_CONTAINMENT_STEP_M: f64 = 0.10;

impl CabinGeometry {
    /// Actual semi-axes of the wall-inset elliptical envelope at `x`.
    ///
    /// Unlike [`Self::internal_half_height`], this strict geometry does not
    /// invent space in a tapered section. `None` means the lining consumes
    /// the complete local section.
    pub fn inner_semi_axes(&self, x: f64) -> Option<(f64, f64)> {
        let half_width = self.width_at(x) * 0.5 - self.wall;
        let half_height = self.height_at(x) * 0.5 - self.wall;
        if half_width > 0.0 && half_height > 0.0 {
            Some((half_width, half_height))
        } else {
            None
        }
    }

    /// The lining ellipse that applies at height `z`: `(half_width, half_height,
    /// centre_z)`. Under an upper-deck hump the section is two lobes; below the
    /// main lobe's centre the lining is the main lobe's circle (the crown rise
    /// taken off the height), not the tall ellipse that also covers the hump,
    /// which is far narrower than the real lower lobe at the cargo floor.
    pub(super) fn lining_axes_at(&self, x: f64, z: f64) -> Option<(f64, f64, f64)> {
        self.lining_axes_in(x, z, false)
    }

    /// [`Self::lining_axes_at`] in the outer loft (`outer`, which is what the
    /// passenger deck stands in) or in the loadable interior, which keeps the
    /// cabin keel through a container hold: a seat row is contained by the
    /// skin around it, so declaring a lower hold must not widen it.
    pub(super) fn lining_axes_in(&self, x: f64, z: f64, outer: bool) -> Option<(f64, f64, f64)> {
        let (zc, height) = if outer {
            (self.zc_at(x), self.height_at(x))
        } else {
            self.interior_section_at(x)
        };
        let half_width = self.width_at(x) * 0.5 - self.wall;
        let half_height = height * 0.5 - self.wall;
        if half_width <= 0.0 || half_height <= 0.0 {
            return None;
        }
        let rise = self.crown_rise_m(x);
        if rise > 0.0 && z < self.main_floor_z(x) {
            let lobe_half = 0.5 * (height - rise) - self.wall;
            if lobe_half > 0.0 {
                return Some((half_width, lobe_half, self.main_lobe_zc_at(x)));
            }
        }
        Some((half_width, half_height, zc))
    }

    /// Half the lined interior's width at station `x` and height `z`, m: the
    /// lining alone, whatever hold polygon the section carries; zero outside it.
    pub(super) fn lining_half_width_at(&self, x: f64, z: f64) -> f64 {
        let Some((half_width, half_height, centre)) = self.lining_axes_at(x, z) else {
            return 0.0;
        };
        let normalized_z = (z - centre).abs() / half_height.max(1e-6);
        if normalized_z >= 1.0 {
            return 0.0;
        }
        let shape = self.shape_at(x);
        half_width * (1.0 - normalized_z.powf(shape)).powf(1.0 / shape)
    }

    /// Half the outer skin's width at station `x` and height `z`, following the
    /// section's own superellipse exponent; zero outside the section.
    pub(super) fn outer_half_width(&self, x: f64, z: f64) -> f64 {
        let (zc, height) = self.interior_section_at(x);
        let (half_width, half_height) = (0.5 * self.width_at(x), 0.5 * height);
        let normalized_z = (z - zc).abs() / half_height.max(1e-6);
        if normalized_z >= 1.0 {
            return 0.0;
        }
        let shape = self.shape_at(x);
        half_width * (1.0 - normalized_z.powf(shape)).powf(1.0 / shape)
    }

    /// Internal fuselage width available at an absolute vertical station.
    ///
    /// The conceptual fuselage sections are elliptical, so crown furniture
    /// cannot reuse the floor chord without protruding through the sidewall.
    pub fn usable_width_at_z(&self, x: f64, z: f64) -> f64 {
        self.usable_width_in(x, z, false)
    }

    /// [`Self::usable_width_at_z`] in the outer loft (`outer`) or the loadable
    /// interior (see [`Self::lining_axes_in`]).
    fn usable_width_in(&self, x: f64, z: f64, outer: bool) -> f64 {
        if let Some(half) = self.hold_half_width(x, z) {
            return 2.0 * half;
        }
        let Some((half_width, half_height, centre)) = self.lining_axes_in(x, z, outer) else {
            return 0.0;
        };
        let normalized_z = (z - centre) / half_height.max(1e-6);
        if normalized_z.abs() >= 1.0 {
            return 0.0;
        }
        2.0 * half_width * (1.0 - normalized_z * normalized_z).sqrt()
    }

    /// The narrowest usable floor width of `deck` between stations `x0` and
    /// `x1`, which is what a row of seats spanning them has to fit.
    ///
    /// Taper and tail upsweep narrow the section monotonically between two
    /// definition stations, so the ends and the stations inside bound it.
    /// The frozen reference frame keeps the width at `x0` alone.
    pub fn usable_width_over(&self, deck: &DeckSpec, x0: f64, x1: f64) -> f64 {
        if !self.strict_envelope || x1 <= x0 {
            return self.usable_width(deck, x0);
        }
        self.x_stations
            .iter()
            .copied()
            .filter(|x| *x > x0 && *x < x1)
            .chain([x0, x1])
            .map(|x| self.usable_width(deck, x))
            .fold(f64::INFINITY, f64::min)
    }

    /// The floor width available for seats or containers on `deck` at station
    /// `x`, after both walls and the deck's own width factor. On a body with a
    /// drawn floor the width a bay or a door sees is the cabin width at the
    /// armrests; a row of seats is held to its shoulders as well
    /// (`row_width_over`).
    pub fn usable_width(&self, deck: &DeckSpec, x: f64) -> f64 {
        if !self.strict_envelope {
            return ((self.width_at(x) - 2.0 * self.wall).max(0.0) * deck.width_factor).max(0.0);
        }
        let floor = self.floor_z(deck, x);
        if deck.is_passenger && self.floor_datum_applied {
            self.lining_chord_in(x, floor + super::ARMREST_ABOVE_FLOOR_M, true) * deck.width_factor
        } else if deck.name == crate::layout::LOWER && self.hold.is_none() {
            // An underfloor hold without a drawn polygon: the lining's chord at
            // the cargo floor less the lining the hold needs beyond the wall,
            // on both sides. A scalar share of the chord (the deck table's
            // 0.90) is not the structure: the allowance is the 0.22 m the A340
            // hold drawing keeps between the flat floor and the skin
            // (`MIN_HOLD_LINING_M`) less the wall the lining already is.
            let extra = (super::hold::MIN_HOLD_LINING_M - self.wall).max(0.0);
            (self.usable_width_at_z(x, floor) - 2.0 * extra).max(0.0)
        } else {
            self.usable_width_in(x, floor, deck.is_passenger) * deck.width_factor
        }
    }

    /// The width a row of seats `seat_w` wide standing on a floor at height
    /// `floor` fits at `x`: the narrower of the lined chord at its armrests
    /// (`ARMREST_ABOVE_FLOOR_M`) and, at the shoulders of the tall occupant
    /// (`SHOULDER_ABOVE_FLOOR_M`), the lined chord less the overhang of the
    /// outboard shoulders beyond the seat on both sides. Below the seat pan
    /// the base is narrower than the row (`SEAT_PAN_ABOVE_FLOOR_M`).
    pub(super) fn seat_chord_at(&self, x: f64, floor: f64, seat_w: f64) -> f64 {
        let lined = |height: f64, scale: f64| self.lining_chord_in(x, floor + height, true) * scale;
        // The seat backs keep their full width to `SEAT_BACK_FULL_WIDTH_TOP_M`.
        let back = lined(super::SEAT_BACK_FULL_WIDTH_TOP_M, self.lining_scale[1])
            + 2.0 * super::SEAT_BACK_INSET_M;
        lined(super::ARMREST_ABOVE_FLOOR_M, self.lining_scale[0])
            .min(
                lined(super::SHOULDER_ABOVE_FLOOR_M, self.lining_scale[1])
                    - 2.0 * super::shoulder_overhang_m(seat_w),
            )
            .min(back)
    }

    /// The narrowest chord a row of seats `seat_w` wide fits between `x0` and
    /// `x1` on `deck`, before any deck allowance: the armrest and shoulder
    /// chords on a body whose floor datum is applied, the wall-inset chord at
    /// the floor otherwise. Falls back to the interior diameter on the frozen
    /// frame.
    pub fn row_chord_over(&self, deck: &DeckSpec, x0: f64, x1: f64, seat_w: f64) -> f64 {
        if !self.strict_envelope {
            return self.diameter_m - 2.0 * self.wall;
        }
        self.x_stations
            .iter()
            .copied()
            .filter(|x| *x > x0 && *x < x1)
            .chain([x0, x1])
            .map(|x| {
                let floor = self.floor_z(deck, x);
                if deck.is_passenger && self.floor_datum_applied {
                    self.seat_chord_at(x, floor, seat_w)
                } else {
                    self.usable_width_in(x, floor, deck.is_passenger)
                }
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// The width a row of seats `seat_w` wide may take between `x0` and `x1`,
    /// deck allowance included (the frozen frame and the decks without a drawn
    /// floor keep the width at the floor).
    pub fn row_width_over(&self, deck: &DeckSpec, x0: f64, x1: f64, seat_w: f64) -> f64 {
        if deck.is_passenger && self.floor_datum_applied && self.strict_envelope {
            self.row_chord_over(deck, x0, x1, seat_w) * deck.width_factor
        } else {
            self.usable_width_over(deck, x0, x1)
        }
    }

    /// The superellipse exponent of the section at `x`, interpolated like the
    /// sections themselves (2 is the ellipse the strict envelope assumes).
    fn shape_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.shapes).max(1.0)
    }

    /// Width of the lined interior at station `x` and height `z`, following
    /// the section's own superellipse exponent: the wall-inset section that
    /// monuments and bins (not the ULD polygons) have to stand in.
    pub fn lining_chord_at_z(&self, x: f64, z: f64) -> f64 {
        self.lining_chord_in(x, z, false)
    }

    /// [`Self::lining_chord_at_z`] in the outer loft (`outer`) or the loadable
    /// interior (see [`Self::lining_axes_in`]).
    fn lining_chord_in(&self, x: f64, z: f64, outer: bool) -> f64 {
        if let Some(half) = self.hold_half_width(x, z) {
            return 2.0 * half;
        }
        let Some((half_width, half_height, centre)) = self.lining_axes_in(x, z, outer) else {
            return 0.0;
        };
        let normalized_z = (z - centre).abs() / half_height.max(1e-6);
        if normalized_z >= 1.0 {
            return 0.0;
        }
        let shape = self.shape_at(x);
        2.0 * half_width * (1.0 - normalized_z.powf(shape)).powf(1.0 / shape)
    }

    /// The height of the lined section's crown at lateral offset `y` of
    /// station `x` (the lining following the section's own superellipse
    /// exponent), or `None` when `y` lies outside the lined section there.
    pub fn lining_top_z(&self, x: f64, y: f64) -> Option<f64> {
        let (half_width, half_height) = self.inner_semi_axes(x)?;
        let share = y.abs() / half_width;
        if share >= 1.0 {
            return None;
        }
        let shape = self.shape_at(x);
        Some(self.zc_at(x) + half_height * (1.0 - share.powf(shape)).powf(1.0 / shape))
    }

    /// Whether a point lies in the station-dependent wall-inset ellipse.
    pub fn contains_point(&self, x: f64, y: f64, z: f64) -> bool {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() || x < self.x_min || x > self.x_max {
            return false;
        }
        if let Some(half) = self.hold_half_width(x, z) {
            return y.abs() <= half + ENVELOPE_TOLERANCE;
        }
        let Some((half_width, half_height, centre)) = self.lining_axes_at(x, z) else {
            return false;
        };
        let normalized_y = y / half_width;
        let normalized_z = (z - centre) / half_height;
        normalized_y.mul_add(normalized_y, normalized_z * normalized_z) <= 1.0 + ENVELOPE_TOLERANCE
    }

    /// Check a constant cross-section polygon throughout a longitudinal span.
    ///
    /// `vertices_yz` are absolute `(y, z)` coordinates. The check includes
    /// both item ends and every fuselage definition station inside the span,
    /// so nose/tail taper and centreline upsweep cannot be skipped by a check
    /// performed only at the item's centre.
    pub fn check_polygon_containment(
        &self,
        x_start: f64,
        x_end: f64,
        vertices_yz: &[[f64; 2]],
    ) -> Result<(), InteriorEnvelopeError> {
        if !self.strict_envelope {
            return Ok(());
        }
        if !x_start.is_finite()
            || !x_end.is_finite()
            || x_start > x_end
            || vertices_yz.is_empty()
            || vertices_yz
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
        {
            return Err(InteriorEnvelopeError::InvalidExtent);
        }

        let breakpoints: Vec<f64> = std::iter::once(x_start)
            .chain(self.stations_within(x_start, x_end))
            .chain(std::iter::once(x_end))
            .collect();
        for interval in breakpoints.windows(2) {
            let interval_length = interval[1] - interval[0];
            let steps = (interval_length / MAX_CONTAINMENT_STEP_M).ceil().max(1.0) as usize;
            for step in 0..=steps {
                let x = interval[0] + interval_length * step as f64 / steps as f64;
                for &[y, z] in vertices_yz {
                    if !self.contains_point(x, y, z) {
                        return Err(InteriorEnvelopeError::OutsideEnvelope { x, y, z });
                    }
                }
            }
        }
        Ok(())
    }

    /// The fuselage definition stations strictly between `x_start` and
    /// `x_end`: where the section outlines, interpolated linearly between
    /// them, and the decks laid in those outlines change slope.
    pub fn stations_within(&self, x_start: f64, x_end: f64) -> impl Iterator<Item = f64> + '_ {
        self.x_stations
            .iter()
            .copied()
            .filter(move |x| *x > x_start && *x < x_end)
    }

    /// Check all eight corners of an axis-aligned rectangular installation.
    pub fn check_rectangular_prism(
        &self,
        x_center: f64,
        length: f64,
        y_center: f64,
        width: f64,
        z_bottom: f64,
        height: f64,
    ) -> Result<(), InteriorEnvelopeError> {
        if !x_center.is_finite()
            || !length.is_finite()
            || !y_center.is_finite()
            || !width.is_finite()
            || !z_bottom.is_finite()
            || !height.is_finite()
            || length < 0.0
            || width < 0.0
            || height < 0.0
        {
            return Err(InteriorEnvelopeError::InvalidExtent);
        }
        let half_width = width * 0.5;
        let vertices = [
            [y_center - half_width, z_bottom],
            [y_center + half_width, z_bottom],
            [y_center - half_width, z_bottom + height],
            [y_center + half_width, z_bottom + height],
        ];
        self.check_polygon_containment(x_center - length * 0.5, x_center + length * 0.5, &vertices)
    }
}

/// A vertex of the hold polygon and how far it stands inside the lined
/// interior of the section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldVertexClearance {
    /// Which vertex of the polygon.
    pub name: &'static str,
    /// Half-width of the vertex, m.
    pub y_m: f64,
    /// Height of the vertex, m.
    pub z_m: f64,
    /// Distance to the lining, m: positive inside it, negative where the
    /// vertex stands in the lining or beyond it.
    pub clearance_m: f64,
}

impl CabinGeometry {
    /// The clearance of each vertex of the underfloor hold's polygon at
    /// station `x` to the lined interior; empty for a body without a drawn or
    /// class hold polygon.
    pub fn hold_vertex_clearances(&self, x: f64) -> Vec<HoldVertexClearance> {
        let Some(hold) = self.hold.as_ref() else {
            return Vec::new();
        };
        let floor = self.floor_z(&self.lower_deck, x);
        let ceiling = self.ceil_z(&self.lower_deck, x);
        [
            ("floor edge", hold.floor_half_m, floor),
            ("wall foot", hold.side_half_m, floor + hold.chamfer_height_m),
            ("wall top", hold.wall_top_half_m, ceiling),
        ]
        .into_iter()
        .map(|(name, y, z)| HoldVertexClearance {
            name,
            y_m: y,
            z_m: z,
            clearance_m: self.distance_inside_lining(x, y, z),
        })
        .collect()
    }

    /// Distance from `(y, z)` to the lining curve of station `x`, positive
    /// inside the lined section.
    fn distance_inside_lining(&self, x: f64, y: f64, z: f64) -> f64 {
        const SAMPLES: usize = 4000;
        let Some((half_width, half_height, centre)) = self.lining_axes_at(x, z) else {
            return f64::NAN;
        };
        let shape = self.shape_at(x);
        let point = |t: f64| {
            (
                half_width * (1.0 - t.abs().powf(shape)).max(0.0).powf(1.0 / shape),
                centre + half_height * t,
            )
        };
        let nearest = (0..=SAMPLES)
            .map(|i| point(-1.0 + 2.0 * i as f64 / SAMPLES as f64))
            .map(|(by, bz)| ((by - y.abs()).powi(2) + (bz - z).powi(2)).sqrt())
            .fold(f64::INFINITY, f64::min);
        let normalized = (z - centre).abs() / half_height.max(1e-6);
        let inside = normalized < 1.0
            && y.abs() <= half_width * (1.0 - normalized.powf(shape)).powf(1.0 / shape);
        if inside {
            nearest
        } else {
            -nearest
        }
    }
}
