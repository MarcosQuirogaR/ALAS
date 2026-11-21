// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The small-angle boundary-condition image of a stabilizer incidence change
//! (`VlmSystem::solve_with_surface_pitch`) against the exact lattice rebuild.
//!
//! Frames: geometry axes (x aft, y right, z up); angles in degrees at the
//! operating point, radians in the pitch argument; positive pitch is nose-up.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

const TAIL: usize = 1;

fn airplane(tail_incidence_deg: f64) -> Airplane {
    let airfoil = Airfoil::from_name("naca0012").expect("valid name");
    let wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 1.0, 2.0, airfoil.clone()),
            WingXSec::new([0.3, 4.0, 0.1], 0.6, 0.0, airfoil.clone()),
        ],
        true,
    );
    let mut tail = Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([4.5, 0.0, 0.2], 0.6, 0.0, airfoil.clone()),
            WingXSec::new([4.7, 1.5, 0.2], 0.4, 0.0, airfoil),
        ],
        true,
    );
    tail.twist_about_pitch_axis = true;
    for xsec in &mut tail.xsecs {
        xsec.twist = tail_incidence_deg;
    }
    let (s, b, c) = (
        wing.reference_area(),
        wing.reference_span(),
        wing.mean_aerodynamic_chord(),
    );
    Airplane {
        name: "Probe".to_owned(),
        xyz_ref: [0.3, 0.0, 0.0],
        wings: vec![wing, tail],
        fuselages: Vec::new(),
        s_ref: s,
        c_ref: c,
        b_ref: b,
    }
}

fn point(alpha_deg: f64) -> OperatingPoint {
    OperatingPoint::new(Atmosphere::new(0.0), 60.0, alpha_deg, 0.0, 0.0, 0.0, 0.0)
}

#[test]
fn zero_pitch_reproduces_the_plain_solve_bit_for_bit() {
    let plane = airplane(1.0);
    let system = VlmSystem::assemble(&plane, 2, 4).unwrap();
    // A system's first solve is direct and its later ones superposed; solve
    // once so that both compared solves take the same path.
    system.solve(&point(1.0)).unwrap();
    let plain = system.solve(&point(3.0)).unwrap();
    let zero = system
        .solve_with_surface_pitch(&point(3.0), &[TAIL], 0.0)
        .unwrap();
    assert_eq!(plain.cl_lift.to_bits(), zero.cl_lift.to_bits());
    assert_eq!(plain.cm_pitch.to_bits(), zero.cm_pitch.to_bits());
}

#[test]
fn a_small_pitch_follows_the_rebuilt_tail_to_first_order() {
    let base = airplane(1.0);
    let system = VlmSystem::assemble(&base, 2, 4).unwrap();
    let before = system.solve(&point(3.0)).unwrap();
    for delta_deg in [-1.0_f64, 0.5, 1.0] {
        let rotated = system
            .solve_with_surface_pitch(&point(3.0), &[TAIL], delta_deg.to_radians())
            .unwrap();
        let rebuilt_plane = airplane(1.0 + delta_deg);
        let rebuilt = VlmSystem::assemble(&rebuilt_plane, 2, 4)
            .unwrap()
            .solve(&point(3.0))
            .unwrap();
        let exact_change = rebuilt.cm_pitch - before.cm_pitch;
        let model_change = rotated.cm_pitch - before.cm_pitch;
        // The nose-up tail pitch moves the moment the same way in both, and
        // the vortex-position effect the model leaves out is a few percent
        // of the change, not its sign or its size.
        assert!(exact_change * delta_deg < 0.0, "tail lift is nose-down");
        assert!(
            (model_change - exact_change).abs() < 0.06 * exact_change.abs(),
            "delta {delta_deg}: model {model_change}, rebuilt {exact_change}"
        );
        let exact_cl = rebuilt.cl_lift - before.cl_lift;
        let model_cl = rotated.cl_lift - before.cl_lift;
        assert!((model_cl - exact_cl).abs() < 0.06 * exact_cl.abs());
    }
}

#[test]
fn a_non_finite_pitch_is_an_error() {
    let plane = airplane(0.0);
    let system = VlmSystem::assemble(&plane, 1, 2).unwrap();
    assert!(system
        .solve_with_surface_pitch(&point(2.0), &[TAIL], f64::NAN)
        .is_err());
}
