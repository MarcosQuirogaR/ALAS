// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The manufacturer reference-wing override of a [`Wing`]: the area, span,
//! mean aerodynamic chord and MAC leading-edge station the aircraft quotes
//! its coefficients, wing loading and `%MAC` against when they are not those
//! of the drawn planform. The construction lives in `alas-config`
//! (`TransportPlanform::reference_trapezoid`); this module only carries and
//! serves the result. Wetted area, lattice panels and every integral over the
//! drawn lofted sections keep using the true planform.

use std::fmt;

use super::*;

/// The reference wing's quantities, projected onto the aircraft XY plane,
/// metres and square metres, x positive aft in the same frame as
/// [`WingXSec::xyz_le`].
#[derive(Debug, Clone, Copy, PartialEq)]
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
        debug.finish()
    }
}

impl Wing {
    /// The mean aerodynamic chord this wing's coefficients and `%MAC` are
    /// quoted against: the reference wing's when one is set, otherwise the
    /// drawn planform's [`Self::mean_aerodynamic_chord`].
    pub fn reference_mac(&self) -> f64 {
        self.reference
            .map_or_else(|| self.mean_aerodynamic_chord(), |r| r.mac_m)
    }

    /// Leading-edge x of the reference MAC: the reference wing's when one is
    /// set, otherwise the drawn planform's `aerodynamic_center(0.0)[0]`
    /// (the unfolded chord-weighted station the mass-layout code reads).
    pub fn reference_mac_le_x(&self) -> f64 {
        self.reference
            .map_or_else(|| self.aerodynamic_center(0.0)[0], |r| r.mac_le_x_m)
    }

    /// Leading-edge x of the reference MAC on the projected planform measure
    /// ([`Self::mac_station`]) unless a reference wing is set: the `%MAC`
    /// datum [`crate::aircraft::airplane::Airplane::mac_frame`] uses.
    pub fn reference_mac_station_le_x(&self) -> f64 {
        self.reference
            .map_or_else(|| self.mac_station().x_le_m, |r| r.mac_le_x_m)
    }
}
