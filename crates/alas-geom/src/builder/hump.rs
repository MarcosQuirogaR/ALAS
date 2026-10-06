// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The upper-deck hump stations of the fuselage loft.
//!
//! The twenty generated stations (which `generated_sections` overrides
//! index) carry the hump through `FuselageConfig::raise_crown`; the cabin
//! has only two of them, so the hump fairings get their own stations from
//! `FuselageConfig::hump_loft_stations`. They are not indexed by the
//! override vector and are skipped where a generated or custom station
//! already sits, so a body without a hump keeps exactly its stations.

use super::{AircraftBuilder, BuildError};
use crate::aircraft::fuselage::FuselageXSec;

impl AircraftBuilder {
    /// Insert the hump loft stations, keeping the nose-to-tail order.
    pub(super) fn append_hump_fuselage_sections(
        &self,
        fuselage_length_m: f64,
        stations: &mut Vec<FuselageXSec>,
    ) -> Result<(), BuildError> {
        let extra = self.geometry.fuselage.hump_loft_stations(fuselage_length_m);
        if extra.is_empty() {
            return Ok(());
        }
        let tolerance = 1.0e-6 * fuselage_length_m.abs().max(1.0);
        for station in extra {
            if stations
                .iter()
                .any(|existing| (existing.xyz_c[0] - station.x_m).abs() <= tolerance)
            {
                continue;
            }
            stations.push(FuselageXSec::new(
                [station.x_m, 0.0, station.z_m],
                None,
                Some(station.width_m),
                Some(station.height_m),
                station.shape,
            )?);
        }
        stations.sort_by(|left, right| left.xyz_c[0].total_cmp(&right.xyz_c[0]));
        Ok(())
    }
}

// A failed expect in a test is the assertion failing on a registered preset.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{presets, FuselageConfig, GeometryConfig};

    fn fuselage(geometry: &GeometryConfig, preset: &str) -> crate::aircraft::fuselage::Fuselage {
        let design = presets::get(preset).expect("preset").design_vector;
        AircraftBuilder::new(Some(geometry.clone()))
            .build(Some(&design), false)
            .expect("builds")
            .fuselages
            .remove(0)
    }

    #[test]
    fn a_body_without_a_hump_keeps_its_twenty_stations_bit_for_bit() {
        for entry in presets::registry() {
            let (preset, geometry) = (entry.name, &entry.geometry);
            if geometry.fuselage.upper_deck_hump().is_some() {
                continue;
            }
            let plain = fuselage(geometry, preset);
            assert_eq!(
                plain.xsecs.len(),
                20 + geometry.fuselage.custom_sections.len(),
                "{preset}"
            );
            // An incomplete hump (height only) is no hump: identical bits.
            let mut partial = geometry.clone();
            partial.fuselage.hump_height_m = Some(1.0);
            partial.fuselage.upper_deck_floor_height_m = Some(2.7);
            assert_eq!(fuselage(&partial, preset), plain, "{preset}");
        }
    }

    #[test]
    fn the_b747_hump_raises_the_crown_and_leaves_the_keel() {
        let config = &presets::get("B747-400").expect("preset").geometry;
        let hump = config
            .fuselage
            .upper_deck_hump()
            .expect("the B747-400 preset declares its hump");
        let humped = fuselage(config, "B747-400");
        let mut flat_config = config.clone();
        flat_config.fuselage = FuselageConfig {
            hump_height_m: None,
            ..config.fuselage.clone()
        };
        let flat = fuselage(&flat_config, "B747-400");
        assert_eq!(flat.xsecs.len(), 20);
        assert!(humped.xsecs.len() > 20);
        let keel = |x: &FuselageXSec| x.xyz_c[2] - 0.5 * x.height;
        let crown = |x: &FuselageXSec| x.xyz_c[2] + 0.5 * x.height;
        // At every generated station the keel and width hold and the crown
        // rises by exactly the hump law.
        for station in &flat.xsecs {
            let x = station.xyz_c[0];
            let same = humped
                .xsecs
                .iter()
                .find(|h| (h.xyz_c[0] - x).abs() < 1e-12)
                .expect("generated station kept");
            assert!((keel(same) - keel(station)).abs() < 1e-12, "keel at {x}");
            assert_eq!(same.width, station.width);
            let rise = hump.crown_rise_m(x);
            assert!(
                (crown(same) - crown(station) - rise).abs() < 1e-12,
                "crown at {x}"
            );
        }
        let top = humped.xsecs.iter().map(crown).fold(f64::MIN, f64::max);
        let main_top = flat.xsecs.iter().map(crown).fold(f64::MIN, f64::max);
        assert!((top - main_top - hump.height_m).abs() < 1e-9);
        assert!(humped
            .xsecs
            .windows(2)
            .all(|pair| pair[1].xyz_c[0] > pair[0].xyz_c[0]));
        assert!(humped.area_wetted() > flat.area_wetted());
    }
}
