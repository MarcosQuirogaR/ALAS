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
    // Both registered T-tails: the ATR 72-600 and the A400M.
    for name in ["ATR72-600", "A400M"] {
        attached_t_tail_follows_scaled_fin(name);
    }
}

fn attached_t_tail_follows_scaled_fin(name: &str) {
    let preset = presets::get(name).expect("T-tail preset");
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
    // The T-tails (ATR 72-600, A400M) attach the stabiliser to the fin tip.
    for preset in presets::registry()
        .iter()
        .filter(|p| !matches!(p.name, "ATR72-600" | "A400M"))
    {
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

#[test]
fn t_tail_root_follows_the_fin_scale_when_the_fin_is_resized() {
    let preset = presets::get("ATR72-600").expect("ATR preset");
    let mut geometry = preset.geometry.clone();
    geometry.empennage.vstab_scale_ratio = 1.25;
    let builder = AircraftBuilder::new(Some(geometry));
    let mut dv = preset.design_vector;
    dv.tail_scale = 0.9;
    let plane = builder.build(Some(&dv), false).expect("finite geometry");
    let h = &plane.wings[1].xsecs[0];
    let v = plane.wings[2].xsecs.last().expect("fin tip");
    for axis in 0..3 {
        assert!(
            (h.xyz_le[axis] - v.xyz_le[axis]).abs() < 1e-9,
            "axis {axis}"
        );
    }
}

/// Chord and area of a built fin's straight-edged trapezoid at and above the
/// configured root line `z_m`: the drawn fin the scale ratio resizes, before
/// the builder carries its edges to the body under the root.
fn at_root_line(fin: &alas_geom::aircraft::wing::Wing, z_m: f64) -> (f64, f64) {
    let (root, tip) = (&fin.xsecs[0], fin.xsecs.last().expect("fin tip"));
    let s = (z_m - root.xyz_le[2]) / (tip.xyz_le[2] - root.xyz_le[2]);
    let chord = root.chord + s * (tip.chord - root.chord);
    (chord, 0.5 * (chord + tip.chord) * (tip.xyz_le[2] - z_m))
}

#[test]
fn the_fin_ratio_scales_only_the_fin_area_quadratically() {
    let preset = presets::get("A320-200").expect("A320 preset");
    let mut dv = preset.design_vector;
    dv.tail_scale = 1.0;
    let base = AircraftBuilder::new(Some(preset.geometry.clone()));
    let (h0, v0) = base.build_empennage(&dv).expect("empennage");
    let mut geometry = preset.geometry.clone();
    geometry.empennage.vstab_scale_ratio = 1.2;
    let resized = AircraftBuilder::new(Some(geometry));
    let (h1, v1) = resized.build_empennage(&dv).expect("empennage");
    assert!((h1.unfolded_area() - h0.unfolded_area()).abs() < 1e-12);
    let z_m = preset.geometry.empennage.vstab_z_m;
    assert!((at_root_line(&v1, z_m).1 / at_root_line(&v0, z_m).1 - 1.44).abs() < 1e-9);
}

#[test]
fn the_unmeshed_empennage_has_the_area_and_centre_of_the_meshed_tails() {
    for preset in presets::registry() {
        let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
        let dv = preset.design_vector;
        let (h, v) = builder.build_empennage(&dv).expect("empennage");
        let plane = builder.build(Some(&dv), false).expect("preset geometry");
        // The tailplane's volume uses its projected area, the fin's its
        // unfolded planform (its projected area is zero by construction).
        assert!((h.reference_area() / plane.wings[1].reference_area() - 1.0).abs() < 1e-9);
        for (light, full) in [(&h, &plane.wings[1]), (&v, &plane.wings[2])] {
            assert!((light.unfolded_area() / full.unfolded_area() - 1.0).abs() < 1e-9);
            let (a, b) = (
                light.aerodynamic_center(0.25),
                full.aerodynamic_center(0.25),
            );
            assert!((a[0] - b[0]).abs() < 1e-9, "{}", preset.name);
        }
    }
}

#[test]
fn fin_scale_ratio_scales_only_the_fin_and_keeps_the_t_tail_attached() {
    for preset in presets::registry() {
        let mut dv = preset.design_vector;
        dv.tail_scale = 1.1;
        let base = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&dv), false)
            .expect("preset geometry");
        let mut geometry = preset.geometry.clone();
        let sizing = alas_config::TailSizing {
            tail_scale: dv.tail_scale,
            vstab_scale_ratio: 1.25,
        };
        sizing.apply_to(&mut geometry.empennage, &mut dv);
        assert_eq!(
            alas_config::TailSizing::of(&geometry.empennage, &dv),
            sizing
        );
        let sized = AircraftBuilder::new(Some(geometry))
            .build(Some(&dv), false)
            .expect("sized geometry");
        let z_m = preset.geometry.empennage.vstab_z_m;
        let fin_chord =
            |plane: &alas_geom::aircraft::airplane::Airplane| at_root_line(&plane.wings[2], z_m).0;
        assert!(
            (fin_chord(&sized) / fin_chord(&base) - 1.25).abs() < 1e-9,
            "{}",
            preset.name
        );
        if preset.name == "ATR72-600" {
            let h = &sized.wings[1].xsecs[0];
            let v = sized.wings[2].xsecs.last().expect("fin tip");
            assert!((h.xyz_le[2] - v.xyz_le[2]).abs() < 1e-9);
        } else {
            assert!(
                (sized.wings[1].xsecs[0].chord - base.wings[1].xsecs[0].chord).abs() < 1e-12,
                "{}",
                preset.name
            );
        }
    }
}
