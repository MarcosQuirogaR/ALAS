// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Bottom-up mass, CG and inertia (UAV-PLAN 3.7).  Verification cases are the
//! worked examples of Sadraey, Aircraft Design: A Systems Engineering Approach:
//! Example 11.2 (CG of 13 components, p. 601-603), Example 11.3 (thin-shell
//! fuselage inertia and parallel axis, p. 617-618) and Example 11.4 (wing as a
//! box, p. 618-620), all read on the page of the PDF.

mod airframe_support;

use std::collections::BTreeMap;

use alas_uav::airframe::inertia::{Inertia, Vec3};
use alas_uav::airframe::mass::{LoadCase, MassBasis, MassGroup, MassItem, MassStatement};
use alas_uav::airframe::structure::{ArealDensity, FuselageShell};
use alas_uav::airframe::{build_mass_statement, AirframeError, MassInputs};
use alas_uav::airframe::{wire_mass_per_m, WireRun};
use alas_uav::spec::skeleton::{Node, Part};
use alas_uav::spec::UavSpec;
use alas_uav_db::parts::{BatterySpec, MotorSpec, ServoSpec};
use alas_uav_db::{Body, ComponentId, Database};

use airframe_support::{component, num, reference_spec};

fn point(name: &str, m: f64, x: f64) -> MassItem {
    MassItem {
        name: name.into(),
        group: MassGroup::Other,
        mass_kg: m,
        cg_m: [x, 0.0, 0.0],
        inertia_cg: Inertia::ZERO,
        basis: MassBasis::UserInput,
        inertia_modelled: false,
    }
}

#[test]
fn sadraey_example_11_2_cg_of_the_business_jet() {
    // Table 11.7 as used in the worked solution (masses and x in m from the
    // reference line; the solution's own groupings: passengers 12 x 90 kg
    // lumped at 10.5 m, fuel 4000 + 5050 kg).
    let rows = [
        (4200.0, 13.0),
        (600.0, 24.0),
        (6900.0, 17.0),
        (3300.0, 13.0),
        (180.0, 3.6),
        (1080.0, 10.5),
        (90.0, 5.0),
        (600.0, 15.0),
        (1200.0, 9.0),
        (4000.0, 14.0),
        (5050.0, 9.0),
        (2800.0, 8.0),
    ];
    let st = MassStatement {
        items: rows.iter().map(|&(m, x)| point("c", m, x)).collect(),
    };
    assert!((st.total_mass_kg() - 30_000.0).abs() < 1e-9);
    let x_cg = st.cg_m().unwrap()[0];
    // Printed: sum m x = 385 288, X_cg1 = 12.843 m.
    assert!((x_cg * 30_000.0 - 385_288.0).abs() < 1e-6);
    assert!((x_cg - 12.843).abs() < 5e-4);
}

#[test]
fn sadraey_example_11_3_thin_shell_fuselage_about_the_aircraft_cg() {
    let (m, d, l) = (3600.0, 3.3, 32.0);
    let shell = Inertia::thin_shell_x(m, 0.5 * d, l);
    assert!((shell.ixx() - 9801.0).abs() < 1e-6); // printed 9801
    assert!((shell.iyy() - 312_100.5).abs() < 1e-3); // printed 312 100.5
    let item = MassItem {
        inertia_cg: shell,
        inertia_modelled: true,
        ..point("fuselage", m, 16.0)
    };
    // Aircraft CG 15.2 m from the nose: the fuselage CG is 0.8 m aft of it.
    let about_cg = MassStatement { items: vec![item] }.inertia_about([15.2, 0.0, 0.0]);
    assert!((about_cg.iyy() - 314_404.5).abs() < 1e-3); // printed 314 404.5
    assert!((about_cg.izz() - 314_404.5).abs() < 1e-3);
    assert!((about_cg.ixx() - 9801.0).abs() < 1e-6);
}

#[test]
fn sadraey_example_11_4_wing_box_inertia() {
    // Wing as a box: span a = sqrt(8 * 12) = 9.798 m, chord c = 12 / a, mean
    // thickness b = 0.5 * 0.15 * c, mass 100 kg (the printed 800.1, 12.57 and
    // 812.5 kg m^2 are m/12 times the usual sums with m = 100 kg).
    let a = (8.0_f64 * 12.0).sqrt();
    let c = 12.0 / a;
    let b = 0.5 * 0.15 * c;
    let m = 100.0;
    let wing = Inertia::solid_box(m, [c, a, b]);
    assert!((wing.ixx() - 800.1).abs() < 0.2);
    assert!((wing.iyy() - 12.57).abs() < 0.02);
    assert!((wing.izz() - 812.5).abs() < 0.3);
    // Wing CG 0.8 m aft and 0.7 m above the aircraft CG: I_yy = 12.57 + m(0.8^2 + 0.7^2)
    // = 125.57 as printed (d^2 = 1.13 m^2).
    let item = MassItem {
        inertia_cg: wing,
        inertia_modelled: true,
        ..MassItem {
            cg_m: [0.8, 0.0, 0.7],
            ..point("wing", m, 0.0)
        }
    };
    let about = MassStatement { items: vec![item] }.inertia_about([0.0; 3]);
    assert!((about.iyy() - 125.57).abs() < 0.05);
    // The printed I_zz = 844.5 adds m d^2 with m = 50 kg (a slip against the
    // 100 kg of the other two axes); with m = 100 kg it is 812.5 + 64 = 876.5.
    assert!((about.izz() - (wing.izz() + 100.0 * 0.64)).abs() < 1e-9);
}

#[test]
fn parallel_axis_agrees_with_a_particle_sum() {
    // Deterministic cloud of 300 particles: inertia from the definition
    // sum m (r^2 E - r r^T) about the CG against item-wise parallel axis.
    let mut seed = 12345_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 11) as f64) / ((1_u64 << 53) as f64)
    };
    let mut items = Vec::new();
    for i in 0..300 {
        let m = 0.1 + next();
        let p: Vec3 = [3.0 * next(), 2.0 * next() - 1.0, 0.5 * next()];
        items.push(point(&format!("p{i}"), m, p[0]));
        items[i].cg_m = p;
    }
    let st = MassStatement { items };
    let cg = st.cg_m().unwrap();
    let mut direct = Inertia::ZERO;
    for it in &st.items {
        let d = [it.cg_m[0] - cg[0], it.cg_m[1] - cg[1], it.cg_m[2] - cg[2]];
        direct = direct.plus(Inertia::point_mass(it.mass_kg, d));
    }
    let via = st.inertia_about_cg().unwrap();
    for i in 0..3 {
        for j in 0..3 {
            assert!((direct.0[i][j] - via.0[i][j]).abs() < 1e-9);
        }
    }
    // Moving the reference point by t adds m (t^2 E - t t^T) of the total mass.
    let t = [0.3, -0.2, 0.9];
    let shifted = st.inertia_about([cg[0] + t[0], cg[1] + t[1], cg[2] + t[2]]);
    let expect = via.plus(Inertia::point_mass(st.total_mass_kg(), t));
    for i in 0..3 {
        for j in 0..3 {
            assert!((shifted.0[i][j] - expect.0[i][j]).abs() < 1e-9);
        }
    }
}

fn parts_db() -> Database {
    let motor = MotorSpec {
        mass_kg: Some(num(0.108, "kg")),
        diameter_m: Some(num(0.035, "m")),
        length_m: Some(num(0.042, "m")),
        ..MotorSpec::default()
    };
    let battery = BatterySpec {
        mass_kg: Some(num(0.185, "kg")),
        dims_m: Some(alas_uav_db::Field::new(
            [0.105, 0.035, 0.026],
            "m",
            alas_uav_db::SourceId::new("test-src").unwrap(),
            None,
            alas_uav_db::Method::User,
        )),
        ..BatterySpec::default()
    };
    let servo = ServoSpec::default(); // no mass: must be an error unless overridden
    Database {
        components: vec![
            component("test-motor", Body::Motor(motor)),
            component("test-pack", Body::Battery(battery)),
            component("test-servo", Body::Servo(servo)),
        ],
        ..Database::default()
    }
}

fn catalog(name: &str, id: &str, off: Vec3) -> Node {
    Node {
        name: name.into(),
        parent: Some("airframe".into()),
        parent_frame: None,
        offset_m: off,
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: None,
        part: Part::Catalog {
            component: ComponentId::new(id).unwrap(),
        },
    }
}

fn node(name: &str, off: Vec3, part: Part) -> Node {
    Node {
        name: name.into(),
        parent: Some("airframe".into()),
        parent_frame: None,
        offset_m: off,
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: None,
        part,
    }
}

fn tree_spec() -> UavSpec {
    let mut spec = reference_spec();
    spec.tree = vec![
        node("airframe", [0.0; 3], Part::Assembly),
        catalog("motor", "test-motor", [0.0, 0.0, 0.0]),
        catalog("battery", "test-pack", [0.12, 0.0, 0.0]),
        node(
            "payload",
            [0.30, 0.0, 0.0],
            Part::Payload {
                role: "cans".into(),
            },
        ),
        node(
            "trim ballast",
            [0.02, 0.0, 0.0],
            Part::Ballast { mass_kg: 0.0 },
        ),
    ];
    spec
}

fn inputs() -> MassInputs {
    let mut i = MassInputs::generic();
    i.structure.fuselage_shell = Some(FuselageShell {
        x_start_m: 0.0,
        length_m: 0.5,
        diameter_m: 0.12,
        centre_yz_m: [0.0, 0.0],
    });
    i
}

fn case(cans: f64) -> LoadCase {
    LoadCase {
        name: format!("{cans} kg"),
        payload_kg: BTreeMap::from([("cans".to_owned(), cans)]),
        ballast: Vec::new(),
    }
}

#[test]
fn bottom_up_statement_closes_and_tags_provenance() {
    let spec = tree_spec();
    let st = build_mass_statement(&spec, &parts_db(), &inputs(), &case(1.0)).unwrap();
    let by_group: f64 = [
        MassGroup::Structure,
        MassGroup::Allowance,
        MassGroup::Propulsion,
        MassGroup::Avionics,
        MassGroup::Gear,
        MassGroup::Payload,
        MassGroup::Ballast,
        MassGroup::Other,
    ]
    .iter()
    .map(|g| st.group_mass_kg(*g))
    .sum();
    assert!((by_group - st.total_mass_kg()).abs() < 1e-12); // closure
                                                            // Database parts enter with exactly their row mass.
    assert!((st.basis_mass_kg(MassBasis::Database) - (0.108 + 0.185)).abs() < 1e-12);
    assert!((st.group_mass_kg(MassGroup::Payload) - 1.0).abs() < 1e-12);
    // Structure is an estimate and says so; the allowance is 6 % of it.
    let structure = st.group_mass_kg(MassGroup::Structure);
    assert!(structure > 0.0);
    assert!((st.group_mass_kg(MassGroup::Allowance) - 0.06 * structure).abs() < 1e-12);
    assert!(st.estimate_fraction() > 0.2 && st.estimate_fraction() < 1.0);
    assert!(
        (st.uncalibrated_band_kg() - 0.08 * st.basis_mass_kg(MassBasis::Estimate)).abs() < 1e-12
    );
    // Wing structure = areal density x projected planform (area 2 * (0.4*0.28*(1+1)/2 ...)).
    let wing = st
        .items
        .iter()
        .find(|i| i.name == "wing structure")
        .unwrap();
    let s_wing = alas_uav::airframe::geometry::wing_planform(&spec.master)
        .unwrap()
        .projected_area_m2();
    assert!((wing.mass_kg - 0.9 * s_wing).abs() < 1e-9);
    assert!(st.inertia_about_cg().unwrap().is_finite());
}

#[test]
fn cg_translates_rigidly_and_inertia_about_cg_is_invariant() {
    let spec = tree_spec();
    let base = build_mass_statement(&spec, &parts_db(), &inputs(), &case(0.8)).unwrap();
    let dx = 0.37;
    let mut moved = spec.clone();
    moved.master.wing_xyz_m[0] += dx;
    moved.master.tail_xyz_m[0] += dx;
    for n in &mut moved.tree {
        n.offset_m[0] += dx;
    }
    let mut inp = inputs();
    if let Some(s) = inp.structure.fuselage_shell.as_mut() {
        s.x_start_m += dx;
    }
    let shifted = build_mass_statement(&moved, &parts_db(), &inp, &case(0.8)).unwrap();
    assert!((shifted.total_mass_kg() - base.total_mass_kg()).abs() < 1e-12);
    let (c0, c1) = (base.cg_m().unwrap(), shifted.cg_m().unwrap());
    assert!((c1[0] - c0[0] - dx).abs() < 1e-12);
    assert!((c1[1] - c0[1]).abs() < 1e-12 && (c1[2] - c0[2]).abs() < 1e-12);
    let (i0, i1) = (
        base.inertia_about_cg().unwrap(),
        shifted.inertia_about_cg().unwrap(),
    );
    for i in 0..3 {
        for j in 0..3 {
            assert!((i0.0[i][j] - i1.0[i][j]).abs() < 1e-9);
        }
    }
}

#[test]
fn payload_load_cases_move_the_cg_and_the_ballast_solve_closes() {
    let spec = tree_spec();
    let db = parts_db();
    let empty = build_mass_statement(&spec, &db, &inputs(), &case(0.0)).unwrap();
    let full = build_mass_statement(&spec, &db, &inputs(), &case(1.5)).unwrap();
    assert!((full.total_mass_kg() - empty.total_mass_kg() - 1.5).abs() < 1e-12);
    // Payload sits at x = 0.30 m; closed form of the moved CG.
    let (xe, xf) = (empty.cg_m().unwrap()[0], full.cg_m().unwrap()[0]);
    let expect = (empty.total_mass_kg() * xe + 1.5 * 0.30) / full.total_mass_kg();
    assert!((xf - expect).abs() < 1e-12);
    let (lo, hi) = MassStatement::cg_x_envelope(&[empty.clone(), full.clone()]).unwrap();
    assert_eq!((lo, hi), (xe.min(xf), xe.max(xf)));
    // Ballast to put the CG at target with the ballast in the nose.
    let target = xf - 0.01;
    let m = full.ballast_for_cg(target, 0.0).unwrap();
    let mut with = full.clone();
    with.items.push(point("ballast", m, 0.0));
    assert!((with.cg_m().unwrap()[0] - target).abs() < 1e-12);
    // Ballast aft of the target cannot move the CG forward.
    assert!(matches!(
        full.ballast_for_cg(target, xf + 1.0),
        Err(AirframeError::NoSolution(_))
    ));
}

#[test]
fn missing_mass_is_an_error_and_a_weighed_override_is_flagged() {
    let mut spec = tree_spec();
    spec.tree
        .push(catalog("servo", "test-servo", [0.4, 0.0, 0.0]));
    let err = build_mass_statement(&spec, &parts_db(), &inputs(), &case(0.0)).unwrap_err();
    assert!(
        matches!(
            err,
            AirframeError::Missing {
                field: "mass_kg",
                ..
            }
        ),
        "{err}"
    );
    spec.tree.last_mut().unwrap().mass_override_kg = Some(0.013);
    let st = build_mass_statement(&spec, &parts_db(), &inputs(), &case(0.0)).unwrap();
    let servo = st.items.iter().find(|i| i.name == "servo").unwrap();
    assert_eq!((servo.mass_kg, servo.basis), (0.013, MassBasis::Weighed));
    // Unknown component and unknown publication are reported, not defaulted.
    let mut bad = tree_spec();
    bad.tree.push(catalog("ghost", "no-such-part", [0.0; 3]));
    assert!(matches!(
        build_mass_statement(&bad, &parts_db(), &inputs(), &case(0.0)),
        Err(AirframeError::UnknownComponent(_))
    ));
    let mut bad = tree_spec();
    bad.tree[1].parent_frame = Some("nowhere".into());
    assert!(matches!(
        build_mass_statement(&bad, &parts_db(), &inputs(), &case(0.0)),
        Err(AirframeError::UnknownName(_))
    ));
}

#[test]
fn structure_override_and_team_calibration_scale_only_what_they_should() {
    let mut spec = tree_spec();
    let db = parts_db();
    let base = build_mass_statement(&spec, &db, &inputs(), &case(0.0)).unwrap();
    let est_wing = base
        .items
        .iter()
        .find(|i| i.name == "wing structure")
        .unwrap()
        .mass_kg;
    let mut n = node(
        "wing weighed",
        [0.0; 3],
        Part::Structure {
            role: "wing".into(),
        },
    );
    n.mass_override_kg = Some(0.5 * est_wing);
    spec.tree.push(n);
    let st = build_mass_statement(&spec, &db, &inputs(), &case(0.0)).unwrap();
    let wing = st
        .items
        .iter()
        .find(|i| i.name == "wing structure")
        .unwrap();
    assert_eq!(wing.basis, MassBasis::Weighed);
    assert!((wing.mass_kg - 0.5 * est_wing).abs() < 1e-12);
    // Calibration scales estimates (and their inertia), not database or user masses.
    let cal = base.calibrated(1.1).unwrap();
    assert!(
        (cal.basis_mass_kg(MassBasis::Estimate) - 1.1 * base.basis_mass_kg(MassBasis::Estimate))
            .abs()
            < 1e-12
    );
    assert!(
        (cal.basis_mass_kg(MassBasis::Database) - base.basis_mass_kg(MassBasis::Database)).abs()
            < 1e-15
    );
    assert!(base.calibrated(0.0).is_err());
}

#[test]
fn structure_item_inertia_matches_a_thin_plate_closed_form() {
    // Rectangular wing 0.3 x 2.0 m, one panel, areal density 1 kg/m^2 and the
    // CG at 50 % chord: a flat plate of mass m has I_xx = m b^2/12 (about the
    // roll axis through its CG) and I_yy = m c^2/12, I_zz = m (b^2 + c^2)/12.
    use alas_uav::airframe::geometry::{Panel, Planform};
    use alas_uav::airframe::structure::wing_structure;
    let pf = Planform {
        panels: vec![Panel {
            span_m: 1.0,
            chord_root_m: 0.3,
            chord_tip_m: 0.3,
            root_le_m: [0.2, 0.0, 0.0],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: true,
        vertical: false,
    };
    let mut inp = MassInputs::generic().structure;
    inp.wing = ArealDensity::new(1.0).unwrap();
    inp.chord_fraction_cg = 0.5;
    inp.strips_per_panel = 400;
    let w = wing_structure(&pf, &inp).unwrap();
    let (m, b, c) = (0.6, 2.0, 0.3);
    assert!((w.mass_kg - m).abs() < 1e-12);
    assert!((w.cg_m[0] - 0.35).abs() < 1e-12 && w.cg_m[1].abs() < 1e-12);
    assert!((w.inertia_cg.ixx() - m * b * b / 12.0).abs() / (m * b * b / 12.0) < 1e-4);
    assert!((w.inertia_cg.iyy() - m * c * c / 12.0).abs() / (m * c * c / 12.0) < 1e-3);
    assert!(
        (w.inertia_cg.izz() - m * (b * b + c * c) / 12.0).abs() / (m * (b * b + c * c) / 12.0)
            < 1e-4
    );
}

#[test]
fn wire_mass_follows_the_awg_definition() {
    // ASTM B258: AWG 36 is 0.0050 in = 0.127 mm and each 6 gauges halve the
    // diameter by 92^(6/39); 0000 (n = -3) is exactly 0.4600 in = 11.684 mm.
    let d = |awg: f64| 0.127e-3 * 92.0_f64.powf((36.0 - awg) / 39.0);
    assert!((d(36.0) - 0.127e-3).abs() < 1e-12);
    assert!((d(-3.0) - 11.684e-3).abs() < 1e-6);
    let per_m = wire_mass_per_m(10.0, 1.0);
    // AWG 10 copper: d = 2.588 mm, area 5.26 mm^2, 8960 kg/m^3 -> 47.1 g/m.
    assert!((per_m - 0.0471).abs() < 3e-4, "{per_m}");
    assert!((wire_mass_per_m(10.0, 1.25) - 1.25 * per_m).abs() < 1e-15);
}

#[test]
fn wire_runs_add_mass_between_named_nodes() {
    let spec = tree_spec();
    let mut inp = inputs();
    inp.wires = vec![WireRun {
        from: "motor".into(),
        to: "battery".into(),
        kg_per_m: 0.05,
        slack_factor: 1.15,
    }];
    let base = build_mass_statement(&spec, &parts_db(), &inputs(), &case(0.0)).unwrap();
    let with = build_mass_statement(&spec, &parts_db(), &inp, &case(0.0)).unwrap();
    assert!((with.total_mass_kg() - base.total_mass_kg() - 0.05 * 0.12 * 1.15).abs() < 1e-12);
    inp.wires[0].to = "missing".into();
    assert!(build_mass_statement(&spec, &parts_db(), &inp, &case(0.0)).is_err());
}
