// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The strip and incidence-offset accessors the UAV viscous coupling uses.
//!
//! Frames: geometry axes (x aft, y right, z up); angles in the operating
//! point in degrees, offsets in radians.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::strips::wing_strips;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

fn tapered_airplane() -> Airplane {
    let airfoil = Airfoil::from_name("naca2412").expect("valid name");
    let wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 0.30, 0.0, airfoil.clone()),
            WingXSec::new([0.03, 0.6, 0.02], 0.24, -1.0, airfoil.clone()),
            WingXSec::new([0.06, 1.0, 0.04], 0.15, -2.0, airfoil),
        ],
        true,
    );
    let (s, b, c) = (
        wing.reference_area(),
        wing.reference_span(),
        wing.mean_aerodynamic_chord(),
    );
    Airplane {
        name: "Probe".to_owned(),
        xyz_ref: [0.07, 0.0, 0.0],
        wings: vec![wing],
        fuselages: Vec::new(),
        s_ref: s,
        c_ref: c,
        b_ref: b,
    }
}

fn point(alpha_deg: f64) -> OperatingPoint {
    OperatingPoint::new(Atmosphere::new(0.0), 20.0, alpha_deg, 0.0, 0.0, 0.0, 0.0)
}

#[test]
fn zero_offsets_reproduce_the_plain_solve_bit_for_bit() {
    let airplane = tapered_airplane();
    let system = VlmSystem::assemble(&airplane, 3, 4).unwrap();
    let op = point(4.0);
    let plain = system.solve(&op).unwrap();
    let zero = system
        .solve_with_incidence_offsets(&op, &vec![0.0; system.panel_count()])
        .unwrap();
    for (a, b) in plain.vortex_strengths.iter().zip(&zero.vortex_strengths) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
    assert_eq!(plain.cl_lift.to_bits(), zero.cl_lift.to_bits());
}

#[test]
fn a_uniform_positive_offset_acts_like_the_same_angle_of_attack() {
    let airplane = tapered_airplane();
    let system = VlmSystem::assemble(&airplane, 3, 4).unwrap();
    let delta = 0.02_f64;
    let shifted = system
        .solve_with_incidence_offsets(&point(3.0), &vec![delta; system.panel_count()])
        .unwrap();
    let reference = system.solve(&point(3.0 + delta.to_degrees())).unwrap();
    let base = system.solve(&point(3.0)).unwrap();
    // Positive offset raises lift, symmetric loading keeps the roll moment at zero.
    assert!(shifted.cl_lift > base.cl_lift);
    assert!(shifted.roll_moment.abs() < 1e-9 * shifted.lift.abs().max(1.0));
    // Only the flow-tangency condition is rotated; the force is still taken on
    // the 3 deg freestream, so agreement with a true alpha shift is first order
    // in delta (the lift axis differs by delta: relative error ~ delta^2/2 * tan).
    let rel = (shifted.cl_lift - reference.cl_lift).abs() / reference.cl_lift;
    assert!(rel < 0.01, "relative difference {rel}");
}

#[test]
fn strips_partition_the_main_wing_and_their_lift_adds_up() {
    let airplane = tapered_airplane();
    let system = VlmSystem::assemble(&airplane, 3, 4).unwrap();
    let op = point(5.0);
    let result = system.solve(&op).unwrap();
    let strips = wing_strips(&result, &op, 0);
    // 2 spanwise sections x 3 subdivisions x 2 halves.
    assert_eq!(strips.len(), 2 * 2 * 3);
    let mut next = 0;
    let mut sum = 0.0;
    for s in &strips {
        assert_eq!(s.first_panel, next);
        assert_eq!(s.panel_count, 4);
        assert!(s.width_m > 0.0 && s.chord_m > 0.0);
        next += s.panel_count;
        sum += s.lift_n;
    }
    assert_eq!(next, system.panel_count());
    assert!((sum - result.lift).abs() < 1e-9 * result.lift.abs());
    // Both halves are present and the chord shrinks toward the tip.
    assert!(strips.iter().any(|s| s.y_m < 0.0) && strips.iter().any(|s| s.y_m > 0.0));
    let root = strips
        .iter()
        .min_by(|a, b| a.y_m.abs().total_cmp(&b.y_m.abs()))
        .unwrap();
    let tip = strips
        .iter()
        .max_by(|a, b| a.y_m.abs().total_cmp(&b.y_m.abs()))
        .unwrap();
    assert!(root.chord_m > tip.chord_m);
}

#[test]
fn an_offset_on_one_strip_changes_only_the_load_near_it() {
    let airplane = tapered_airplane();
    let system = VlmSystem::assemble(&airplane, 3, 4).unwrap();
    let op = point(2.0);
    let base = system.solve(&op).unwrap();
    let strips = wing_strips(&base, &op, 0);
    let target = strips.iter().find(|s| s.y_m > 0.0).unwrap();
    let mut offsets = vec![0.0; system.panel_count()];
    for o in &mut offsets[target.first_panel..target.first_panel + target.panel_count] {
        *o = 0.02;
    }
    let bumped = system.solve_with_incidence_offsets(&op, &offsets).unwrap();
    let after = wing_strips(&bumped, &op, 0);
    let gain = |i: usize| after[i].lift_n - strips[i].lift_n;
    let index = strips
        .iter()
        .position(|s| s.first_panel == target.first_panel)
        .unwrap();
    assert!(gain(index) > 0.0);
    // The bumped strip gains more than any strip that is not adjacent.
    for (i, s) in strips.iter().enumerate() {
        if (s.first_panel as i64 - target.first_panel as i64).abs() > 2 * 4 {
            assert!(gain(i).abs() < gain(index));
        }
    }
    assert!(bumped.cl_lift > base.cl_lift);
}

#[test]
fn offsets_of_the_wrong_length_or_not_finite_are_rejected() {
    let airplane = tapered_airplane();
    let system = VlmSystem::assemble(&airplane, 1, 2).unwrap();
    let op = point(2.0);
    assert!(system.solve_with_incidence_offsets(&op, &[0.0]).is_err());
    let mut bad = vec![0.0; system.panel_count()];
    bad[0] = f64::NAN;
    assert!(system.solve_with_incidence_offsets(&op, &bad).is_err());
}
