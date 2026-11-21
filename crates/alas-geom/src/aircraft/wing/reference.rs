// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The manufacturer reference-wing override of a [`Wing`]: the area, span,
//! mean aerodynamic chord and MAC leading-edge station the aircraft quotes
//! its coefficients, wing loading and `%MAC` against when they are not those
//! of the drawn planform. The construction lives in `alas-config`
//! (`TransportPlanform::reference_trapezoid`); this module only carries and
//! serves the result. A reference changes no section: the lattice and the wing's
//! own area, MAC and wetted integrals read the lofted sections, and the box, mass
//! and balance readers read [`Wing::carried_box`].

use std::borrow::Cow;
use std::fmt;

use super::*;

/// The reference wing's quantities, projected onto the aircraft XY plane,
/// metres and square metres, x positive aft in the same frame as
/// [`WingXSec::xyz_le`].
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct WingReference {
    /// Reference area of the whole (both-semispan) wing, m^2.
    pub area_m2: f64,
    /// Reference span, m.
    pub span_m: f64,
    /// Reference mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Leading-edge x of the reference MAC, m.
    pub mac_le_x_m: f64,
}

impl WingReference {
    /// The same reference after the wing moves by `dx` metres in x.
    #[must_use]
    pub fn translate(self, dx: f64) -> Self {
        Self {
            mac_le_x_m: self.mac_le_x_m + dx,
            ..self
        }
    }
}

/// The design planform behind a wing whose lofted sections inside the body are
/// the physical wing rather than the design root (the builder's `carry_through`
/// module): what the design root and the straight-edge tip were before that
/// replacement, and the side-of-body section the structural box carries through
/// the body. Metres, square metres and degrees, aircraft axes, x aft.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PlanformDatum {
    /// Leading edge of the design root section at the centreline.
    pub root_le_m: [f64; 3],
    /// Design root chord.
    pub root_chord_m: f64,
    /// Chordwise unit axis of the design root section.
    pub root_chord_axis: [f64; 3],
    /// Twist of the design root section. The loft holds the side-of-body twist
    /// inboard of the side of body; the design twist line runs to this value.
    pub root_twist_deg: f64,
    /// Leading-edge x the straight edges reach at the tip.
    pub tip_le_x_m: f64,
    /// Chord the straight edges reach at the tip (the design vector's tip chord:
    /// no leading-edge rake, no trailing-edge flare).
    pub tip_chord_m: f64,
    /// Projected area of both semispans on the design planform: with a wingtip
    /// trailing-edge flare, the trailing edge stays on its straight line outboard
    /// of the last station on it; a leading-edge tip rake stays as drawn (the
    /// drawn wing's area is `Wing::projected_area`).
    pub gross_area_m2: f64,
    /// Projected mean aerodynamic chord of the design planform, on that same
    /// unflared trailing edge and drawn leading edge.
    pub gross_mac_m: f64,
    /// Leading-edge x of that mean aerodynamic chord (drawn leading edge).
    pub gross_mac_le_x_m: f64,
    /// Spanwise station of the side of body.
    pub side_of_body_y_m: f64,
    /// Leading-edge x of the side-of-body section, the one the box carries through.
    pub side_of_body_le_x_m: f64,
    /// Chord of the side-of-body section.
    pub side_of_body_chord_m: f64,
    /// Twist of the side-of-body section.
    pub side_of_body_twist_deg: f64,
}

impl PlanformDatum {
    /// The same datum after the wing moves by `xyz` metres.
    #[must_use]
    pub fn translate(self, xyz: [f64; 3]) -> Self {
        Self {
            root_le_m: add3(self.root_le_m, xyz),
            tip_le_x_m: self.tip_le_x_m + xyz[0],
            gross_mac_le_x_m: self.gross_mac_le_x_m + xyz[0],
            side_of_body_y_m: self.side_of_body_y_m + xyz[1],
            side_of_body_le_x_m: self.side_of_body_le_x_m + xyz[0],
            ..self
        }
    }

    /// The same datum with x stretched by `factor` about the wing's own frame
    /// origin (a compressibility stretch: chordwise lengths and stations scale,
    /// the areas with them).
    #[must_use]
    pub fn stretch_x(self, factor: f64) -> Self {
        let mut root_le_m = self.root_le_m;
        root_le_m[0] *= factor;
        Self {
            root_le_m,
            root_chord_m: self.root_chord_m * factor,
            tip_le_x_m: self.tip_le_x_m * factor,
            tip_chord_m: self.tip_chord_m * factor,
            gross_area_m2: self.gross_area_m2 * factor,
            gross_mac_m: self.gross_mac_m * factor,
            gross_mac_le_x_m: self.gross_mac_le_x_m * factor,
            side_of_body_le_x_m: self.side_of_body_le_x_m * factor,
            side_of_body_chord_m: self.side_of_body_chord_m * factor,
            ..self
        }
    }

    /// The `x_nondim` chordwise points of the design root and of the straight-edge
    /// tip, given that point of the lofted tip section and its leading edge and chord.
    pub(super) fn sweep_line(
        &self,
        x_nondim: f64,
        lofted_tip: [f64; 3],
        lofted_tip_le: [f64; 3],
        lofted_tip_chord: f64,
    ) -> ([f64; 3], [f64; 3]) {
        let root = add3(
            self.root_le_m,
            scale3(self.root_chord_axis, x_nondim * self.root_chord_m),
        );
        // The tip's chord axis scaled from the lofted chord to the straight one.
        let axis_part = scale3(
            sub3(lofted_tip, lofted_tip_le),
            self.tip_chord_m / lofted_tip_chord,
        );
        let tip_le = [self.tip_le_x_m, lofted_tip_le[1], lofted_tip_le[2]];
        (root, add3(tip_le, axis_part))
    }
}

/// `Debug` as the derived form was, with the reference printed only when set,
/// so a wing without one formats (and hashes) exactly as before.
impl fmt::Debug for Wing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("Wing");
        debug
            .field("name", &self.name)
            .field("xsecs", &self.xsecs)
            .field("symmetric", &self.symmetric);
        if let Some(reference) = &self.reference {
            debug.field("reference", reference);
        }
        if let Some(datum) = &self.planform_datum {
            debug.field("planform_datum", datum);
        }
        if let Some(sections) = &self.reference_xsecs {
            debug.field("reference_xsecs", sections);
        }
        if self.tip_te_flare_m != 0.0 {
            debug.field("tip_te_flare_m", &self.tip_te_flare_m);
        }
        if self.twist_about_pitch_axis {
            debug.field("twist_about_pitch_axis", &true);
        }
        debug.finish()
    }
}

impl Wing {
    /// Leading edge and chord of the design root at the centreline: the
    /// [`PlanformDatum`]'s when the sections inside the body are the physical wing
    /// on its edge lines to the centreline, otherwise the first section's. The planform correlations
    /// (taper, gear and fairing stations) are stated on this station; integrals over
    /// the wing read the lofted sections or [`Self::carried_box`].
    pub fn design_root(&self) -> ([f64; 3], f64) {
        self.planform_datum.map_or_else(
            || (self.xsecs[0].xyz_le, self.xsecs[0].chord),
            |datum| (datum.root_le_m, datum.root_chord_m),
        )
    }

    /// This wing with the side-of-body section carried straight across the body,
    /// the shape of the centre wing box: between the spars it is a box of the
    /// side-of-body chord and station on the design twist line, whatever the loft
    /// does around it. Spar-box volumes, structure, the mass and exposed-wing
    /// integrals and the wing station of the tail arms and balance, which the buried
    /// planform must not move, read this; the lattice and the wing's own MAC and
    /// aerodynamic centre read the lofted [`Self::xsecs`]. A wing with no
    /// [`PlanformDatum`] is its own carried box.
    pub fn carried_box(&self) -> Cow<'_, Self> {
        let Some(datum) = &self.planform_datum else {
            return Cow::Borrowed(self);
        };
        let inboard_of_m = datum.side_of_body_y_m * (1.0 - 1.0e-9);
        let mut wing = self.clone();
        let sob_airfoil = self
            .xsecs
            .iter()
            .find(|xsec| {
                (xsec.xyz_le[1].abs() - datum.side_of_body_y_m).abs()
                    <= 1.0e-9 * datum.side_of_body_y_m
            })
            .map(|xsec| xsec.airfoil.clone());
        for xsec in &mut wing.xsecs {
            if xsec.xyz_le[1].abs() < inboard_of_m {
                if let Some(airfoil) = &sob_airfoil {
                    xsec.airfoil = airfoil.clone();
                }
                let t = xsec.xyz_le[1].abs() / datum.side_of_body_y_m;
                xsec.xyz_le[0] = datum.side_of_body_le_x_m;
                xsec.chord = datum.side_of_body_chord_m;
                xsec.twist = datum.root_twist_deg
                    + t * (datum.side_of_body_twist_deg - datum.root_twist_deg);
            }
        }
        Cow::Owned(wing)
    }

    /// Leading edge and chord of the box at the centreline: [`Self::carried_box`]'s
    /// first section, without building the rest of it.
    pub fn carried_root(&self) -> ([f64; 3], f64) {
        let [_, y_m, z_m] = self.xsecs[0].xyz_le;
        self.planform_datum.map_or_else(
            || (self.xsecs[0].xyz_le, self.xsecs[0].chord),
            |datum| {
                (
                    [datum.side_of_body_le_x_m, y_m, z_m],
                    datum.side_of_body_chord_m,
                )
            },
        )
    }

    /// The mean aerodynamic chord this wing's coefficients and `%MAC` are
    /// quoted against: the reference wing's when one is set, otherwise the
    /// drawn planform's [`Self::mean_aerodynamic_chord`].
    pub fn reference_mac(&self) -> f64 {
        self.reference.map_or_else(
            || self.reference_planform().mean_aerodynamic_chord(),
            |r| r.mac_m,
        )
    }

    /// Leading-edge x of the reference MAC: the reference wing's when one is
    /// set, otherwise the drawn planform's `aerodynamic_center(0.0)[0]`
    /// (the unfolded chord-weighted station the mass-layout code reads).
    pub fn reference_mac_le_x(&self) -> f64 {
        self.reference.map_or_else(
            || self.reference_planform().aerodynamic_center(0.0)[0],
            |r| r.mac_le_x_m,
        )
    }

    /// Leading-edge x of the reference MAC on the projected planform measure
    /// ([`Self::mac_station`]) unless a reference wing is set: the `%MAC`
    /// datum [`crate::aircraft::airplane::Airplane::mac_frame`] uses.
    pub fn reference_mac_station_le_x(&self) -> f64 {
        self.reference
            .map_or_else(|| self.mac_station().x_le_m, |r| r.mac_le_x_m)
    }
}
