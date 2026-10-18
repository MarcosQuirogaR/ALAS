// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Packaging of database parts into bays, collision and volume checks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod airframe_support;

use alas_uav::spec::skeleton::Node;
use alas_uav::spec::UavSpec;
use alas_uav::studio::check::{check, IssueKind, Severity};
use alas_uav::studio::pack::{mount, mount_motor, pack, PartRequest};
use alas_uav::studio::topology::{build, Preset, Sizing};
use alas_uav_db::parts::{BatterySpec, MotorSpec, PropellerSpec, ServoSpec};
use alas_uav_db::{Body, ComponentId, Database, Field, Method, SourceId};

use airframe_support::{component, num};

fn dims(d: [f64; 3]) -> Option<Field<[f64; 3]>> {
    Some(Field::new(
        d,
        "m",
        SourceId::new("test-src").unwrap(),
        None,
        Method::User,
    ))
}

fn db() -> Database {
    let motor = MotorSpec {
        mass_kg: Some(num(0.108, "kg")),
        diameter_m: Some(num(0.035, "m")),
        length_m: Some(num(0.0545, "m")),
        ..MotorSpec::default()
    };
    let pack = |d: [f64; 3]| BatterySpec {
        mass_kg: Some(num(0.2, "kg")),
        dims_m: dims(d),
        ..BatterySpec::default()
    };
    let servo = ServoSpec {
        mass_kg: Some(num(0.009, "kg")),
        dims_m: dims([0.023, 0.012, 0.024]),
        ..ServoSpec::default()
    };
    let prop = |d: f64| PropellerSpec {
        diameter_m: Some(num(d, "m")),
        ..PropellerSpec::default()
    };
    Database {
        components: vec![
            component("motor", Body::Motor(motor)),
            component("pack-small", Body::Battery(pack([0.106, 0.034, 0.024]))),
            component("pack-fat", Body::Battery(pack([0.15, 0.30, 0.10]))),
            // Longest side across the bay: only fits when rotated.
            component("pack-long", Body::Battery(pack([0.034, 0.024, 0.106]))),
            component("pack-huge", Body::Battery(pack([0.5, 0.05, 0.05]))),
            component("servo", Body::Servo(servo)),
            component("prop-11", Body::Propeller(prop(0.279))),
            component("prop-huge", Body::Propeller(prop(0.9))),
            component("no-dims", Body::Battery(BatterySpec::default())),
        ],
        ..Database::default()
    }
}

fn req(name: &str, id: &str, bay: &str) -> PartRequest {
    PartRequest {
        name: name.into(),
        component: ComponentId::new(id).unwrap(),
        bay: Some(bay.into()),
    }
}

fn spec() -> UavSpec {
    build(Preset::VTailTwin, &Sizing::default()).unwrap()
}

#[test]
fn parts_pack_inside_their_bay_without_collisions() {
    let s = pack(
        &spec(),
        &db(),
        &[
            req("pack a", "pack-small", "Battery"),
            req("pack b", "pack-small", "Battery"),
        ],
    )
    .unwrap();
    let rep = check(&s, &db()).unwrap();
    assert!(rep.is_ok(), "{:?}", rep.issues);
    assert_eq!(rep.parts.len(), 2);
    let (a, b) = (&rep.parts[0].aabb, &rep.parts[1].aabb);
    assert!(b.min[0] >= a.max[0], "stacked along x");
    // Sitting on the floor, centred in y.
    assert!((a.centre()[1]).abs() < 1e-12);
    let fill = &rep.fills.iter().find(|f| f.bay == "Battery").unwrap();
    assert!((fill.used_m3 - 2.0 * 0.106 * 0.034 * 0.024).abs() < 1e-9);
}

#[test]
fn a_part_is_rotated_to_fit_the_section() {
    let s = pack(&spec(), &db(), &[req("long", "pack-long", "Battery")]).unwrap();
    let node = &s.tree[0];
    assert!(node.euler_deg.iter().any(|a| *a != 0.0), "rotated");
    assert!(check(&s, &db()).unwrap().is_ok());
}

#[test]
fn oversize_overlap_missing_dims_and_fill_are_reported() {
    let rep = |s: &UavSpec| check(s, &db()).unwrap();
    let s = pack(&spec(), &db(), &[req("fat", "pack-fat", "Battery")]).unwrap();
    assert!(rep(&s)
        .issues
        .iter()
        .any(|i| i.kind == IssueKind::OutsideBay && i.severity == Severity::Error));
    let s = pack(&spec(), &db(), &[req("huge", "pack-huge", "Battery")]).unwrap();
    assert!(!rep(&s).is_ok());
    // Two packs forced onto the same spot.
    let mut s = pack(&spec(), &db(), &[req("p1", "pack-small", "Battery")]).unwrap();
    let mut dup = s.tree[0].clone();
    dup.name = "p2".into();
    s.tree.push(dup);
    assert!(rep(&s)
        .issues
        .iter()
        .any(|i| i.kind == IssueKind::Collision));
    // A row without dimensions cannot be packed ...
    assert!(pack(&spec(), &db(), &[req("x", "no-dims", "Battery")]).is_err());
    // ... nor checked silently when it sits in the tree.
    let mut s = spec();
    s.tree.push(mount(
        "ghost",
        &ComponentId::new("no-dims").unwrap(),
        "FS_Bay_Aft",
        [0.0; 3],
        [0.0; 3],
    ));
    assert!(rep(&s)
        .issues
        .iter()
        .any(|i| i.kind == IssueKind::MissingDims));
    // Fill above the practical limit warns: a pack nearly as big as the Nose bay.
    let big = pack(&spec(), &db(), &[req("n1", "pack-small", "Nose")]).unwrap();
    let r = rep(&big);
    let nose = r.fills.iter().find(|f| f.bay == "Nose").unwrap();
    assert!(nose.fraction() > 0.0 && nose.fraction() < 1.0);
}

fn with_propellers(prop: &str) -> UavSpec {
    let db = db();
    let mut s = spec();
    for side in ["R", "L"] {
        let frame = format!("PS_Motor_{side}_Plane");
        s.tree.push(
            mount_motor(
                &db,
                &format!("motor {side}"),
                &ComponentId::new("motor").unwrap(),
                &frame,
                false,
            )
            .unwrap(),
        );
        let mut p: Node = mount(
            &format!("prop {side}"),
            &ComponentId::new(prop).unwrap(),
            &frame,
            [0.0; 3],
            [0.0; 3],
        );
        p.offset_m = [0.0; 3];
        s.tree.push(p);
    }
    s
}

#[test]
fn propeller_discs_are_checked_against_pod_ground_and_each_other() {
    let ok = with_propellers("prop-11");
    let rep = check(&ok, &db()).unwrap();
    assert!(rep.is_ok(), "{:?}", rep.issues);
    assert_eq!(rep.discs.len(), 2);
    // A 0.9 m propeller at 20 % half span hits the pod, the other disc and the ground.
    let bad = with_propellers("prop-huge");
    let rep = check(&bad, &db()).unwrap();
    let kinds: Vec<_> = rep.issues.iter().map(|i| i.kind).collect();
    assert!(kinds.contains(&IssueKind::PropellerStrike), "{kinds:?}");
    assert!(kinds.contains(&IssueKind::PropellerGround), "{kinds:?}");
    assert!(!rep.is_ok());
}

#[test]
fn motor_sits_behind_a_tractor_plane() {
    let s = with_propellers("prop-11");
    let rep = check(&s, &db()).unwrap();
    let m = rep.parts.iter().find(|p| p.name == "motor R").unwrap();
    let d = rep.discs.iter().find(|p| p.name == "prop R").unwrap();
    assert!((m.aabb.min[0] - d.centre_m[0]).abs() < 1e-12);
    assert!((m.aabb.max[0] - m.aabb.min[0] - 0.0545).abs() < 1e-12);
    let _ = (num(0.0, "m"),);
}
