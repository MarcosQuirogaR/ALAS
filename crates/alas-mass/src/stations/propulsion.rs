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

use alas_geom::aircraft::airplane::Airplane;

use super::ComponentStation;

/// One station per engine nacelle, at each nacelle's mid-length point
/// (x aft, y starboard, z up, m, geometry frame).
pub(super) fn propulsion_stations(plane: &Airplane) -> Vec<ComponentStation> {
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
                position_m: [start[0] + length * 0.5, start[1], start[2]],
                extent_m: [length, 2.0 * radius, 2.0 * radius],
                method: "nacelle mid-length",
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
        let stations = propulsion_stations(&plane);
        assert_eq!(stations.len(), 1);
        let [x, y, z] = stations[0].position_m;
        assert!((x - 11.65).abs() < 1.0e-12);
        assert_eq!((y, z), (4.0, -2.0));
        assert!((stations[0].extent_m[0] - 3.3).abs() < 1.0e-12);
        assert_eq!(stations[0].extent_m[1], 2.0);
    }
}
