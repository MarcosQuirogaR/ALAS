// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lumped ten-group coordinates read off the component stations.
//!
//! Every consumer of the legacy breakdown -- the trim anchor, the model CG
//! envelope, the figures -- reads one point per group. Deriving those points
//! from the same stations the item ledger places its rows at is what keeps
//! the two representations from disagreeing about where the aircraft
//! balances: the ledger's gear centroid is the mass-weighted nose and main
//! station, the lumped gear point is the same number.

use crate::breakdown::{MassBreakdown, MassCoordinates};

use super::ComponentStations;

/// Share of the landing-gear mass on the nose gear; the same split the
/// ledger builder uses, after the typical nose-gear static-load share
/// (Raymer, *Aircraft Design*, ch. 11).
pub const NOSE_GEAR_MASS_FRACTION: f64 = 0.15;

/// Vertical offset of the no-nacelle propulsion point below the wing
/// station, the legacy breakdown's convention for an engine drawn without a
/// nacelle body.
const NO_NACELLE_PROPULSION_DROP_M: f64 = 1.0;

impl ComponentStations {
    /// The centroid of the whole landing gear, nose and main together.
    pub fn gear_centroid_m(&self) -> [f64; 3] {
        let nose = self.nose_gear.position_m;
        let main = self.main_gear.position_m;
        let mut centroid = [0.0; 3];
        for axis in 0..3 {
            centroid[axis] =
                NOSE_GEAR_MASS_FRACTION * nose[axis] + (1.0 - NOSE_GEAR_MASS_FRACTION) * main[axis];
        }
        centroid
    }

    /// The mean of the propulsion units, or the legacy wing-station point
    /// when the aircraft was built without nacelle bodies.
    pub fn propulsion_centroid_m(&self) -> [f64; 3] {
        if self.propulsion_units.is_empty() {
            let wing = self.wing.position_m;
            return [wing[0], wing[1], wing[2] - NO_NACELLE_PROPULSION_DROP_M];
        }
        let count = self.propulsion_units.len() as f64;
        let mut centroid = [0.0; 3];
        for unit in &self.propulsion_units {
            for (axis, value) in centroid.iter_mut().enumerate() {
                *value += unit.position_m[axis] / count;
            }
        }
        centroid
    }

    /// The lumped coordinates the legacy breakdown consumers read.
    ///
    /// `payload_position_m` is the detailed layout's centre when one exists
    /// and the occupied-cabin fallback otherwise; `fuel_position_m` is the
    /// centroid of the fuel as it sits in the tanks at the analyzed load.
    /// The masses are read only to keep the signature honest about what the
    /// coordinates describe; each group point is independent of its mass.
    pub fn mass_coordinates(
        &self,
        _masses: &MassBreakdown,
        payload_position_m: [f64; 3],
        fuel_position_m: [f64; 3],
    ) -> MassCoordinates {
        MassCoordinates {
            wing: self.wing.position_m,
            h_stab: self.horizontal_tail.position_m,
            v_stab: self.vertical_tail.position_m,
            fuselage: self.fuselage.position_m,
            gear: self.gear_centroid_m(),
            propulsion: self.propulsion_centroid_m(),
            systems: self.systems.position_m,
            furnishings: self.furnishings.position_m,
            payload: payload_position_m,
            fuel: fuel_position_m,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stations::ComponentStation;

    fn station(x: f64, z: f64) -> ComponentStation {
        ComponentStation {
            position_m: [x, 0.0, z],
            extent_m: [1.0, 1.0, 1.0],
            method: "test",
        }
    }

    fn stations() -> ComponentStations {
        ComponentStations {
            wing: station(30.0, -1.0),
            horizontal_tail: station(60.0, 1.0),
            vertical_tail: station(62.0, 4.0),
            fuselage: station(32.0, 0.0),
            nose_gear: station(6.0, -3.0),
            main_gear: station(34.0, -3.0),
            propulsion_units: Vec::new(),
            systems: station(28.0, 0.0),
            furnishings: station(30.0, 0.0),
            operating_items: station(30.0, 0.0),
            payload_fallback: station(31.0, 0.0),
        }
    }

    #[test]
    fn the_gear_point_is_the_mass_weighted_nose_and_main_station() {
        let centroid = stations().gear_centroid_m();
        assert!((centroid[0] - (0.15 * 6.0 + 0.85 * 34.0)).abs() < 1.0e-12);
        assert_eq!(centroid[2], -3.0);
    }

    #[test]
    fn a_missing_nacelle_falls_back_to_the_legacy_wing_point() {
        let centroid = stations().propulsion_centroid_m();
        assert_eq!(centroid, [30.0, 0.0, -2.0]);
        let mut with_units = stations();
        with_units.propulsion_units = vec![station(24.0, -3.0), station(24.0, -3.0)];
        assert_eq!(with_units.propulsion_centroid_m(), [24.0, 0.0, -3.0]);
    }

    #[test]
    fn the_lumped_coordinates_carry_the_payload_and_fuel_points_given() {
        let masses = MassBreakdown {
            wing: 1.0,
            h_stab: 1.0,
            v_stab: 1.0,
            fuselage: 1.0,
            gear: 1.0,
            propulsion: 1.0,
            systems: 1.0,
            furnishings: 1.0,
            payload: 1.0,
            fuel: 1.0,
        };
        let coords = stations().mass_coordinates(&masses, [31.5, 0.0, 0.2], [29.0, 0.0, -0.5]);
        assert_eq!(coords.payload, [31.5, 0.0, 0.2]);
        assert_eq!(coords.fuel, [29.0, 0.0, -0.5]);
        assert_eq!(coords.h_stab, [60.0, 0.0, 1.0]);
    }
}
