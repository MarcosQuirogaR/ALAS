// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail placement options: bit-identity with them unset, and their effect.

// A failed expect in a test is the assertion failing on a registered preset.
#![allow(clippy::expect_used)]

use super::*;
use crate::aircraft::airplane::Airplane;
use crate::aircraft::wing::Wing;
use alas_config::presets;

/// FNV-1a over the bit patterns of every tail section coordinate, chord and
/// twist, so any change in the unset-option tail geometry changes the hash.
fn tail_hash(plane: &Airplane) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |value: f64| {
        for byte in value.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for wing in plane.wings.iter().skip(1) {
        for xsec in &wing.xsecs {
            xsec.xyz_le.iter().for_each(|v| eat(*v));
            eat(xsec.chord);
            eat(xsec.twist);
        }
    }
    hash
}

fn built(preset: &str, mutate: impl Fn(&mut alas_config::GeometryConfig)) -> Vec<Wing> {
    let entry = presets::get(preset).expect("preset");
    let mut geometry = entry.geometry.clone();
    geometry.wing.wingtip_device = None; // step B: the A320 now has a sharklet; these tests look at the tail
    mutate(&mut geometry);
    AircraftBuilder::new(Some(geometry))
        .build(Some(&entry.design_vector), false)
        .expect("builds")
        .wings
}

/// Tail hashes of every registered preset (loft cleared for the presets not
/// lofted at the baseline), produced by the build before the
/// placement options existed (base commit 1c951d9f).
const BASELINE: &[(&str, u64)] = &[
    ("AVE", 0x5334_ed27_5460_9dc5), // geo/tailfit 2026-10-11: direct tail planform fit (was 0x5b1b_ab93_8fa1_472a)
    ("A340-300", 0x9e84_639f_d747_cf96), // geo/close 2026-10-12 re-pin (tailfit 0x866f_6e5a_5f40_e1c4; pre-tailfit 0x314d_cf1a_02ac_9be5)
    ("A380-800", 0x4aee_0753_854b_8a6f), // geo/tailfit 2026-10-11: direct tail planform fit (was 0xd44c_2c36_07a0_bda5); unchanged by geo/close
    ("B787-9", 0xef30_ec27_cd65_b580), // geo/tailfit 2026-10-11: direct tail planform fit (was 0x86b4_6724_31ce_ad1d)
    // Step B group n: A320 hstab tip z, A220 fin tip LE, C919 tail chords and placement, ATR T-tail raise, fin and hstab re-pinned (old: A320 0x2c71003ac1013ba7,
    // A220 0x39c9500b1dfc5002, ATR 0xb8312d6fa4164a6a, C919 0x9c9118a841e7abf0).
    ("A320-200", 0x62de_0d1a_5d92_bcb4), // geo/tailfit 2026-10-11: direct tail planform fit (was 0xec5d_146f_e247_4560)
    ("A220-300", 0xf9c2_c794_893a_19f0), // geo/close 2026-10-12 re-pin (tailfit 0xb71c_56d9_e371_2ccf; pre-tailfit 0x5ae8_7865_e186_a760)
    // Re-pinned at integration step A: the preset body fields moved (D 2.77 -> 2.865 m, H 2.71 m,
    // tail_z 1.0 -> 0.64 m), so the generated tail cone under the T-tail fin seat moved.
    ("ATR72-600", 0x41e0_7af2_0f64_1d60), // geo/close 2026-10-12 re-pin (tailfit 0x3dae_3f75_2c65_119a; pre-tailfit 0x4c0b_49ca_9196_47cc)
    ("DC-10", 0x3b03_4695_5f35_1288), // geo/close 2026-10-12 re-pin (tailfit 0x3805_5648_7427_6ba4; pre-tailfit 0xcacc_6e73_bfa8_4603)
    ("E195-E2", 0x534d_02da_77bd_1781), // geo/tailfit 2026-10-11: direct tail planform fit (was 0xd1b2_e7d5_d6f7_3db7)
    ("C919", 0x1cab_9887_e8cc_096a), // geo/tailfit 2026-10-11: direct tail planform fit (was 0x7078_84e2_88be_741c)
    ("B747-400", 0x1882_1879_c77f_2078), // geo/tailfit 2026-10-11: direct tail planform fit (was 0x2407_2497_80a6_bfcb)
    // Re-pinned at integration step A: height_m None -> 5.26 m, cabin_z_m 0.17 m (crown kept).
    ("A400M", 0x6df0_db13_9bed_14c6), // geo/close 2026-10-12 re-pin (tailfit 0x84e2_25fe_529e_3677; pre-tailfit 0xc7bf_1fe4_6ef9_2504)
];

/// Pins of the presets whose tail bits differ off the MSVC math runtime.
const NON_WINDOWS_BASELINE: &[(&str, u64)] = &[
    ("B787-9", 0xd68d_c457_8f9d_9a26),
    ("A320-200", 0xcda1_0272_61c9_ccee),
    ("E195-E2", 0x8627_50e5_7e91_67c3),
];

/// Presets whose fuselage already came from a loft at the baseline commit.
const LOFTED_AT_BASELINE: &[&str] = &[
    "A220-300", "A320-200", "A340-300", "B787-9", "C919", "E195-E2",
];

#[test]
fn presets_keep_their_tail_bits_with_the_options_unset() {
    let mut seen = Vec::new();
    for entry in presets::registry() {
        // The baseline was taken at 1c951d9f, when six presets already built
        // from their lofts and the others from generated stations (the fin
        // seat follows the fuselage crown). The presets converted later keep
        // that old state here by clearing their loft: the invariant is that
        // the placement options at their defaults add nothing to the tail.
        let mut geometry = entry.geometry.clone();
        geometry.wing.wingtip_device = None; // step B: real devices are not part of the tail baseline
        if !LOFTED_AT_BASELINE.contains(&entry.name) {
            geometry.fuselage.loft = None;
        }
        let plane = AircraftBuilder::new(Some(geometry))
            .build(Some(&entry.design_vector), false)
            .expect("builds");
        assert_eq!(plane.wings.len(), 3, "{}", entry.name); // devices cleared above
        seen.push((entry.name, tail_hash(&plane)));
    }
    let mut expected: std::collections::BTreeMap<_, _> = BASELINE.iter().copied().collect();
    if !cfg!(windows) {
        // The pins above were taken with the MSVC math runtime. These presets
        // use transcendental functions whose last bit differs under glibc, so
        // they carry their own host-family pins (same geometry to rounding).
        expected.extend(NON_WINDOWS_BASELINE.iter().copied());
    }
    let moved: Vec<String> = seen
        .iter()
        .filter(|(name, hash)| expected.get(name) != Some(hash))
        .map(|(name, hash)| format!("{name} {hash:#x}"))
        .collect();
    assert!(moved.is_empty(), "tail bits moved: {moved:?}");
}

#[test]
fn an_explicit_fin_station_replaces_the_offset() {
    let plain = built("A320-200", |_| {});
    let entry = presets::get("A320-200").expect("preset");
    let g = &entry.geometry.empennage;
    let same = entry.design_vector.fuselage_length_m - g.vstab_offset_from_tail_m;
    let moved = built("A320-200", |geometry| {
        geometry.empennage.vstab_root_le_x_m = Some(same + 1.5);
    });
    let tip_x = |wings: &[Wing]| wings.last().expect("fin").xsecs.last().expect("tip").xyz_le[0];
    // The fin tip leading edge is the tip offset aft of the root-line
    // leading edge, so it moves by exactly the change in the station.
    assert!((tip_x(&moved) - tip_x(&plain) - 1.5).abs() < 1e-9);
    let again = built("A320-200", |geometry| {
        geometry.empennage.vstab_root_le_x_m = Some(same);
    });
    assert!((tip_x(&again) - tip_x(&plain)).abs() < 1e-9);
}

#[test]
fn a_drawn_hstab_root_station_is_met_and_the_edges_reach_the_centreline() {
    let station = 1.9;
    let entry = presets::get("A320-200").expect("preset");
    let g = entry.geometry.empennage.clone();
    let wings = built("A320-200", |geometry| {
        geometry.empennage.hstab_root_station_y_m = Some(station);
    });
    let tail = &wings[1];
    let root = &tail.xsecs[0];
    let tip = tail.xsecs.last().expect("tip");
    let ts = entry.design_vector.tail_scale;
    let plain = built("A320-200", |_| {});
    let plain_root = &plain[1].xsecs[0];
    // Interpolate the extended planform back to the drawn station.
    let s = (station * ts - root.xyz_le[1]) / (tip.xyz_le[1] - root.xyz_le[1]);
    let chord = root.chord + s * (tip.chord - root.chord);
    let le_x = root.xyz_le[0] + s * (tip.xyz_le[0] - root.xyz_le[0]);
    let le_z = root.xyz_le[2] + s * (tip.xyz_le[2] - root.xyz_le[2]);
    assert!((chord - g.hstab_root_chord_m * ts).abs() < 1e-9);
    assert!((le_x - plain_root.xyz_le[0]).abs() < 1e-9);
    assert!((le_z - plain_root.xyz_le[2]).abs() < 1e-9);
    // Tip unchanged; centreline chord larger on a tapered tail.
    assert!((tip.chord - g.hstab_tip_chord_m * ts).abs() < 1e-12);
    assert!(root.chord > chord);
    assert!(root.xyz_le[0] < plain_root.xyz_le[0]);
}

#[test]
fn a_hstab_root_station_outside_the_semispan_is_refused() {
    let entry = presets::get("A320-200").expect("preset");
    let mut geometry = entry.geometry.clone();
    geometry.empennage.hstab_root_station_y_m = Some(100.0);
    let result = AircraftBuilder::new(Some(geometry)).build(Some(&entry.design_vector), false);
    assert!(matches!(result, Err(BuildError::TailPlacement(_))));
}

#[test]
fn t_tails_seat_the_tailplane_root_on_the_fin_tip_at_any_tail_scale() {
    for preset in ["ATR72-600", "A400M"] {
        for (scale, ratio, shift, explicit) in [
            (1.0, 1.0, 0.0, false),
            (1.2, 0.9, 0.5, false),
            (0.8, 1.1, -0.4, true),
        ] {
            let entry = presets::get(preset).expect("preset");
            let mut design = entry.design_vector;
            design.tail_scale = scale;
            design.tail_x_shift_m = shift;
            let mut geometry = entry.geometry.clone();
            geometry.empennage.vstab_scale_ratio = ratio;
            if explicit {
                let e = &mut geometry.empennage;
                e.vstab_root_le_x_m = Some(design.fuselage_length_m - e.vstab_offset_from_tail_m);
                e.hstab_root_le_x_m = Some(design.fuselage_length_m - e.hstab_offset_from_tail_m);
            }
            let (hstab, vstab) = AircraftBuilder::new(Some(geometry))
                .build_empennage(&design)
                .expect("builds");
            let fin_tip = vstab.xsecs.last().expect("tip").xyz_le;
            let root = hstab.xsecs[0].xyz_le;
            // geo/close 2026-10-12: fixed offset from the fin tip (ATR 0.6 m, A400M 1.0 m below it in z; A400M 0.62 m forward in x)
            let e = &entry.geometry.empennage;
            let dx = e.vstab_offset_from_tail_m - e.hstab_offset_from_tail_m - e.vstab_tip_le_m.0;
            let dz = e.hstab_z_m - e.vstab_z_m - e.vstab_tip_le_m.2;
            assert!(
                (root[0] - fin_tip[0] - dx).abs() < 1e-9,
                "{preset} x {scale}"
            );
            assert!(
                (root[2] - fin_tip[2] - dz).abs() < 1e-9,
                "{preset} z {scale}"
            );
            assert!(root[1].abs() < 1e-12);
        }
    }
}

#[test]
fn the_dorsal_fillet_is_geometry_only_until_the_aero_flag_is_set() {
    let entry = presets::get("A320-200").expect("preset");
    let g = &entry.geometry.empennage;
    let length = entry.design_vector.fuselage_length_m;
    let x_fin = length - g.vstab_offset_from_tail_m;
    let x_start = x_fin - 4.0;
    let configure = |in_aero: bool| {
        let mut geometry = entry.geometry.clone();
        geometry.wing.wingtip_device = None; // step B: the A320 now has a sharklet (a fourth surface)
        geometry.empennage.vstab_dorsal_x_start_m = Some(x_start);
        geometry.empennage.vstab_dorsal_height_m = Some(1.5);
        geometry.empennage.vstab_dorsal_in_aero = in_aero;
        AircraftBuilder::new(Some(geometry))
    };
    let off = configure(false);
    let plane = off
        .build(Some(&entry.design_vector), false)
        .expect("builds");
    assert_eq!(plane.wings.len(), 3);
    let dorsal = off
        .build_dorsal_fin(&entry.design_vector)
        .expect("builds")
        .expect("configured");
    let root = &dorsal.xsecs[0];
    assert!((root.xyz_le[0] - x_start).abs() < 1e-12);
    assert!((root.xyz_le[2] - g.vstab_z_m).abs() < 1e-12);
    assert!((root.xyz_le[0] + root.chord - x_fin).abs() < 1e-9);
    let tip = dorsal.xsecs.last().expect("tip");
    assert!((tip.xyz_le[2] - g.vstab_z_m - 1.5).abs() < 1e-9);
    // The sharp tip's trailing edge lies on the fin leading edge line.
    let expected = x_fin + g.vstab_tip_le_m.0 * 1.5 / g.vstab_tip_le_m.2;
    assert!((tip.xyz_le[0] + tip.chord - expected).abs() < 1e-9);
    let on = configure(true);
    let wings = on
        .build(Some(&entry.design_vector), false)
        .expect("builds")
        .wings;
    assert_eq!(wings.len(), 4);
}

#[test]
fn a_half_configured_dorsal_is_refused() {
    let entry = presets::get("A320-200").expect("preset");
    let mut geometry = entry.geometry.clone();
    geometry.empennage.vstab_dorsal_height_m = Some(1.0);
    let builder = AircraftBuilder::new(Some(geometry));
    assert!(matches!(
        builder.build_dorsal_fin(&entry.design_vector),
        Err(BuildError::TailPlacement(_))
    ));
}
