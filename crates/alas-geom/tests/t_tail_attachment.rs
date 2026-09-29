// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Nominal and scaled fin-tip attachments in product aircraft geometry.

// A test asserts on values it built here, so a failed expect is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::expect_used)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;

#[test]
fn attached_t_tail_follows_scaled_fin_in_product_geometry() {
    let preset = presets::get("ATR72-600").expect("ATR preset");
    let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
    for scale in [0.7, 1.0, 1.3] {
        let mut dv = preset.design_vector;
        dv.tail_scale = scale;
        dv.tail_x_shift_m = 0.4;
        dv.fuselage_length_m += 0.5;
        let plane = builder.build(Some(&dv), false).expect("finite geometry");
        let h = &plane.wings[1].xsecs[0];
        let v = plane.wings[2].xsecs.last().expect("fin tip");
        for axis in 0..3 {
            assert!((h.xyz_le[axis] - v.xyz_le[axis]).abs() < 1e-9);
        }
        assert!((h.chord - v.chord).abs() < 1e-9);
        assert!(h.xyz_le.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn independent_tails_keep_their_existing_root_placement() {
    for preset in presets::registry().iter().filter(|p| p.name != "ATR72-600") {
        let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
        let mut dv = preset.design_vector;
        dv.tail_scale = 1.3;
        let plane = builder.build(Some(&dv), false).expect("preset geometry");
        let root = plane.wings[1].xsecs[0].xyz_le;
        let e = &preset.geometry.empennage;
        assert!(
            (root[0] - (dv.fuselage_length_m - e.hstab_offset_from_tail_m + dv.tail_x_shift_m))
                .abs()
                < 1e-9,
            "{}",
            preset.name
        );
        assert!((root[2] - e.hstab_z_m).abs() < 1e-9, "{}", preset.name);
    }
}
