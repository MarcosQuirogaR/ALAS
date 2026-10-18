// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Studio core: derived publications, topology library, tail span convention.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_uav::airframe::geometry::{AirframeGeometry, TailShape};
use alas_uav::spec::master::{EmpennageDriving, TailSurface};
use alas_uav::spec::skeleton::Datum;
use alas_uav::studio::publish::{self, find, is_derived_name, motor_stations};
use alas_uav::studio::topology::{build, Preset, Sizing, LIBRARY};

fn point(spec: &alas_uav::spec::UavSpec, name: &str) -> [f64; 3] {
    match &find(spec, name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .datum
    {
        Datum::Point { xyz_m } => *xyz_m,
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn every_library_entry_builds_valid_and_publishes() {
    for info in LIBRARY {
        let spec = build(info.preset, &Sizing::default()).unwrap();
        spec.validate().unwrap();
        assert!(find(&spec, "WS_Root_LE").is_some(), "{}", info.key);
        assert!(find(&spec, "WS_AC").is_some());
        let has_pod = find(&spec, "FS_Envelope").is_some();
        assert_eq!(has_pod, info.preset != Preset::FlyingWing, "{}", info.key);
        // Every control surface got a hinge axis and a servo seat per side.
        let hinges = spec
            .subskeletons
            .values()
            .flatten()
            .filter(|p| p.name.contains("_Hinge_"))
            .count();
        assert!(hinges >= spec.master.controls.len(), "{}", info.key);
    }
}

#[test]
fn json_round_trip_keeps_the_layout() {
    let spec = build(Preset::VTailTwin, &Sizing::default()).unwrap();
    let text = serde_json::to_string(&spec).unwrap();
    let back: alas_uav::spec::UavSpec = serde_json::from_str(&text).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn master_change_moves_dependent_datums() {
    let mut spec = build(Preset::VTailTwin, &Sizing::default()).unwrap();
    let before_tip = point(&spec, "WS_Tip_LE");
    let before_motor = point_of_plane(&spec, "PS_Motor_R_Plane");
    spec.master.panels[1].span_m += 0.2;
    spec.master.wing_xyz_m[0] += 0.1;
    publish::refresh(&mut spec).unwrap();
    let tip = point(&spec, "WS_Tip_LE");
    assert!((tip[1] - before_tip[1] - 0.2).abs() < 1e-12);
    assert!((tip[0] - before_tip[0] - 0.1).abs() < 1e-12);
    let motor = point_of_plane(&spec, "PS_Motor_R_Plane");
    assert!((motor[0] - before_motor[0] - 0.1).abs() < 1e-12);
    // Idempotent and free of duplicates.
    let n = spec.subskeletons.values().map(Vec::len).sum::<usize>();
    publish::refresh(&mut spec).unwrap();
    assert_eq!(n, spec.subskeletons.values().map(Vec::len).sum::<usize>());
}

fn point_of_plane(spec: &alas_uav::spec::UavSpec, name: &str) -> [f64; 3] {
    match &find(spec, name).unwrap().datum {
        Datum::Plane { origin_m, .. } => *origin_m,
        other => panic!("{other:?}"),
    }
}

#[test]
fn hinge_lines_lie_on_the_hinge_chord_fraction() {
    let spec = build(Preset::ConventionalSingle, &Sizing::default()).unwrap();
    // Aileron is controls[0]: 0.55..0.95 of the half span, 25 % chord.
    let Datum::Axis {
        origin_m,
        direction,
    } = &find(&spec, "WMS_Hinge_0_Axis_R").unwrap().datum
    else {
        panic!()
    };
    // Outer panel taper 0.5 from the break at 0.3 half-span: chord at 0.55*half.
    let half = 0.5 * 2.4;
    let c_break = 0.3;
    let s = 0.55 * half - 0.3 * half;
    let c = c_break + (c_break * 0.5 - c_break) * s / (0.7 * half);
    let x_le = 0.40 * 0.8; // wing_x; zero sweep
    assert!(
        (origin_m[0] - (x_le + 0.75 * c)).abs() < 1e-9,
        "{origin_m:?}"
    );
    assert!((origin_m[1] - 0.55 * half).abs() < 1e-9);
    let norm: f64 = direction.iter().map(|d| d * d).sum::<f64>().sqrt();
    assert!((norm - 1.0).abs() < 1e-12);
    // The port side mirrors.
    let Datum::Axis { origin_m: l, .. } = &find(&spec, "WMS_Hinge_0_Axis_L").unwrap().datum else {
        panic!()
    };
    assert!((l[1] + origin_m[1]).abs() < 1e-12 && (l[0] - origin_m[0]).abs() < 1e-12);
}

#[test]
fn motor_stations_follow_the_topology() {
    let twin = build(Preset::VTailTwin, &Sizing::default()).unwrap();
    let m = motor_stations(&twin).unwrap();
    assert_eq!(m.len(), 2);
    assert!((m[0].plane_m[1] - 0.2 * 1.2).abs() < 1e-12);
    assert!((m[0].plane_m[1] + m[1].plane_m[1]).abs() < 1e-12);
    let single = build(Preset::TTailPusher, &Sizing::default()).unwrap();
    let p = motor_stations(&single).unwrap();
    assert!(p[0].pusher && p[0].plane_m[0] > 0.8);
}

#[test]
fn custom_publications_survive_a_refresh() {
    assert!(is_derived_name("FS_Bay_Nose") && !is_derived_name("USR_Mark"));
    let mut spec = build(Preset::ConventionalSingle, &Sizing::default()).unwrap();
    spec.subskeletons
        .entry(alas_uav::spec::skeleton::Skeleton::Fuselage)
        .or_default()
        .push(alas_uav::spec::skeleton::Publication {
            name: "USR_Mark".into(),
            owner: alas_uav::spec::skeleton::Skeleton::Fuselage,
            datum: Datum::Point {
                xyz_m: [0.1, 0.0, 0.0],
            },
        });
    publish::refresh(&mut spec).unwrap();
    assert!(find(&spec, "USR_Mark").is_some());
}

#[test]
fn tail_span_is_measured_along_the_surface() {
    // V pair, span 0.625 m along the surface, chord 0.176 m, 40 deg:
    // true area b*c, lateral projection b cos(40).
    let mut spec = build(Preset::VTailTwin, &Sizing::default()).unwrap();
    let (b, c, g) = (0.625, 0.176, 40.0_f64);
    spec.master.empennage = EmpennageDriving::Explicit {
        horizontal: Some(TailSurface {
            span_m: b,
            root_chord_m: c,
            taper_ratio: 1.0,
            sweep_le_deg: 0.0,
            incidence_deg: 0.0,
            dihedral_deg: g,
            airfoil: "naca0006".into(),
        }),
        vertical: None,
    };
    let geo =
        AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default()).unwrap();
    let h = geo.horizontal.unwrap();
    assert!((h.planform.true_area_m2() - b * c).abs() < 1e-9);
    assert!((h.planform.span_m() - b * g.to_radians().cos()).abs() < 1e-9);
}
