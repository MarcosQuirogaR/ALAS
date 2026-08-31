// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`DeckSpec`, `CabinGeometry`)
// Reference: alas @ rust-port-baseline.

//! Where the cabin is, how wide the floor is at each station, and where each
//! deck sits inside the fuselage.
//!
//! Both layout engines, the deck-plan drawings and the cargo loader all read
//! from this one object, which is the point of it: seats packed against a
//! floor width the hold loader disagreed about would produce a cabin and a
//! belly that cannot both be in the same aeroplane. It samples the *built*
//! fuselage rather than the configuration, so a body the design vector
//! stretched is the body the interior is laid out in.
//!
//! # Two things worth knowing before reading the numbers
//!
//! The deck fractions are of the internal half-height `b`, measured from the
//! section centre with +z up, so a deck occupies the band
//! `[zc + floor_frac*b, zc + ceil_frac*b]`. Items rest *on* the floor and have
//! their height clamped to the band, which is what stops a double-deck body's
//! two cabins from overlapping vertically.
//!
//! And the double-deck test is a shape test, not a name test: a body whose
//! declared height clears 1.15 diameters is treated as an A380-style ovoid and
//! gets two passenger decks. A circular fuselage leaves `height_m` unset and
//! can never reach it, which is why the test reads the raw `Option` rather
//! than [`alas_config::FuselageConfig::effective_height_m`] -- the latter
//! falls back to the diameter, and `d >= 1.15 d` would be a different
//! question that happens to have the same answer today.
//!
//! `_xsec_width` and `_xsec_height` do not survive the translation. Upstream
//! keeps them in `stability.py` to read a cross-section that may carry either
//! a `radius` or a `width`/`height` pair; `alas-geom::aircraft::fuselage`'s
//! `FuselageXSec` resolves that in its constructor, so both accessors are the
//! fields themselves here.

use alas_config::GeometryConfig;
use alas_geom::aircraft::airplane::Airplane;

use crate::numeric::interp;

/// The main wing is found by name, and everything else falls back to the first
/// wing -- upstream's `next((w for w in plane.wings if w.name == "Main Wing"),
/// plane.wings[0])`.
const MAIN_WING: &str = "Main Wing";

/// Quarter chord, which is where upstream leaves `aerodynamic_center`'s
/// default and therefore what `x_lemac` is measured back from.
const AERODYNAMIC_CENTER_CHORD_FRACTION: f64 = 0.25;

/// Numerical allowance used when testing a point against the inner ellipse.
const ENVELOPE_TOLERANCE: f64 = 1e-9;

/// Maximum longitudinal spacing between strict envelope evaluations.
const MAX_CONTAINMENT_STEP_M: f64 = 0.10;

/// Minimum clear deck-to-deck band, as a fraction of inner half-height.
///
/// This reserves approximately 0.15--0.20 m in a widebody for the floor beam,
/// panels and systems instead of letting the hold ceiling touch the cabin
/// floor geometrically.
const MIN_DECK_SEPARATION_FRAC: f64 = 0.06;

/// An airplane [`CabinGeometry`] cannot be built from.
///
/// Upstream indexes `plane.fuselages[0]` and `plane.wings[0]` and raises an
/// `IndexError` on either; this crate does not panic (`CONTRIBUTING.md`), so
/// the same two conditions are a typed error. `AircraftBuilder::build` always
/// supplies three wings and at least one fuselage, so neither is reachable
/// from this program's own inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CabinGeometryError {
    /// The airplane has no fuselage to lay an interior out inside.
    #[error("the airplane has no fuselage, so there is no cabin to lay out")]
    NoFuselage,
    /// The airplane has no wing, so there is no mean aerodynamic chord to
    /// report a centre of gravity against.
    #[error("the airplane has no wing, so there is no MAC to measure the payload CG against")]
    NoWings,
}

/// Why an installed item cannot be evaluated or contained by the cabin.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum InteriorEnvelopeError {
    /// One or more dimensions are negative or a coordinate is not finite.
    #[error("the interior item has a non-finite coordinate or a negative extent")]
    InvalidExtent,
    /// A corner of the item lies outside the usable, wall-inset fuselage.
    #[error(
        "the interior item protrudes through the cabin envelope at ({x:.3}, {y:.3}, {z:.3}) m"
    )]
    OutsideEnvelope {
        /// Longitudinal station, in metres.
        x: f64,
        /// Lateral coordinate, in metres; positive to starboard.
        y: f64,
        /// Vertical coordinate, in metres; positive upward.
        z: f64,
    },
}

/// A horizontal deck: its floor, its ceiling, and its usable width.
///
/// `width_factor` scales the internal width to the usable floor width, which
/// accounts for a deck sitting off the section centre where the cross-section
/// is narrower -- a lower hold in a circular body is a chord of the circle,
/// not its diameter.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckSpec {
    /// [`crate::layout::MAIN`], [`crate::layout::UPPER`] or
    /// [`crate::layout::LOWER`].
    pub name: &'static str,
    /// Floor height as a fraction of the internal half-height, +up.
    pub floor_frac: f64,
    /// Ceiling height as a fraction of the internal half-height, +up.
    pub ceil_frac: f64,
    /// Internal width to usable floor width.
    pub width_factor: f64,
    /// Whether this deck seats people.
    pub is_passenger: bool,
}

/// The built fuselage, sampled: cabin extent, per-station section, and the
/// decks inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct CabinGeometry {
    /// Inset per side from the outer skin to the usable cabin wall.
    pub wall: f64,
    /// The forwardmost station.
    pub x_min: f64,
    /// The aftmost station.
    pub x_max: f64,
    /// Overall length.
    pub fus_len: f64,
    /// The widest cross-section's diameter, from the configuration.
    pub diameter_m: f64,
    /// Where the nose taper ends.
    pub cabin_start_x: f64,
    /// How much of the length is aft taper.
    pub tailcone_len: f64,
    /// Where the constant section ends.
    pub cabin_end_x: f64,
    /// Whether this body gets two passenger decks.
    pub is_double_deck: bool,
    /// Mean aerodynamic chord, which the centre of gravity is reported against.
    pub mac: f64,
    /// The main wing's aerodynamic centre, longitudinally.
    pub x_wing_ac: f64,
    /// The leading edge of the MAC, the origin of the percent-MAC frame.
    pub x_lemac: f64,
    /// The main wing root's leading edge, the front of the centre wing box.
    pub x_wing_le: f64,
    /// The main wing root chord, the length of that box.
    pub wing_root_chord: f64,
    /// The passenger decks, forward-lowest first.
    pub passenger_decks: Vec<DeckSpec>,
    /// The lower-deck holds.
    pub lower_deck: DeckSpec,
    /// Whether product analyses enforce the physical inner-envelope contract.
    /// Frozen Python parity construction leaves this false deliberately.
    strict_envelope: bool,
    /// Station coordinates, ascending. The three sampled series below are in
    /// this same order, which is what lets one interpolation index serve all.
    x_stations: Vec<f64>,
    widths: Vec<f64>,
    heights: Vec<f64>,
    zcs: Vec<f64>,
}

impl CabinGeometry {
    /// Whether this frame enforces the product physical-envelope contract.
    pub const fn enforces_physical_envelope(&self) -> bool {
        self.strict_envelope
    }

    /// Sample `plane`'s first fuselage into a cabin frame.
    ///
    /// # Errors
    ///
    /// [`CabinGeometryError`], for an airplane with no fuselage or no wing.
    pub fn new(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
    ) -> Result<Self, CabinGeometryError> {
        Self::new_with_mode(plane, geometry_config, wall_thickness_m, true)
    }

    /// Rebuild the historical cabin frame for explicit Python parity paths.
    pub fn new_reference_compatibility(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
    ) -> Result<Self, CabinGeometryError> {
        Self::new_with_mode(plane, geometry_config, wall_thickness_m, false)
    }

    fn new_with_mode(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
        strict_envelope: bool,
    ) -> Result<Self, CabinGeometryError> {
        let fus = plane
            .fuselages
            .first()
            .ok_or(CabinGeometryError::NoFuselage)?;
        // Upstream sorts with `np.argsort`, whose default is not a stable
        // sort. Every fuselage this program builds has strictly increasing
        // stations, so no tie exists for the two to disagree about, and a
        // stable sort is the conservative choice where one ever did.
        let mut order: Vec<usize> = (0..fus.xsecs.len()).collect();
        order.sort_by(|&a, &b| fus.xsecs[a].xyz_c[0].total_cmp(&fus.xsecs[b].xyz_c[0]));

        let x_stations: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[0]).collect();
        let widths: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].width).collect();
        let heights: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].height).collect();
        let zcs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[2]).collect();

        let x_min = x_stations
            .first()
            .copied()
            .ok_or(CabinGeometryError::NoFuselage)?;
        let x_max = x_stations
            .last()
            .copied()
            .ok_or(CabinGeometryError::NoFuselage)?;

        let fg = &geometry_config.fuselage;
        let diameter_m = fg.diameter_m;
        let tailcone_len = fg.tailcone_length_m;

        let wing = plane
            .wings
            .iter()
            .find(|w| w.name == MAIN_WING)
            .or_else(|| plane.wings.first())
            .ok_or(CabinGeometryError::NoWings)?;
        let root = wing.xsecs.first().ok_or(CabinGeometryError::NoWings)?;

        let mac = plane.c_ref;
        let x_wing_ac = wing.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION)[0];

        let (passenger_decks, lower_deck) = if strict_envelope {
            decks(fg.height_m, diameter_m)
        } else {
            reference_decks(fg.height_m, diameter_m)
        };

        Ok(Self {
            wall: wall_thickness_m,
            x_min,
            x_max,
            fus_len: x_max - x_min,
            diameter_m,
            cabin_start_x: fg.cabin_start_x_m,
            tailcone_len,
            cabin_end_x: x_max - tailcone_len,
            is_double_deck: is_double_deck(fg.height_m, diameter_m),
            mac,
            x_wing_ac,
            x_lemac: x_wing_ac - AERODYNAMIC_CENTER_CHORD_FRACTION * mac,
            x_wing_le: root.xyz_le[0],
            wing_root_chord: root.chord,
            passenger_decks,
            lower_deck,
            strict_envelope,
            x_stations,
            widths,
            heights,
            zcs,
        })
    }

    /// The section width at station `x`, clamped to the end sections outside
    /// the body.
    pub fn width_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.widths)
    }

    /// The section height at station `x`.
    pub fn height_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.heights)
    }

    /// The section centre's height at station `x`, which follows the body's
    /// droop and tail upsweep.
    pub fn zc_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.zcs)
    }

    /// Half the internal height at station `x`, the unit every deck fraction
    /// is measured in.
    ///
    /// Floored at 0.1 m so that the tapered ends, where the wall thickness
    /// exceeds the half-section, still divide into decks rather than
    /// inverting.
    pub fn internal_half_height(&self, x: f64) -> f64 {
        (self.height_at(x) / 2.0 - self.wall).max(0.1)
    }

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

    /// Where `deck`'s floor sits at station `x`.
    pub fn floor_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        self.zc_at(x) + deck.floor_frac * self.internal_half_height(x)
    }

    /// Where `deck`'s ceiling sits at station `x`.
    pub fn ceil_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        self.zc_at(x) + deck.ceil_frac * self.internal_half_height(x)
    }

    /// The headroom on `deck` at station `x`, floored at 0.3 m for the same
    /// reason [`Self::internal_half_height`] has a floor.
    pub fn deck_height(&self, deck: &DeckSpec, x: f64) -> f64 {
        ((deck.ceil_frac - deck.floor_frac) * self.internal_half_height(x)).max(0.3)
    }

    /// An item height cut down to what `deck` has room for at station `x`.
    pub fn clamp_height(&self, deck: &DeckSpec, x: f64, h: f64) -> f64 {
        h.min(self.deck_height(deck, x))
    }

    /// The vertical centre of an item of height `h` resting on `deck`'s floor.
    pub fn item_z(&self, deck: &DeckSpec, x: f64, h: f64) -> f64 {
        self.floor_z(deck, x) + self.clamp_height(deck, x, h) / 2.0
    }

    /// The floor width available for seats or containers on `deck` at station
    /// `x`, after both walls and the deck's own width factor.
    pub fn usable_width(&self, deck: &DeckSpec, x: f64) -> f64 {
        if self.strict_envelope {
            self.usable_width_at_z(x, self.floor_z(deck, x)) * deck.width_factor
        } else {
            ((self.width_at(x) - 2.0 * self.wall).max(0.0) * deck.width_factor).max(0.0)
        }
    }

    /// Internal fuselage width available at an absolute vertical station.
    ///
    /// The conceptual fuselage sections are elliptical, so crown furniture
    /// cannot reuse the floor chord without protruding through the sidewall.
    pub fn usable_width_at_z(&self, x: f64, z: f64) -> f64 {
        let Some((half_width, half_height)) = self.inner_semi_axes(x) else {
            return 0.0;
        };
        let normalized_z = (z - self.zc_at(x)) / half_height.max(1e-6);
        if normalized_z.abs() >= 1.0 {
            return 0.0;
        }
        2.0 * half_width * (1.0 - normalized_z * normalized_z).sqrt()
    }

    /// Whether a point lies in the station-dependent wall-inset ellipse.
    pub fn contains_point(&self, x: f64, y: f64, z: f64) -> bool {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() || x < self.x_min || x > self.x_max {
            return false;
        }
        let Some((half_width, half_height)) = self.inner_semi_axes(x) else {
            return false;
        };
        let normalized_y = y / half_width;
        let normalized_z = (z - self.zc_at(x)) / half_height;
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
            .chain(
                self.x_stations
                    .iter()
                    .copied()
                    .filter(|x| *x > x_start && *x < x_end),
            )
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

    /// Internal fuselage width available at an absolute vertical station.
    ///
    /// The conceptual fuselage sections are elliptical, so crown furniture
    /// cannot reuse the floor chord without protruding through the sidewall.
    pub fn usable_width_at_z(&self, x: f64, z: f64) -> f64 {
        let half_width = (self.width_at(x) * 0.5 - self.wall).max(0.0);
        let half_height = self.internal_half_height(x);
        let normalized_z = (z - self.zc_at(x)) / half_height.max(1e-6);
        if normalized_z.abs() >= 1.0 {
            return 0.0;
        }
        2.0 * half_width * (1.0 - normalized_z * normalized_z).sqrt()
    }

    /// A station as a percentage of MAC.
    ///
    /// The MAC is floored at a micrometre so that a degenerate wing reports a
    /// large percentage rather than a division by zero, which is upstream's
    /// guard and matters because the optimizer evaluates infeasible designs.
    pub fn x_to_pct_mac(&self, x: f64) -> f64 {
        (x - self.x_lemac) / self.mac.max(1e-6) * 100.0
    }

    /// A percentage of MAC as a station.
    pub fn pct_mac_to_x(&self, pct: f64) -> f64 {
        self.x_lemac + (pct / 100.0) * self.mac
    }

    /// The longitudinal span of the centre wing box, which is what the lower
    /// holds are split forward and aft of.
    pub fn wing_box_x_range(&self) -> (f64, f64) {
        (self.x_wing_le, self.x_wing_le + self.wing_root_chord)
    }
}

/// Whether a body of this declared height and diameter is a double-decker.
fn is_double_deck(height_m: Option<f64>, diameter_m: f64) -> bool {
    height_m.is_some_and(|h| h >= diameter_m * 1.15)
}

/// The passenger decks and the lower hold for a body of this shape.
///
/// A double-decker's main deck sits low and its upper deck high, each taking
/// roughly half the section; a single-deck body's main deck takes nearly all
/// of it and the hold takes the bottom fifth.
fn decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    floor_frac: -0.30,
                    ceil_frac: 0.12,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.14 + MIN_DECK_SEPARATION_FRAC,
                    ceil_frac: 0.75,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.82,
                ceil_frac: -0.32 - MIN_DECK_SEPARATION_FRAC,
                width_factor: 0.55,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: 0.0,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.71,
                ceil_frac: -0.02 - MIN_DECK_SEPARATION_FRAC,
                width_factor: 0.60,
                is_passenger: false,
            },
        )
    }
}

/// Original Python deck table, retained only for explicit parity evidence.
fn reference_decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    floor_frac: -0.48,
                    ceil_frac: -0.02,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.04,
                    ceil_frac: 0.55,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.50,
                width_factor: 0.55,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: -0.18,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.20,
                width_factor: 0.60,
                is_passenger: false,
            },
        )
    }
}

// A test asserts on geometry it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::build_payload_layout;
    use alas_config::AlasConfig;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn xsec(x: f64, radius: f64) -> FuselageXSec {
        FuselageXSec::new([x, 0.0, 0.0], Some(radius), None, None, DEFAULT_SHAPE)
            .expect("a radius alone is a valid section")
    }

    fn wing() -> Wing {
        Wing {
            name: MAIN_WING.to_owned(),
            xsecs: vec![
                WingXSec {
                    xyz_le: [10.0, 0.0, 0.0],
                    chord: 6.0,
                    twist: 0.0,
                    airfoil: Airfoil::from_coordinates("probe", Vec::new()),
                },
                WingXSec {
                    xyz_le: [14.0, 12.0, 0.0],
                    chord: 2.0,
                    twist: 0.0,
                    airfoil: Airfoil::from_coordinates("probe", Vec::new()),
                },
            ],
            symmetric: true,
        }
    }

    fn plane(sections: Vec<FuselageXSec>) -> Airplane {
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![wing()],
            fuselages: vec![Fuselage::new("Fuselage", sections)],
            s_ref: 100.0,
            c_ref: 4.0,
            b_ref: 24.0,
        }
    }

    fn geometry(sections: Vec<FuselageXSec>) -> CabinGeometry {
        CabinGeometry::new(&plane(sections), &GeometryConfig::default(), 0.15)
            .expect("a probe with a fuselage and a wing builds")
    }

    #[test]
    fn sections_are_sorted_so_a_body_given_tail_first_samples_the_same() {
        // The station order is an input, not a guarantee: `argsort` exists
        // upstream because a builder is free to emit the tail first.
        let forward = geometry(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        let reversed = geometry(vec![xsec(10.0, 2.0), xsec(0.0, 1.0)]);
        assert_eq!(forward.width_at(5.0), reversed.width_at(5.0));
        assert_eq!(forward.x_min, reversed.x_min);
        assert_eq!(forward.x_max, reversed.x_max);
    }

    #[test]
    fn an_airplane_with_no_fuselage_is_an_error_and_not_a_panic() {
        let mut bare = plane(vec![xsec(0.0, 1.0)]);
        bare.fuselages.clear();
        assert_eq!(
            CabinGeometry::new(&bare, &GeometryConfig::default(), 0.15),
            Err(CabinGeometryError::NoFuselage)
        );
    }

    #[test]
    fn the_public_layout_entry_point_preserves_geometry_errors() {
        let mut bare = plane(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        bare.fuselages.clear();

        assert_eq!(
            build_payload_layout(&bare, &AlasConfig::default(), 0.0, 0.0),
            Err(CabinGeometryError::NoFuselage),
            "layout construction must not turn a missing cabin into an empty payload"
        );
    }

    #[test]
    fn an_airplane_with_no_wing_is_an_error_and_not_a_panic() {
        // There is no mean aerodynamic chord to report a payload CG against,
        // and the optimizer does evaluate geometry this degenerate.
        let mut wingless = plane(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        wingless.wings.clear();
        assert_eq!(
            CabinGeometry::new(&wingless, &GeometryConfig::default(), 0.15),
            Err(CabinGeometryError::NoWings)
        );
    }

    #[test]
    fn a_circular_body_gets_one_passenger_deck_and_an_ovoid_one_gets_two() {
        assert!(!is_double_deck(None, 7.14));
        assert!(!is_double_deck(Some(8.0), 7.14));
        assert!(is_double_deck(Some(8.41), 7.14));

        let (single, _) = decks(None, 6.2);
        assert_eq!(single.len(), 1);
        let (double, hold) = decks(Some(8.41), 7.14);
        assert_eq!(double.len(), 2);
        assert_eq!(double[1].name, crate::layout::UPPER);
        // The upper deck's floor has to clear the main deck's ceiling, or the
        // two cabins would be drawn through each other.
        assert!(double[1].floor_frac - double[0].ceil_frac >= MIN_DECK_SEPARATION_FRAC);
        assert!(double[0].floor_frac - hold.ceil_frac >= MIN_DECK_SEPARATION_FRAC);
        let (single, hold) = decks(None, 6.2);
        assert!(single[0].floor_frac - hold.ceil_frac >= MIN_DECK_SEPARATION_FRAC);
    }

    #[test]
    fn crown_width_is_narrower_than_the_section_center() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        let center = cabin.usable_width_at_z(5.0, 0.0);
        let crown = cabin.usable_width_at_z(5.0, 1.4);
        assert!(center > crown);
        assert!(crown > 0.0);
    }

    #[test]
    fn deck_width_is_the_ellipse_chord_at_its_actual_floor() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        let deck = &cabin.lower_deck;
        let floor_z = cabin.floor_z(deck, 5.0);
        assert_eq!(
            cabin.usable_width(deck, 5.0),
            cabin.usable_width_at_z(5.0, floor_z) * deck.width_factor
        );
        assert!(cabin.usable_width(deck, 5.0) < (4.0 - 2.0 * cabin.wall) * deck.width_factor);
    }

    #[test]
    fn rectangular_containment_checks_top_corners_not_only_the_centre() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        assert!(cabin
            .check_rectangular_prism(5.0, 1.0, 0.0, 1.0, -0.5, 1.0)
            .is_ok());
        let error = cabin
            .check_rectangular_prism(5.0, 1.0, 0.0, 3.0, 0.0, 1.5)
            .expect_err("the upper outer corners protrude through the ellipse");
        assert!(matches!(
            error,
            InteriorEnvelopeError::OutsideEnvelope { .. }
        ));
    }

    #[test]
    fn longitudinal_containment_checks_the_tapered_item_end() {
        let cabin = geometry(vec![xsec(0.0, 0.5), xsec(5.0, 2.0), xsec(10.0, 2.0)]);
        let error = cabin
            .check_rectangular_prism(3.0, 4.0, 0.0, 1.3, -0.25, 0.5)
            .expect_err("an item fitting at its centre still protrudes at its nose end");
        assert!(matches!(
            error,
            InteriorEnvelopeError::OutsideEnvelope { x, .. } if x == 1.0
        ));
    }

    #[test]
    fn invalid_item_extents_are_typed_instead_of_becoming_geometry() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        assert_eq!(
            cabin.check_rectangular_prism(5.0, -1.0, 0.0, 1.0, 0.0, 1.0),
            Err(InteriorEnvelopeError::InvalidExtent)
        );
    }

    #[test]
    fn crown_width_is_narrower_than_the_section_center() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        let center = cabin.usable_width_at_z(5.0, 0.0);
        let crown = cabin.usable_width_at_z(5.0, 1.4);
        assert!(center > crown);
        assert!(crown > 0.0);
    }

    #[test]
    fn a_wall_thicker_than_the_body_leaves_a_floor_rather_than_inverting() {
        // A nose section is narrower than twice the wall, and a negative
        // usable width would put seats outside the aeroplane.
        let g = geometry(vec![xsec(0.0, 0.05), xsec(10.0, 2.0)]);
        assert_eq!(g.usable_width(&g.lower_deck, 0.0), 0.0);
        assert_eq!(g.internal_half_height(0.0), 0.1);
        assert!(g.deck_height(&g.lower_deck, 0.0) >= 0.3);
    }

    #[test]
    fn an_item_rests_on_the_floor_and_is_capped_by_the_ceiling() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        let deck = &g.passenger_decks[0];
        let tall = 99.0;
        assert_eq!(g.clamp_height(deck, 10.0, tall), g.deck_height(deck, 10.0));
        assert_eq!(
            g.item_z(deck, 10.0, tall),
            g.floor_z(deck, 10.0) + g.deck_height(deck, 10.0) / 2.0
        );
        // A short item's centre is half its own height above the floor.
        assert_eq!(g.item_z(deck, 10.0, 1.0), g.floor_z(deck, 10.0) + 0.5);
    }

    #[test]
    fn the_percent_mac_frame_round_trips() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        for pct in [-20.0, 0.0, 25.0, 140.0] {
            let x = g.pct_mac_to_x(pct);
            assert!((g.x_to_pct_mac(x) - pct).abs() < 1e-9);
        }
        // Zero percent MAC is the leading edge of the MAC, by definition.
        assert_eq!(g.pct_mac_to_x(0.0), g.x_lemac);
    }

    #[test]
    fn the_wing_box_spans_the_root_chord_from_its_leading_edge() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        let (start, end) = g.wing_box_x_range();
        assert_eq!(start, 10.0);
        assert_eq!(end, 16.0);
    }
}
