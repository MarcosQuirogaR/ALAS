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
    geometry.wing.wingtip_device = None; // the A320 has a sharklet; these tests look at the tail
    mutate(&mut geometry);
    AircraftBuilder::new(Some(geometry))
        .build(Some(&entry.design_vector), false)
        .expect("builds")
        .wings
}

/// Tail hashes of every registered preset (loft cleared for the presets not
/// lofted at the baseline), produced by the build before the
/// placement options existed.
const BASELINE: &[(&str, u64)] = &[
    ("AVE", 0x5b5c9f4fde9ff701),
    ("A340-300", 0x5365bc533bb7c9f9),
    ("A380-800", 0xb6615fc403656ac),
    ("B787-9", 0x32e98267b8a55a4e),
    ("A320-200", 0x22dab9584270a80),
    ("A220-300", 0x3651c2cc4d856290),
    ("ATR72-600", 0x6e8d90d252efe38a),
    ("DC-10", 0xa74532d88c3840a9),
    ("E195-E2", 0x6a2060aab915bcbe),
    ("C919", 0xb673b1f861cece8c),
    ("B747-400", 0xd90e13082f2cfa4a),
    ("A400M", 0x82c76432864dcbfe),
    ("A350-900", 0x555181edf90b5b1),
    ("LJ60", 0x90198fd6b8409b62),
    ("C295", 0xfffe8a00f250cb41),
];

/// Pins of the presets whose tail bits differ off the MSVC math runtime.
const NON_WINDOWS_BASELINE: &[(&str, u64)] = &[
    // Pins for Linux (glibc): the tail-cone refit and shoulder offset differ from the Windows bits for the A320 and A220; the B787-9 and E195-E2 tail bits equal the Windows pins.
    ("A320-200", 0xa26a_18dd_e8e5_1981),
    ("A220-300", 0x3367_e7b2_1c41_911f),
];

/// Presets whose fuselage already came from a loft at the baseline commit.
const LOFTED_AT_BASELINE: &[&str] = &[
    "A220-300", "A320-200", "A340-300", "B787-9", "C919", "E195-E2",
];

#[test]
fn presets_keep_their_tail_bits_with_the_options_unset() {
    let mut seen = Vec::new();
    for entry in presets::registry() {
        // The baseline was taken when six presets already built
        // from their lofts and the others from generated stations (the fin
        // seat follows the fuselage crown). The presets converted later keep
        // that old state here by clearing their loft: the invariant is that
        // the placement options at their defaults add nothing to the tail.
        let mut geometry = entry.geometry.clone();
        geometry.wing.wingtip_device = None; // real devices are not part of the tail baseline
        if !LOFTED_AT_BASELINE.contains(&entry.name) {
            geometry.fuselage.loft = None;
        }
        let plane = AircraftBuilder::new(Some(geometry))
            .build(Some(&entry.design_vector), false)
            .expect("builds");
        // Devices are cleared above; the surfaces beyond the wing, tailplane and fin are the tail
        // fixtures product geometry always builds (dorsal fillet, pod struts and fairing, ventral
        // fin).
        let names: Vec<&str> = plane.wings.iter().map(|wing| wing.name.as_str()).collect();
        assert_eq!(
            names[..3],
            ["Main Wing", "Horizontal Stabilizer", "Vertical Stabilizer"],
            "{}",
            entry.name
        );
        assert!(
            names[3..].iter().all(|name| {
                name.starts_with("Dorsal Fin")
                    || name.starts_with("Centre Pod Strut")
                    || name.starts_with("Fin Aft Fairing")
                    || name.starts_with("Ventral Fin")
            }),
            "{}: {names:?}",
            entry.name
        );
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
    // The built A320 tailplane carries edge stations (a root fillet and a rounded tip),
    // so the section polyline is read at the drawn station, not between its end
    // sections.
    let tail = &wings[1];
    let root = &tail.xsecs[0];
    let tip = tail.xsecs.last().expect("tip");
    let ts = entry.design_vector.tail_scale;
    let plain = built("A320-200", |_| {});
    let plain_root = &plain[1].xsecs[0];
    let at = |wing: &Wing, y: f64| {
        let pair = wing
            .xsecs
            .windows(2)
            .find(|pair| y <= pair[1].xyz_le[1])
            .expect("inside the span");
        let t = (y - pair[0].xyz_le[1]) / (pair[1].xyz_le[1] - pair[0].xyz_le[1]);
        let lerp = |a: f64, b: f64| a + t * (b - a);
        (
            lerp(pair[0].chord, pair[1].chord),
            lerp(pair[0].xyz_le[0], pair[1].xyz_le[0]),
            lerp(pair[0].xyz_le[2], pair[1].xyz_le[2]),
        )
    };
    let y = station * ts;
    let (chord, le_x, le_z) = at(tail, y);
    // The drawn station keeps its drawn chord and position wherever the root is extended to.
    assert!((chord - g.hstab_root_chord_m * ts).abs() < 1e-9, "{chord}");
    assert!(
        (le_x - plain_root.xyz_le[0]).abs() < 1e-9,
        "{le_x} vs {}",
        plain_root.xyz_le[0]
    );
    assert!((le_z - plain_root.xyz_le[2]).abs() < 1e-9);
    // Tip unchanged; centreline chord larger on a tapered tail.
    assert!((tip.chord - plain[1].xsecs.last().expect("tip").chord).abs() < 1e-12);
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
            // Fixed offset from the fin tip (ATR 0.6 m, A400M 1.0 m below it in z; A400M 0.62 m forward in x)
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
fn the_dorsal_fillet_is_built_and_stays_out_of_the_lattice_until_the_aero_flag_is_set() {
    let entry = presets::get("A320-200").expect("preset");
    let g = &entry.geometry.empennage;
    let length = entry.design_vector.fuselage_length_m;
    let x_fin = length - g.vstab_offset_from_tail_m;
    let configure = |in_aero: bool| {
        let mut geometry = entry.geometry.clone();
        geometry.wing.wingtip_device = None; // the A320 has a sharklet (a fourth surface)
        geometry.empennage.vstab_dorsal_length_m = Some(4.0);
        geometry.empennage.vstab_dorsal_height_m = Some(1.5);
        geometry.empennage.vstab_dorsal_in_aero = in_aero;
        AircraftBuilder::new(Some(geometry))
    };
    let off = configure(false);
    let plane = off
        .build(Some(&entry.design_vector), false)
        .expect("builds");
    assert_eq!(plane.wings.len(), 4);
    assert!(plane.wings[3].is_dorsal_fin() && plane.wings[3].is_geometry_only());
    let dorsal = off
        .build_dorsal_fin(&entry.design_vector)
        .expect("builds")
        .expect("configured");
    let skin = dorsal.reference_planform();
    let tip = skin.xsecs.last().expect("tip");
    assert!((tip.xyz_le[2] - g.vstab_z_m - 1.5).abs() < 1e-9);
    // The exposed tip's trailing edge lies on the built fin leading edge.
    let z_tip = tip.xyz_le[2];
    let fin_sections = &plane.wings[2].xsecs;
    let above = fin_sections
        .iter()
        .position(|xsec| z_tip <= xsec.xyz_le[2])
        .expect("under the tip");
    let (low, high) = (&fin_sections[above - 1], &fin_sections[above]);
    let expected = low.xyz_le[0]
        + (z_tip - low.xyz_le[2]) / (high.xyz_le[2] - low.xyz_le[2])
            * (high.xyz_le[0] - low.xyz_le[0]);
    assert!(expected > x_fin);
    assert!((tip.xyz_le[0] + tip.chord - expected).abs() < 1e-6);
    // The wetted input is the exposed part, not the run inside the fin.
    assert!(dorsal.skin_planform().unfolded_area() < 0.8 * dorsal.unfolded_area());
    let on = configure(true);
    let wings = on
        .build(Some(&entry.design_vector), false)
        .expect("builds")
        .wings;
    assert_eq!(wings.len(), 4);
    assert!(wings[3].is_dorsal_fin() && !wings[3].is_geometry_only());
    // In the lattice it stays the exposed plate, ending on the fin leading edge.
    assert!(wings[3].reference_xsecs.is_none());
    let tip = wings[3].xsecs.last().expect("tip");
    assert!((tip.xyz_le[0] + tip.chord - expected).abs() < 1e-6);
}

/// `(x, half thickness)` of a section's upper surface, leading edge to trailing edge.
fn half_thickness(section: &crate::aircraft::wing::WingXSec) -> Vec<(f64, f64)> {
    let mut upper = section.airfoil.upper_coordinates().to_vec();
    upper.reverse();
    upper
        .into_iter()
        .map(|(x, y)| (section.xyz_le[0] + x * section.chord, y * section.chord))
        .collect()
}

#[test]
fn a_built_dorsal_stands_inside_the_fin_aft_of_its_thickest_station_and_covers_its_nose() {
    for preset in ["A320-200", "ATR72-600", "C295", "B747-400"] {
        let entry = presets::get(preset).expect("preset");
        let builder = AircraftBuilder::new(Some(entry.geometry.clone()));
        let plane = builder
            .build(Some(&entry.design_vector), false)
            .expect("builds");
        let fin = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Vertical Stabilizer")
            .expect("fin");
        let dorsal = plane
            .wings
            .iter()
            .find(|wing| wing.is_dorsal_fin())
            .expect("dorsal");
        for section in &dorsal.xsecs {
            let z = section.xyz_le[2];
            let upper = fin
                .xsecs
                .iter()
                .position(|xsec| z <= xsec.xyz_le[2])
                .expect("under the tip")
                .max(1);
            let (a, b) = (&fin.xsecs[upper - 1], &fin.xsecs[upper]);
            let s = ((z - a.xyz_le[2]) / (b.xyz_le[2] - a.xyz_le[2])).clamp(0.0, 1.0);
            let (fin_le, fin_chord) = (
                a.xyz_le[0] + s * (b.xyz_le[0] - a.xyz_le[0]),
                a.chord + s * (b.chord - a.chord),
            );
            // Ends inside the fin, thin at its own leading edge, ahead of or on the fin's.
            assert!(section.xyz_le[0] <= fin_le + 1e-9, "{preset} z {z}");
            assert!(
                section.xyz_le[0] + section.chord < fin_le + fin_chord,
                "{preset} z {z}"
            );
            let surface = half_thickness(section);
            let at_fin = |x_over_c: f64| {
                let thick = |w: &crate::aircraft::wing::WingXSec| {
                    w.chord * w.airfoil.local_thickness(&[x_over_c])[0] / 2.0
                };
                (1.0 - s) * thick(a) + s * thick(b)
            };
            for &(x, half) in surface
                .iter()
                .filter(|point| point.0 >= fin_le + 0.5 * fin_chord)
            {
                assert!(
                    half <= at_fin((x - fin_le) / fin_chord) + 1e-9,
                    "{preset} z {z} x {x}"
                );
            }
            let thickest = surface.iter().map(|point| point.1).fold(0.0, f64::max);
            let fin_thickest = (0..=20)
                .map(|k| at_fin(0.05 * f64::from(k)))
                .fold(0.0, f64::max);
            assert!(
                thickest > 0.95 * fin_thickest,
                "{preset} z {z}: the fin's nose stands proud"
            );
        }
    }
}

#[test]
fn the_side_line_leaves_the_body_on_its_tangent_and_joins_the_fin_on_its_edge() {
    let entry = presets::get("A320-200").expect("preset");
    let builder = AircraftBuilder::new(Some(entry.geometry.clone()));
    let dorsal = builder
        .build_dorsal_fin(&entry.design_vector)
        .expect("builds")
        .expect("dorsal");
    let seat_x = entry.geometry.empennage.vstab_root_le_x_m.unwrap_or(
        entry.design_vector.fuselage_length_m - entry.geometry.empennage.vstab_offset_from_tail_m,
    ) - entry
        .geometry
        .empennage
        .vstab_dorsal_length_m
        .expect("length");
    let slopes: Vec<f64> = dorsal
        .xsecs
        .windows(2)
        .filter(|pair| pair[1].xyz_le[0] > pair[0].xyz_le[0] + 1e-6 && pair[0].xyz_le[0] >= seat_x)
        .map(|pair| {
            (pair[1].xyz_le[2] - pair[0].xyz_le[2]) / (pair[1].xyz_le[0] - pair[0].xyz_le[0])
        })
        .collect();
    assert!(slopes.len() > 10);
    // A curve with no kink: the slope only rises, from nearly level, in small steps.
    assert!(slopes[0] < 0.3, "{slopes:?}");
    for pair in slopes.windows(2) {
        assert!(
            pair[1] > pair[0] - 1e-6 && pair[1] < pair[0] + 0.5,
            "{slopes:?}"
        );
    }
}

#[test]
fn a_ventral_fin_pair_stays_out_of_the_lattice_and_ends_on_its_drawn_points() {
    let entry = presets::get("LJ60").expect("preset");
    let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
        .build(Some(&entry.design_vector), true)
        .expect("builds");
    let ventral = plane
        .wings
        .iter()
        .find(|wing| wing.is_ventral_fin())
        .expect("ventral");
    assert!(ventral.symmetric && ventral.is_geometry_only());
    // The third station, little canted: the rotated chord lands on the drawn trailing edge.
    let g = &entry.geometry.empennage;
    let drawn = g.ventral_fin_stations[2];
    let section = &ventral.xsecs[2];
    let te = ventral.xyz_of_xsec(2, 1.0, 0.0);
    assert!((section.xyz_le[1] - drawn.y_m).abs() < 1e-12);
    assert!(
        (te[2] - (g.vstab_z_m + drawn.te_z_m)).abs() < 0.03,
        "te {te:?}"
    );
    assert!(
        (te[0] - section.xyz_le[0] - (drawn.te_x_m - drawn.le_x_m)).abs() < 0.03,
        "te {te:?}"
    );
}

#[test]
fn ventral_stations_on_the_plane_of_symmetry_draw_one_centreline_fin() {
    // C295: only the side view draws the strake, so one fin of the drawn side outline.
    let entry = presets::get("C295").expect("preset");
    let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
        .build(Some(&entry.design_vector), true)
        .expect("builds");
    let ventral = plane
        .wings
        .iter()
        .find(|wing| wing.is_ventral_fin())
        .expect("ventral");
    assert!(!ventral.symmetric && ventral.is_geometry_only());
    assert!(ventral.xsecs.iter().all(|xsec| xsec.xyz_le[1] == 0.0));
    // Level chords cut from the drawn side outline: the root one at the higher end of the
    // drawn root (its leading edge carried up along the drawn edge), the lowest at the
    // drawn bottom point, and the side area is that outline's.
    let g = &entry.geometry.empennage;
    for index in 0..ventral.xsecs.len() {
        let (le, te) = (
            ventral.xyz_of_xsec(index, 0.0, 0.0),
            ventral.xyz_of_xsec(index, 1.0, 0.0),
        );
        assert!(
            (te[2] - le[2]).abs() < 1e-12 && te[1].abs() < 1e-12,
            "{index} {le:?} {te:?}"
        );
    }
    let stations = &g.ventral_fin_stations;
    let top = g.vstab_z_m + stations[0].le_z_m.max(stations[0].te_z_m);
    let bottom = g.vstab_z_m
        + stations
            .iter()
            .map(|s| s.le_z_m.min(s.te_z_m))
            .fold(f64::INFINITY, f64::min);
    assert!((ventral.xsecs[0].xyz_le[2] - top).abs() < 1e-12);
    assert!((ventral.xsecs.last().expect("tip").xyz_le[2] - bottom).abs() < 1e-12);
    let mut outline: Vec<(f64, f64)> = stations
        .iter()
        .map(|s| (s.le_x_m, g.vstab_z_m + s.le_z_m))
        .chain(
            stations
                .iter()
                .rev()
                .map(|s| (s.te_x_m, g.vstab_z_m + s.te_z_m)),
        )
        .collect();
    let (root, next) = (outline[0], outline[1]);
    outline[0] = (
        root.0 + (top - root.1) / (root.1 - next.1) * (root.0 - next.0),
        top,
    );
    let width = |z: f64| {
        let xs: Vec<f64> = (0..outline.len())
            .filter_map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                ((a.1 - z) * (b.1 - z) <= 0.0 && a.1 != b.1)
                    .then(|| a.0 + (z - a.1) / (b.1 - a.1) * (b.0 - a.0))
            })
            .collect();
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let steps = 4000;
    let drawn_area: f64 = (0..steps)
        .map(|k| {
            width(bottom + (f64::from(k) + 0.5) / f64::from(steps) * (top - bottom))
                * (top - bottom)
                / f64::from(steps)
        })
        .sum();
    assert!(
        (ventral.unfolded_area() / drawn_area - 1.0).abs() < 0.01,
        "{} {drawn_area}",
        ventral.unfolded_area()
    );
    let mut mirrored = entry.geometry.clone();
    for station in &mut mirrored.empennage.ventral_fin_stations {
        station.y_m += 0.3;
    }
    let pair = AircraftBuilder::new(Some(mirrored))
        .build(Some(&entry.design_vector), true)
        .expect("builds");
    assert!(
        pair.wings
            .iter()
            .find(|wing| wing.is_ventral_fin())
            .expect("pair")
            .symmetric
    );
}

#[test]
fn a_ventral_fin_skin_is_the_plan_outside_the_body_and_its_root_is_buried() {
    for name in ["LJ60", "C295"] {
        let entry = presets::get(name).expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let ventral = plane
            .wings
            .iter()
            .find(|wing| wing.is_ventral_fin())
            .expect("ventral");
        let shares = super::ventral::buried_shares(ventral, &plane.fuselages[0]);
        // The root panel is partly inside the body, the outer panels stand clear of it.
        assert!(shares[0] > 0.05 && shares[0] < 1.0, "{name} {shares:?}");
        assert!(
            shares.iter().skip(1).all(|share| *share < shares[0]),
            "{name} {shares:?}"
        );
        // The root section itself lies wholly inside the body: no slit at the junction.
        for i in 0..=20 {
            let p = ventral.xyz_of_xsec(0, f64::from(i) / 20.0, 0.0);
            assert!(
                p[1].abs() < plane.fuselages[0].half_width_at(p[0], p[2]),
                "{name} root {p:?}"
            );
        }
        let skin = ventral.skin_planform();
        // The skin is the plan less the plan inside the body.
        let buried: f64 = ventral
            .xsecs
            .windows(2)
            .zip(&shares)
            .map(|(pair, share)| {
                let span = (pair[1].xyz_le[1] - pair[0].xyz_le[1])
                    .hypot(pair[1].xyz_le[2] - pair[0].xyz_le[2]);
                share * span * (pair[0].chord + pair[1].chord) / 2.0
            })
            .sum();
        let side = if ventral.symmetric { 2.0 } else { 1.0 };
        let (skin_area, area) = (skin.unfolded_area(), ventral.unfolded_area());
        assert!(skin_area < area, "{name} skin {skin_area} of {area}");
        assert!(
            (skin_area - (area - side * buried)).abs() < 0.02 * area,
            "{name} skin {skin_area} of {area}, buried {buried}"
        );
        assert_eq!(skin.xsecs.last(), ventral.xsecs.last());
    }
}

#[test]
fn a_dorsal_without_length_is_dropped_and_it_follows_the_fin_along_the_fuselage() {
    let entry = presets::get("A320-200").expect("preset");
    let mut none = entry.geometry.clone();
    none.empennage.vstab_dorsal_length_m = Some(0.0);
    let none = AircraftBuilder::new(Some(none));
    assert!(none
        .build_dorsal_fin(&entry.design_vector)
        .expect("builds")
        .is_none());
    let builder = AircraftBuilder::new(Some(entry.geometry.clone()));
    let area = |delta: f64| {
        let mut design = entry.design_vector;
        design.fuselage_length_m += delta;
        builder
            .build_dorsal_fin(&design)
            .expect("builds")
            .expect("has a dorsal")
            .unfolded_area()
    };
    let nominal = area(0.0);
    for delta in [-6.0, -1.0, 1.0, 6.0] {
        assert!((area(delta) - nominal).abs() < 0.25 * nominal, "dL {delta}");
    }
}

#[test]
fn a_half_configured_dorsal_is_refused() {
    let entry = presets::get("A320-200").expect("preset");
    let mut geometry = entry.geometry.clone();
    geometry.empennage.vstab_dorsal_height_m = Some(1.0);
    geometry.empennage.vstab_dorsal_length_m = None;
    geometry.empennage.vstab_dorsal_x_start_m = None;
    let builder = AircraftBuilder::new(Some(geometry));
    assert!(matches!(
        builder.build_dorsal_fin(&entry.design_vector),
        Err(BuildError::TailPlacement(_))
    ));
}

#[test]
fn the_dc_10_pod_carries_its_struts_and_a_fairing_outside_the_lattice() {
    let entry = presets::get("DC-10").expect("preset");
    let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
        .build(Some(&entry.design_vector), true)
        .expect("builds");
    let (fuselage, pod) = (&plane.fuselages[0], &plane.fuselages[1]);
    let surfaces: Vec<_> = plane
        .wings
        .iter()
        .filter(|wing| wing.is_centre_pod_surface())
        .collect();
    assert_eq!(surfaces.len(), 3);
    assert!(surfaces.iter().all(|surface| surface.is_geometry_only()));
    assert!(surfaces[0].xsecs[0].xyz_le[2] < surfaces[0].xsecs[1].xyz_le[2]);
    assert!(pod.xsecs[0].xyz_c[0] < surfaces[0].xsecs[0].xyz_le[0]);
    assert!(surfaces[1].xsecs[0].xyz_le[0] < fuselage.xsecs.last().expect("end").xyz_c[0]);
    // The fairing starts on the fin trailing edge at the top of its outline.
    let fin = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .expect("fin");
    let top = surfaces[2].xsecs.last().expect("top");
    let root = &fin.xsecs[0];
    assert!(top.xyz_le[0] > root.xyz_le[0] && top.xyz_le[2] > root.xyz_le[2]);
    // No other preset has them.
    let other = presets::get("A320-200").expect("preset");
    let plane = AircraftBuilder::new(Some(other.geometry.clone()))
        .build(Some(&other.design_vector), true)
        .expect("builds");
    assert!(!plane.wings.iter().any(|wing| wing.is_centre_pod_surface()));
}

#[test]
fn a_fin_keeps_its_reference_unless_it_stands_above_the_fuselage_on_a_pod() {
    let fin_of = |wings: Vec<Wing>| {
        wings
            .into_iter()
            .find(|wing| wing.name == "Vertical Stabilizer")
            .expect("fin")
    };
    // The generic third engine lies on the last nacelle length of the tailcone. This
    // fin ends ahead of it, on the falling tailcone, so nothing about it changes.
    let forward = |geometry: &mut alas_config::GeometryConfig| {
        geometry.empennage.vstab_offset_from_tail_m += 3.5;
    };
    let third_engine = |geometry: &mut alas_config::GeometryConfig| {
        geometry.engine.spanwise_positions_m.push(0.0);
        geometry.engine.centre_engine = None;
    };
    let alone = fin_of(built("A320-200", forward));
    let third = fin_of(built("A320-200", |geometry| {
        forward(geometry);
        third_engine(geometry);
    }));
    assert_eq!(third.xsecs[0].xyz_le, alone.xsecs[0].xyz_le);
    assert_eq!(
        third.reference_planform().area(),
        alone.reference_planform().area()
    );
    // On the tail the engine raises the root a little, to below where the leading edge
    // meets the fuselage: the reference is not carried up from the built root.
    let raised = fin_of(built("A320-200", third_engine));
    let reference = raised.reference_planform();
    assert_eq!(reference.xsecs[0].xyz_le, raised.xsecs[0].xyz_le);
    // A fin standing well above the fuselage on the pod is carried down.
    let dc10 = fin_of(built("DC-10", |_| {}));
    let reference = dc10.reference_planform();
    assert!(reference.xsecs[0].xyz_le[2] < dc10.xsecs[0].xyz_le[2] - 2.0);
}

#[test]
fn a_first_edge_station_gives_the_same_surface_at_every_mesh() {
    let fin_at = |n: i64, offset: (f64, f64)| {
        built("B747-400", |geometry| {
            geometry.empennage.n_subdivisions = n;
            geometry.empennage.vstab_edge_stations = vec![alas_config::TailEdgeStation {
                span_fraction: 0.5,
                le_offset_m: offset.0,
                te_offset_m: offset.1,
            }];
        })
        .into_iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .expect("fin")
    };
    // Each edge runs straight through the station from the root to the tip, so the
    // area differs from the straight surface by half the offset over the built height.
    for (le_offset_m, te_offset_m) in [(0.5, 0.0), (0.0, 0.3), (0.5, -0.2)] {
        let coarse = fin_at(6, (le_offset_m, te_offset_m));
        let fine = fin_at(30, (le_offset_m, te_offset_m));
        assert!((coarse.area() - fine.area()).abs() < 1.0e-9);
        let root_z = fine.xsecs[0].xyz_le[2];
        let tip_z = fine.xsecs.last().expect("tip").xyz_le[2];
        let drawn =
            fine.reference_planform().area() + 0.5 * (te_offset_m - le_offset_m) * (tip_z - root_z);
        assert!(
            (fine.area() - drawn).abs() < 1.0e-9,
            "{} {drawn}",
            fine.area()
        );
    }
    // A leading edge pulled forward keeps its corner on the line at every mesh.
    let coarse = fin_at(6, (-0.3, 0.0));
    let fine = fin_at(30, (-0.3, 0.0));
    assert!((coarse.area() - fine.area()).abs() < 1.0e-9);
    assert!(fine.area() > fine.reference_planform().area());
}

#[test]
fn the_a350_tailplane_leading_edge_leaves_its_line_where_the_drawing_does() {
    let entry = presets::get("A350-900").expect("preset");
    let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
        .build(Some(&entry.design_vector), false)
        .expect("builds");
    let htp = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Horizontal Stabilizer")
        .expect("HTP");
    let reference = htp.reference_planform();
    let (root, tip) = (
        &reference.xsecs[0],
        &reference.xsecs[reference.xsecs.len() - 1],
    );
    let offset = |eta: f64| {
        let section = htp
            .xsecs
            .iter()
            .find(|xsec| (xsec.xyz_le[1] - eta * tip.xyz_le[1]).abs() < 1e-9)
            .expect("a section at the station");
        section.xyz_le[0] - (root.xyz_le[0] + eta * (tip.xyz_le[0] - root.xyz_le[0]))
    };
    assert!(offset(0.905).abs() < 1e-9);
    assert!((offset(0.94) - 0.12).abs() < 1e-9);
}

#[test]
fn a_wingtip_flare_stays_out_of_the_taper_and_the_sweep_with_no_side_of_body() {
    for preset in ["B747-400", "LJ60", "A320-200"] {
        let entry = presets::get(preset).expect("preset");
        let wing_with = |offset_m: Option<f64>| {
            let mut geometry = entry.geometry.clone();
            geometry.wing.side_of_body_span_fraction = None;
            geometry.wing.side_of_body_chord_ratio = None;
            let sharklet = alas_config::presets::get("A320-200")
                .expect("preset")
                .geometry
                .wing
                .wingtip_device
                .clone();
            let device = geometry
                .wing
                .wingtip_device
                .get_or_insert(sharklet.expect("device"));
            // A flare needs a device without path stations (the B747-400 winglet has them).
            device.path_stations.clear();
            device.root_te_flare = offset_m.map(|offset_m| alas_config::WingtipTeFlare {
                offset_m,
                fade_length_m: 0.4,
            });
            AircraftBuilder::new(Some(geometry))
                .build(Some(&entry.design_vector), false)
                .expect("builds")
                .wings
                .swap_remove(0)
        };
        let (plain, flared) = (wing_with(None), wing_with(Some(0.2)));
        assert!(flared.planform_datum.is_none(), "{preset}");
        let tip = |wing: &crate::aircraft::wing::Wing| wing.xsecs[wing.xsecs.len() - 1].chord;
        assert!((tip(&flared) - tip(&plain) - 0.2).abs() < 1e-12, "{preset}");
        // The built sections alone would read the flare as taper.
        let drawn_taper = tip(&flared) / flared.xsecs[0].chord;
        assert!(drawn_taper - flared.taper_ratio() > 0.01, "{preset}");
        assert!(
            (flared.taper_ratio() - plain.taper_ratio()).abs() < 1e-12,
            "{preset}"
        );
        for chord_point in [0.0, 0.25, 1.0] {
            let (with, without) = (
                flared.mean_sweep_angle(chord_point),
                plain.mean_sweep_angle(chord_point),
            );
            assert!((with - without).abs() < 1e-9, "{preset} at {chord_point}");
        }
    }
}

#[test]
fn a_wingtip_flare_stays_out_of_the_taper_and_the_sweep() {
    for preset in ["B747-400", "AVE", "LJ60", "A400M"] {
        let entry = presets::get(preset).expect("preset");
        let wing_with = |offset_m: Option<f64>| {
            let mut geometry = entry.geometry.clone();
            let sharklet = alas_config::presets::get("A320-200")
                .expect("preset")
                .geometry
                .wing
                .wingtip_device
                .clone();
            let device = geometry
                .wing
                .wingtip_device
                .get_or_insert(sharklet.expect("device"));
            // A flare needs a device without path stations (the B747-400 winglet has them).
            device.path_stations.clear();
            device.root_te_flare = offset_m.map(|offset_m| alas_config::WingtipTeFlare {
                offset_m,
                fade_length_m: 0.4,
            });
            AircraftBuilder::new(Some(geometry))
                .build(Some(&entry.design_vector), false)
                .expect("builds")
                .wings
                .swap_remove(0)
        };
        let (plain, flared) = (wing_with(None), wing_with(Some(0.2)));
        assert!(
            (flared.taper_ratio() - plain.taper_ratio()).abs() < 1e-12,
            "{preset}"
        );
        assert!(
            (flared.mean_sweep_angle(0.25) - plain.mean_sweep_angle(0.25)).abs() < 1e-9,
            "{preset}"
        );
    }
}

#[test]
fn a_strut_without_chord_stops_the_build() {
    let entry = presets::get("DC-10").expect("preset");
    let mut geometry = entry.geometry.clone();
    let pod = geometry.engine.centre_engine.as_mut().expect("pod");
    pod.struts[0].chord_at_fuselage_m = -1.0;
    let built = AircraftBuilder::new(Some(geometry)).build(Some(&entry.design_vector), false);
    assert!(
        matches!(&built, Err(BuildError::TailPlacement(issue)) if issue.contains("strut 0")),
        "{:?}",
        built.map(|_| ())
    );
}

#[test]
fn a_dorsal_in_the_lattice_keeps_its_section_count_at_every_tail_scale() {
    for preset in [
        "A320-200",
        "A220-300",
        "A340-300",
        "A350-900",
        "ATR72-600",
        "B787-9",
        "C295",
    ] {
        let entry = presets::get(preset).expect("preset");
        let mut geometry = entry.geometry.clone();
        geometry.empennage.vstab_dorsal_in_aero = true;
        let builder = AircraftBuilder::new(Some(geometry));
        let cut_at = |tail_scale: f64| {
            let mut design = entry.design_vector;
            design.tail_scale = tail_scale;
            builder
                .build_dorsal_fin(&design)
                .expect("builds")
                .expect("dorsal")
        };
        let count = cut_at(1.0).xsecs.len();
        for step in 0..=80 {
            let dorsal = cut_at(0.8 + 0.4 * f64::from(step) / 80.0);
            assert_eq!(dorsal.xsecs.len(), count, "{preset} step {step}");
            assert!(dorsal
                .xsecs
                .windows(2)
                .all(|pair| pair[1].xyz_le[2] > pair[0].xyz_le[2]));
        }
    }
}
