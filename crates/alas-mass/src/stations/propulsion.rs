// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Engine placement: one station per built nacelle body.
//!
//! The rule is the nacelle mid-length point. The one published engine centre
//! of gravity at hand does not support a different general rule: EASA TCDS
//! E.003 (CFM56-5B/-5C), section 4, gives the bare-engine centre of gravity
//! (engine only: no cowls, reverser or inlet) at about 0.47-0.48 of the
//! 2,599.7 mm length from the fan-case forward flange to the LP-turbine aft
//! flange; the digits in the text layer are not fully legible. Locating it
//! needs the fan-flange station inside the nacelle, i.e. an inlet length,
//! which no engine record carries. On the 3.3 m CFM56-5B record nacelle the
//! bare engine would sit at or forward of mid-length only for an inlet
//! shorter than about 0.43 m (0.25 fan diameters), which no turbofan inlet
//! is, so the sourced fraction could only move the engine aft of this point.
//! No unsourced inlet length is added.
//!
//! # Tractor turboprops
//!
//! A tractor turboprop nacelle is not an engine pod. Its first third is the
//! spinner, propeller hub and reduction gearbox; the engine core sits behind
//! them; and the aft part is exhaust and gear-bay fairing that carries almost
//! no mass but sets the nacelle length. A mid-length rule therefore moves the
//! engine group aft whenever the drawn fairing is lengthened, with no change
//! in where any engine or propeller mass is. For the ATR 72-600 the nacelle
//! runs from the spinner tip at 8.43 m to 14.63 m aft of the nose; its
//! mid-length (11.53 m) is 1.5 m behind the engine-removal arm that the ATR
//! Aircraft Recovery Manual implies, and it put 2.5 t of propulsion group at
//! the wing mid-chord of a high-wing aeroplane whose engines sit ahead of the
//! leading edge.
//!
//! The rule used for a turboprop is the class II nacelle rule of Jenkinson,
//! *Civil Jet Aircraft Design*, chapter 7, "Position of mass components"
//! (book p.148, PDF p.163): "Nacelles: 40% nacelle length from nose". Evidence that it is
//! the right side of mid-length, read on the pages: the ATR 72 Aircraft
//! Recovery Manual p.129 worked example removes one 800 kg engine from 13,700
//! kg at 27 %MAC and leaves 12,900 kg at 32 %MAC, which puts the engine at
//! 10.0 m from the nose (+-0.3 m from the rounding of the example); the 40 %
//! point of the 6.2 m built ATR nacelle is 10.91 m; and JCAB TCDS No. 75 (as cited
//! in the ATR preset) places the engine oil tank at 9.83 m. The ARM example
//! is illustrative, not a certified datum, and the 40 % rule is a statistical
//! class value, not an ATR calibration; the A400M nacelle takes the same
//! fraction. Turbofan nacelles keep the mid-length rule above, whose TCDS
//! basis is specific to a fan engine, so every turbofan preset is unchanged.

use alas_geom::aircraft::airplane::Airplane;

use super::ComponentStation;

/// Fraction of a turbofan nacelle's length, from the inlet, at which its mass
/// is placed.
pub(super) const TURBOFAN_NACELLE_FRACTION: f64 = 0.5;

/// Fraction of a tractor-turboprop nacelle's length, from the spinner tip, at
/// which its mass is placed: Jenkinson, *Civil Jet Aircraft Design*, ch. 7
/// "Position of mass components" (book p.148, PDF p.163), "Nacelles: 40% nacelle length
/// from nose".
pub(super) const TURBOPROP_NACELLE_FRACTION: f64 = 0.40;

/// One station per engine nacelle, at the class fraction of each nacelle's
/// length from its nose (x aft, y starboard, z up, m, geometry frame):
/// mid-length for a turbofan, 40 % for a tractor turboprop.
pub(super) fn propulsion_stations(plane: &Airplane, turboprop: bool) -> Vec<ComponentStation> {
    let (fraction, method) = if turboprop {
        (
            TURBOPROP_NACELLE_FRACTION,
            "turboprop nacelle 40% of length from the spinner tip",
        )
    } else {
        (TURBOFAN_NACELLE_FRACTION, "nacelle mid-length")
    };
    plane
        .fuselages
        .iter()
        .filter(|fuselage| fuselage.name.contains("Nacelle"))
        .map(|nacelle| {
            let start = nacelle.xsecs.first().map_or([0.0; 3], |xsec| xsec.xyz_c);
            let end_x = nacelle.xsecs.last().map_or(start[0], |xsec| xsec.xyz_c[0]);
            let length = end_x - start[0];
            let radius = nacelle
                .xsecs
                .iter()
                .map(|xsec| xsec.width.max(xsec.height) / 2.0)
                .fold(0.0, f64::max);
            ComponentStation {
                position_m: [start[0] + length * fraction, start[1], start[2]],
                extent_m: [length, 2.0 * radius, 2.0 * radius],
                method,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};

    fn section(x: f64, width: f64) -> FuselageXSec {
        FuselageXSec {
            xyz_c: [x, 4.0, -2.0],
            width,
            height: width,
            shape: 2.0,
        }
    }

    #[test]
    fn a_nacelle_is_placed_at_its_mid_length_and_non_nacelles_are_ignored() {
        // Hand check: inlet at x = 10.0 m, exit at 13.3 m, so the station is
        // 10.0 + 0.5 * 3.3 = 11.65 m at the inlet's own y and z; the widest
        // section is 2.0 m, so the radius extent is 2.0 m.
        let nacelle = Fuselage::new(
            "Nacelle R",
            vec![section(10.0, 1.2), section(11.0, 2.0), section(13.3, 0.8)],
        );
        let body = Fuselage::new("Fuselage", vec![section(0.0, 4.0), section(37.0, 4.0)]);
        let plane = Airplane {
            name: "fixture".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: vec![body, nacelle],
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let stations = propulsion_stations(&plane, false);
        assert_eq!(stations.len(), 1);
        let [x, y, z] = stations[0].position_m;
        assert!((x - 11.65).abs() < 1.0e-12);
        assert_eq!((y, z), (4.0, -2.0));
        assert!((stations[0].extent_m[0] - 3.3).abs() < 1.0e-12);
        assert_eq!(stations[0].extent_m[1], 2.0);
    }

    #[test]
    fn a_turboprop_nacelle_is_placed_at_forty_percent_and_ignores_the_aft_fairing() {
        // Hand check: spinner tip at 10.0 m, 40 % of 6.0 m is 12.4 m. Making
        // the drawn aft fairing 1.0 m longer moves the station by the
        // fraction of that length (0.4 m), where mid-length would move it
        // 0.5 m: the engine group is anchored to the nose of the nacelle.
        let body = Fuselage::new("Fuselage", vec![section(0.0, 4.0), section(37.0, 4.0)]);
        let plane_with = |end: f64| Airplane {
            name: "fixture".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: vec![
                body.clone(),
                Fuselage::new("Nacelle R", vec![section(10.0, 1.2), section(end, 0.8)]),
            ],
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let short = propulsion_stations(&plane_with(16.0), true);
        assert!((short[0].position_m[0] - 12.4).abs() < 1.0e-12);
        let long = propulsion_stations(&plane_with(17.0), true);
        assert!((long[0].position_m[0] - 12.8).abs() < 1.0e-12);
        let fan = propulsion_stations(&plane_with(16.0), false);
        assert!((fan[0].position_m[0] - 13.0).abs() < 1.0e-12);
    }
}
