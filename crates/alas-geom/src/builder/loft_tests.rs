// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Builder tests for the lofted fuselage path.

use super::*;
use alas_config::presets;

/// FNV-1a over the `Debug` text of a value. `f64`'s `Debug` is the shortest
/// text that round-trips, so equal hashes mean bit-identical numbers.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Hash of the built airplane of a preset with its loft CLEARED, i.e. the legacy
/// generated-station path (every registered preset now references a loft).
fn built_hash(name: &str, nacelle_v2: bool) -> u64 {
    let preset = presets::get(name).expect("preset exists");
    let mut geometry = preset.geometry.clone();
    geometry.fuselage.loft = None;
    let airplane = AircraftBuilder::new(Some(geometry))
        .with_nacelle_v2(nacelle_v2)
        .build(Some(&preset.design_vector), true)
        .expect("preset builds");
    fnv1a(&format!("{airplane:?}"))
}

/// Pinned on the tree before the loft builder existed (merge of the core,
/// nacelle and wing changes): with `loft = None` the built geometry of these
/// presets, nacelles included, must not change by one bit. Every registered
/// preset now references a loft, so the test clears it: the legacy path is
/// still the contract of a preset (or a user geometry) without a loft. The
/// tail-placement and wingtip-device changes keep these bits with their options
/// unset, so the pre-loft values still hold.
#[test]
fn presets_without_a_loft_build_bit_identical_geometry() {
    let expected = [
        ("A320-200", 15_155_333_065_114_281_125_u64),
        ("B747-400", 9_561_091_969_367_184_215_u64),
        ("A220-300", 4_178_324_166_916_265_089_u64),
    ];
    // The pins above were taken with the MSVC math runtime; glibc rounds some
    // transcendental functions differently in the last bit, so off Windows the
    // same geometry carries its own host-family pins.
    let expected = if cfg!(windows) {
        expected
    } else {
        [
            ("A320-200", 16_195_312_667_050_220_398_u64),
            ("B747-400", 12_802_470_272_508_494_065_u64),
            ("A220-300", 18_386_087_386_061_217_417_u64),
        ]
    };
    let moved: Vec<String> = expected
        .iter()
        .filter_map(|&(name, hash)| {
            let built = built_hash(name, false);
            (built != hash).then(|| format!("{name}: built {built}, pinned {hash}"))
        })
        .collect();
    assert!(moved.is_empty(), "geometry hash moved: {moved:?}");
}

fn fixture_builder(tweak: impl FnOnce(&mut GeometryConfig)) -> (AircraftBuilder, DesignVector) {
    let preset = presets::get("A320-200").expect("preset exists");
    let mut geometry = preset.geometry.clone();
    geometry.wing.wingtip_device = None; // the A320 has a sharklet (a fourth surface)
    let f = &mut geometry.fuselage;
    f.diameter_m = 3.95;
    f.height_m = None;
    f.cabin_z_m = 0.2;
    f.loft = Some(alas_config::LoftRef::embedded("test_fuselage"));
    tweak(&mut geometry);
    let mut dv = preset.design_vector;
    dv.fuselage_length_m = 37.5;
    (AircraftBuilder::new(Some(geometry)), dv)
}

#[test]
fn a_referenced_loft_builds_the_fuselage_and_its_polyline_view() {
    let (builder, dv) = fixture_builder(|_| {});
    let (airplane, report) = builder
        .build_reported(Some(&dv), true, WingShape::Flight)
        .expect("lofted aircraft builds");
    let fuselage = &airplane.fuselages[0];
    let loft = fuselage.loft.as_ref().expect("the loft is attached");
    assert_eq!(loft.length_m(), 37.5);
    let info = report.loft.as_ref().expect("loft figures reported");
    assert_eq!(info.loft_id, "test_fuselage");
    assert_eq!(info.stations, fuselage.xsecs.len());
    assert!(
        info.polyline_deviation_m <= POLYLINE_TOLERANCE_M,
        "{info:?}"
    );
    assert!((info.max_width_m - 3.95).abs() < 1e-6 && (info.max_height_m - 3.95).abs() < 1e-6);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // The view really is within the tolerance of the exact crown at mid-intervals.
    for pair in fuselage.xsecs.windows(2) {
        let x = 0.5 * (pair[0].xyz_c[0] + pair[1].xyz_c[0]);
        let chord = 0.5
            * (pair[0].xyz_c[2] + 0.5 * pair[0].height + pair[1].xyz_c[2] + 0.5 * pair[1].height);
        assert!((chord - loft.crown_z(x)).abs() <= POLYLINE_TOLERANCE_M + 1e-12);
    }
    // Main wing, tailplane and fin, plus the A320 dorsal fillet that product geometry always
    // builds.
    let names: Vec<&str> = airplane
        .wings
        .iter()
        .map(|wing| wing.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Main Wing",
            "Horizontal Stabilizer",
            "Vertical Stabilizer",
            "Dorsal Fin (outside the lattice)"
        ]
    );
}

#[test]
fn a_stretched_loft_keeps_its_cabin_cylinder_and_moves_the_tail() {
    let (builder, mut dv) = fixture_builder(|_| {});
    dv.fuselage_length_m = 42.0;
    let (airplane, report) = builder
        .build_reported(Some(&dv), false, WingShape::Flight)
        .expect("stretched loft builds");
    let loft = airplane.fuselages[0].loft.as_ref().unwrap();
    assert_eq!(loft.length_m(), 42.0);
    assert_eq!(loft.cabin_cylinder().map(|(a, _)| a), Some(6.0));
    assert!((loft.cabin_cylinder().unwrap().1 - 30.75).abs() < 1e-6);
    assert!((report.loft.unwrap().length_ratio - 1.12).abs() < 1e-12);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.code == "loft_length_remap"));
}

#[test]
fn an_unknown_loft_id_is_an_error_not_a_fallback() {
    let (builder, dv) = fixture_builder(|g| {
        g.fuselage.loft = Some(alas_config::LoftRef::embedded("no_such_loft"));
    });
    assert!(matches!(
        builder.build(Some(&dv), false),
        Err(BuildError::Loft(
            alas_config::LoftSpecError::UnknownEmbedded(_)
        ))
    ));
}

#[test]
fn extents_are_checked_against_the_configured_diameter_and_height() {
    // 0.9 % off: warning, loft used.
    let (builder, dv) = fixture_builder(|g| g.fuselage.diameter_m = 3.985);
    let (_, report) = builder
        .build_reported(Some(&dv), false, WingShape::Flight)
        .unwrap();
    assert!(report.has_warning("loft_extent_mismatch"));
    // 14 % off in height: refused.
    let (builder, dv) = fixture_builder(|g| g.fuselage.height_m = Some(4.5));
    assert!(matches!(
        builder.build(Some(&dv), false),
        Err(BuildError::LoftExtent {
            quantity: "height",
            ..
        })
    ));
}

#[test]
fn custom_sections_are_reported_as_ignored_in_loft_mode() {
    let (builder, dv) = fixture_builder(|g| {
        g.fuselage.custom_sections = vec![alas_config::FuselageSection {
            x_fraction: 0.5,
            z_m: 0.0,
            width_m: 3.0,
            height_m: 3.0,
            shape: 2.0,
        }];
    });
    let (_, report) = builder
        .build_reported(Some(&dv), false, WingShape::Flight)
        .unwrap();
    assert!(report.has_warning("loft_sections_ignored"));
}

#[test]
fn reference_compatibility_builds_never_use_a_loft() {
    let (builder, dv) = fixture_builder(|_| {});
    let compat = AircraftBuilder::new_reference_compatibility(Some(builder.geometry.clone()))
        .with_nacelle_v2(true);
    assert!(compat.geometry.fuselage.loft.is_none());
    let (airplane, report) = compat
        .build_reported(Some(&dv), true, WingShape::Flight)
        .unwrap();
    assert!(airplane.fuselages[0].loft.is_none());
    assert!(report.loft.is_none() && report.warnings.is_empty());
    // Even if the public geometry is edited after construction.
    let mut compat = compat;
    compat.geometry.fuselage.loft = Some(alas_config::LoftRef::embedded("test_fuselage"));
    assert!(compat.build(Some(&dv), false).unwrap().fuselages[0]
        .loft
        .is_none());
}

#[test]
fn the_empennage_seats_on_a_lofted_body() {
    let (builder, dv) = fixture_builder(|_| {});
    let (_, fin) = builder.build_empennage(&dv).unwrap();
    // The fixture crown falls to 1.5 m at the tail; the seat is the exact crown.
    assert!(fin
        .xsecs
        .iter()
        .all(|x| x.xyz_le.iter().all(|v| v.is_finite())));
}

#[test]
fn sample_xsecs_within_meets_the_tolerance_and_a_tighter_one_costs_stations() {
    let (builder, dv) = fixture_builder(|_| {});
    let airplane = builder.build(Some(&dv), false).unwrap();
    let loft = airplane.fuselages[0].loft.as_ref().unwrap();
    let coarse = loft.sample_xsecs_within(0.02, 8, 400);
    let fine = loft.sample_xsecs_within(0.002, 8, 400);
    assert!(coarse.within_tolerance && fine.within_tolerance);
    assert!(coarse.max_deviation_m <= 0.02 && fine.max_deviation_m <= 0.002);
    assert!(fine.xsecs.len() > coarse.xsecs.len());
    assert!(!loft.sample_xsecs_within(1e-6, 8, 20).within_tolerance);
}

/// No shipped loft carries an APU cone since the A380-800 body was ended at its printed tail cut,
/// so a design length that is too short is now refused by the loft itself, as a
/// typed error, before any cone arithmetic; `BuildError::ApuConeLength` is reachable only with a
/// loft file that declares a cone.
#[test]
fn a_design_length_not_longer_than_the_nose_and_tail_is_a_typed_error() {
    let preset = presets::get("A380-800").unwrap();
    let mut dv = preset.design_vector;
    dv.fuselage_length_m = 2.0;
    let error = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&dv), false)
        .unwrap_err();
    assert!(
        matches!(
            &error,
            BuildError::LoftBuild(crate::loft::LoftError::LengthTooShort { length_m, needed_m })
                if *length_m == 2.0 && *needed_m > 2.0
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("does not exceed the nose and tail"),
        "{error}"
    );
}
