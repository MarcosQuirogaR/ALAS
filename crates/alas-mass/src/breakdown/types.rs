// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component mass and centroid records shared by the buildup and the coordinate model.

use alas_config::StructuresConfig;

use super::{
    FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};

/// Every `Wing::aerodynamic_center` call in the mass buildup reads the
/// quarter-chord point.
pub(super) const AERODYNAMIC_CENTER_CHORD_FRACTION: f64 = 0.25;

/// Which main-wing mass-coordinate model an analysis uses.
///
/// [`Self::ReferenceCompatibility`] is the reference-aircraft coordinate
/// convention used by reference-adaptation and screening modes.
/// [`Self::StructuralWingbox`] places the main-wing point from the configured
/// structural first moment and uses a cabin-centered planning payload rather
/// than the forward-load convention. A resolved payload layout supersedes
/// either fallback.
#[derive(Debug, Clone, Copy)]
pub enum MassCoordinateModel<'a> {
    /// Reference-aircraft coordinate behavior.
    ReferenceCompatibility,
    /// Geometry- and structure-derived main-wing mass coordinate.
    StructuralWingbox(&'a StructuresConfig),
}

/// The mass of each primary component, in kg, with one field per canonical
/// component name (see the module doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    /// [`WING`]'s mass.
    pub wing: f64,
    /// [`H_STAB`]'s mass.
    pub h_stab: f64,
    /// [`V_STAB`]'s mass.
    pub v_stab: f64,
    /// [`FUSELAGE`]'s mass.
    pub fuselage: f64,
    /// [`GEAR`]'s mass.
    pub gear: f64,
    /// [`PROPULSION`]'s mass.
    pub propulsion: f64,
    /// [`SYSTEMS`]'s mass.
    pub systems: f64,
    /// [`FURNISHINGS`]'s mass.
    pub furnishings: f64,
    /// [`PAYLOAD`]'s mass.
    pub payload: f64,
    /// Signed [`FUEL`] closure, `MTOW - MZFW`, in kg.
    ///
    /// Negative values are retained so sizing and optimization callers can
    /// diagnose an overweight candidate. Use
    /// [`Self::physical_fuel_mass_kg`] before treating this value as a load or
    /// forming a mass moment.
    pub fuel: f64,
}

impl MassBreakdown {
    /// Signed MTOW-closure remainder, in kg.
    ///
    /// This diagnostic preserves negative closure for callers that must
    /// detect `MZFW > MTOW`; it does not assert that the value is a physical
    /// fuel load.
    pub fn signed_fuel_closure_kg(&self) -> f64 {
        self.fuel
    }

    /// Physically admissible fuel load, in kg.
    ///
    /// A finite, nonnegative closure is a usable mass value. Negative and
    /// non-finite closures return `None` so they cannot silently become a
    /// negative fuel mass or moment while remaining available through
    /// [`Self::signed_fuel_closure_kg`] for diagnostics.
    pub fn physical_fuel_mass_kg(&self) -> Option<f64> {
        (self.fuel.is_finite() && self.fuel >= 0.0).then_some(self.fuel)
    }

    /// Every component paired with its canonical name, in a fixed order: the
    /// generic iteration [`super::calculate_physical_cg`] and
    /// [`super::OEW_KEYS`]'s summation need.
    pub fn as_pairs(&self) -> [(&'static str, f64); 10] {
        [
            (WING, self.wing),
            (H_STAB, self.h_stab),
            (V_STAB, self.v_stab),
            (FUSELAGE, self.fuselage),
            (GEAR, self.gear),
            (PROPULSION, self.propulsion),
            (SYSTEMS, self.systems),
            (FURNISHINGS, self.furnishings),
            (PAYLOAD, self.payload),
            (FUEL, self.fuel),
        ]
    }

    /// The mass named `name`, or `None` if it is not one of the ten canonical
    /// components, for a caller (such as the [`super::OEW_KEYS`] summation)
    /// that only has the name.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.as_pairs()
            .into_iter()
            .find(|&(candidate, _)| candidate == name)
            .map(|(_, mass)| mass)
    }
}

/// The `[x, y, z]` centroid of each primary component, in meters, with one
/// field per canonical component name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassCoordinates {
    /// [`WING`]'s centroid.
    pub wing: [f64; 3],
    /// [`H_STAB`]'s centroid.
    pub h_stab: [f64; 3],
    /// [`V_STAB`]'s centroid.
    pub v_stab: [f64; 3],
    /// [`FUSELAGE`]'s centroid.
    pub fuselage: [f64; 3],
    /// [`GEAR`]'s centroid.
    pub gear: [f64; 3],
    /// [`PROPULSION`]'s centroid.
    pub propulsion: [f64; 3],
    /// [`SYSTEMS`]'s centroid.
    pub systems: [f64; 3],
    /// [`FURNISHINGS`]'s centroid.
    pub furnishings: [f64; 3],
    /// [`PAYLOAD`]'s centroid.
    pub payload: [f64; 3],
    /// [`FUEL`]'s centroid.
    pub fuel: [f64; 3],
}

impl MassCoordinates {
    /// Every component's centroid paired with its canonical name, in the same
    /// order [`MassBreakdown::as_pairs`] uses.
    pub fn as_pairs(&self) -> [(&'static str, [f64; 3]); 10] {
        [
            (WING, self.wing),
            (H_STAB, self.h_stab),
            (V_STAB, self.v_stab),
            (FUSELAGE, self.fuselage),
            (GEAR, self.gear),
            (PROPULSION, self.propulsion),
            (SYSTEMS, self.systems),
            (FURNISHINGS, self.furnishings),
            (PAYLOAD, self.payload),
            (FUEL, self.fuel),
        ]
    }
}

/// The three payload-layout attributes `run_mass_analysis` reads. A small
/// local type keeps this crate independent of the payload crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadLayoutSummary {
    /// The layout's total mass, kg.
    pub total_mass: f64,
    /// The layout's longitudinal centre of gravity, m.
    pub cg_x: f64,
    /// The layout's lateral centre of gravity, m.
    pub cg_y: f64,
}
