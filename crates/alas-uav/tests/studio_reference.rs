// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Xtra2UPV reference aircraft built from a spec: packaging, markers,
//! scene data and the live feasibility lights. Implementation checks, not
//! physical validation (see the module doc of `studio::reference`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use std::time::Instant;

use alas_uav::studio::check::check;
use alas_uav::studio::feasibility::{l0, l1, Area, Light};
use alas_uav::studio::hull::{derive_hull, with_explicit_empennage};
use alas_uav::studio::markers::{markers, mass_inputs_for};
use alas_uav::studio::reference::xtra2upv;
use alas_uav::studio::scene::build_scene;

#[test]
fn reference_builds_packs_and_passes_the_geometric_checks() {
    let r = xtra2upv().unwrap();
    r.spec.validate().unwrap();
    let rep = check(&r.spec, &r.db).unwrap();
    assert!(rep.is_ok(), "{:?}", rep.issues);
    // 2 ESC + pack + receiver + 2 motors + servos in the bays and on the seats.
    assert!(rep.parts.len() >= 6, "{}", rep.parts.len());
    assert_eq!(rep.discs.len(), 2);
    assert!((rep.discs[0].radius_m - 0.5 * 11.0 * 0.0254).abs() < 1e-9);
    assert_eq!(r.substitutions.len(), 4);
}

#[test]
fn reference_mass_cg_and_static_margin() {
    let r = xtra2upv().unwrap();
    let inputs = mass_inputs_for(&r.spec);
    let m = markers(&r.spec, &r.db, &inputs, &r.cases).unwrap();
    let total = m.mass.total_mass_kg();
    let payload = m.mass.group_mass_kg(alas_uav::airframe::MassGroup::Payload);
    assert!((payload - 3.15).abs() < 1e-12);
    // Bottom-up empty mass against the report's 2.917 kg (CAD) and 3.2 kg (built).
    let oew = total - payload;
    eprintln!(
        "xtra28 bottom-up OEW {oew:.3} kg (published {} / {}); estimate share {:.0} %",
        r.published_oew_estimate_kg,
        r.published_oew_built_kg,
        100.0 * m.mass.estimate_fraction()
    );
    assert!(oew > 1.5 && oew < 5.0, "{oew}");
    for s in &m.stability {
        eprintln!(
            "  {}: m {:.2} kg, CG {:.3}, NP {:.3}, SM {:.3}",
            s.case, s.mass_kg, s.x_cg_m, m.neutral_point.x_np_m, s.static_margin
        );
    }
    assert_eq!(m.stability.len(), 3);
    // The reference wing was slid to a 10 % smallest margin.
    let min = m
        .stability
        .iter()
        .map(|s| s.static_margin)
        .fold(f64::INFINITY, f64::min);
    assert!((min - 0.10).abs() < 1e-3, "{min}");
    // More payload at the bay centre moves the CG towards the payload bay.
    let cg = |i: usize| m.stability[i].x_cg_m;
    assert!((cg(0) - cg(1)).abs() > 1e-4);
    assert!(m.neutral_point.x_np_m > m.neutral_point.x_ac_wing_m - 0.1);
    let kinds: Vec<_> = m.markers.iter().map(|k| k.label.clone()).collect();
    assert!(kinds.iter().any(|k| k == "NP") && kinds.iter().any(|k| k == "CG 9 cans"));
}

#[test]
fn scene_carries_boxes_outlines_and_markers() {
    let r = xtra2upv().unwrap();
    let s = build_scene(&r.spec, &r.db, &mass_inputs_for(&r.spec), &r.cases).unwrap();
    assert!(s.envelope.is_some() && s.bays.len() == 4);
    assert!(s.outlines.len() >= 4, "wing and V pair, both sides");
    assert!(s.boom.is_some() && !s.lines.is_empty());
    assert!(s.static_margin.is_finite());
    assert!(s.outlines.iter().all(|o| o.points_m.len() >= 4));
}

#[test]
fn hull_is_derived_from_the_layout() {
    let r = xtra2upv().unwrap();
    let h = derive_hull(&r.spec, Some(r.motor_diameter_m)).unwrap();
    let f = h.fuselage.unwrap();
    assert!((f.length_m - 0.85).abs() < 1e-12);
    assert!((f.diameter_m - (4.0 * 0.19 * 0.12 / std::f64::consts::PI).sqrt()).abs() < 1e-12);
    assert_eq!(h.motors.len(), 2);
    assert!(h.motors.iter().all(|m| m.s_to_wing_m > 0.0));
    let e = with_explicit_empennage(&r.spec).unwrap();
    assert_eq!(e, r.spec, "an explicit empennage is unchanged");
}

#[test]
fn l0_lights_run_fast_and_are_explained() {
    let r = xtra2upv().unwrap();
    let inp = r.inputs();
    let t = Instant::now();
    let rep = l0(&inp).unwrap();
    eprintln!("L0 in {:?}", t.elapsed());
    for i in &rep.items {
        eprintln!(
            "  {:?} {:?} {}: {:?} / {:?} {} - {}",
            i.area, i.light, i.label, i.value, i.limit, i.unit, i.note
        );
    }
    assert_eq!(rep.level, 0);
    assert!(rep
        .items
        .iter()
        .any(|i| i.area == Area::Packaging && i.light == Light::Green));
    assert!(rep
        .items
        .iter()
        .any(|i| i.label.starts_with("static margin")));
    assert!(rep
        .items
        .iter()
        .any(|i| i.label == "static thrust / weight"));
    // The ESC (40 A) carries 15 A: green; a stall estimate on an assumed CLmax is never green.
    let stall = rep
        .items
        .iter()
        .find(|i| i.label == "stall speed (L0)")
        .unwrap();
    assert_ne!(stall.light, Light::Green);
}

#[test]
fn l1_runs_the_coupled_chain_and_honours_cancel() {
    let r = xtra2upv().unwrap();
    let inp = r.inputs();
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let early = l1(&inp, Some(&cancel)).unwrap();
    assert_eq!(early.level, 1);
    assert!(!early.items.iter().any(|i| i.label == "stall speed (L1)"));
    let t = Instant::now();
    let rep = l1(&inp, None).unwrap();
    eprintln!("L1 in {:?}", t.elapsed());
    for i in &rep.items {
        eprintln!(
            "  {:?} {:?} {}: {:?} / {:?} {}",
            i.area, i.light, i.label, i.value, i.limit, i.unit
        );
    }
    let stall = rep
        .items
        .iter()
        .find(|i| i.label == "stall speed (L1)")
        .unwrap();
    // Published team prediction 11.9 m/s at 9 cans; the M2 blind band is +-8 %.
    let v = stall.value.unwrap();
    assert!(v > 9.0 && v < 14.5, "{v}");
    assert!(rep.items.iter().any(|i| i.label == "take-off ground roll"));
}
