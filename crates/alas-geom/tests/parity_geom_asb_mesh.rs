// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::asb::mesh` against AeroSandbox's
//! `Wing.mesh_thin_surface`/`Wing.mesh_line`, via
//! `golden/generators/gen_geom_asb_mesh.py`.
//!
//! `points` and `mesh_line`'s output are closed-form arithmetic over
//! already-`closed`-tier wing geometry (no factorization, spline or solve in
//! this row -- `Airfoil::local_camber`'s `np.interp` is the same
//! interpolation `local_thickness` already uses at `closed`), checked at
//! `Tier::Closed`, the tier `docs/PORTING.md` names for this row. `faces` is
//! an integer index array and is checked at `Tier::Exact`.
//!
//! Geometry is rebuilt here from the same literal `DesignVector`/
//! `GeometryConfig` defaults `parity_asb_wing.rs` already uses for
//! `main_wing`/`vstab`, mirroring that file's pattern rather than depending
//! on a builder this phase does not translate.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::mesh::XsecStation;
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MeshCase {
    chordwise_resolution: usize,
    points: Vec<[f64; 3]>,
    faces: Vec<[usize; 4]>,
}

#[derive(Debug, Deserialize)]
struct MeshLineCase {
    x_nondim: f64,
    points: Vec<[f64; 3]>,
}

#[derive(Debug, Deserialize)]
struct MeshLineFixture {
    cases: HashMap<String, MeshLineCase>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    mesh_thin_surface: HashMap<String, MeshCase>,
    mesh_line: MeshLineFixture,
}

/// Shaped like `parity_asb_wing.rs`'s `build_main_wing`, rebuilt here rather
/// than shared across test binaries (integration tests do not share a
/// crate).
fn build_main_wing() -> Wing {
    let root_z_m = -2.1;
    let break_z_m = -0.3;
    let tip_z_m = 2.5;
    let root_twist_deg = 4.0;
    let break_twist_deg = 2.0;
    let break_span_fraction = 0.35;
    let outboard_sweep_decrement_deg: f64 = 2.0;

    let span_m: f64 = 71.75;
    let root_chord_m = 16.50;
    let break_chord_m = 7.80;
    let tip_chord_m = 1.60;
    let sweep_deg: f64 = 34.00;
    let tip_twist_deg = 0.00;

    let semi_span = span_m / 2.0;
    let y_break = break_span_fraction * semi_span;
    let sweep_in = sweep_deg.to_radians();
    let sweep_out = (sweep_deg - outboard_sweep_decrement_deg).to_radians();
    let dx_break = y_break * sweep_in.tan();
    let dx_tip = dx_break + (semi_span - y_break) * sweep_out.tan();

    let root_section = Airfoil::from_name("naca4412").expect("naca4412 parses");
    let tip_airfoil = Airfoil::from_name("naca2410").expect("naca2410 parses");

    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new(
                [0.0, 0.0, root_z_m],
                root_chord_m,
                root_twist_deg,
                root_section.clone(),
            ),
            WingXSec::new(
                [dx_break, y_break, break_z_m],
                break_chord_m,
                break_twist_deg,
                root_section,
            ),
            WingXSec::new(
                [dx_tip, semi_span, tip_z_m],
                tip_chord_m,
                tip_twist_deg,
                tip_airfoil,
            ),
        ],
        true,
    )
}

fn build_vstab() -> Wing {
    let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
    Wing::new(
        "Vertical Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 9.5, 0.0, tail_airfoil.clone()),
            WingXSec::new([9.0, 0.0, 9.8], 3.2, 0.0, tail_airfoil),
        ],
        false,
    )
}

fn compare_mesh(
    comparison: &mut Comparison,
    faces: &mut Comparison,
    label: &str,
    wing: &Wing,
    expected: &MeshCase,
) {
    let (points, computed_faces) = wing.mesh_thin_surface(expected.chordwise_resolution, true);

    if points.len() != expected.points.len() {
        comparison.exact(
            &format!("{label}.points (count)"),
            &points.len(),
            &expected.points.len(),
        );
    } else {
        for (index, (actual, expected_point)) in points.iter().zip(&expected.points).enumerate() {
            comparison.slice(&format!("{label}.points[{index}]"), actual, expected_point);
        }
    }

    if computed_faces.len() != expected.faces.len() {
        faces.exact(
            &format!("{label}.faces (count)"),
            &computed_faces.len(),
            &expected.faces.len(),
        );
    } else {
        for (index, (actual, expected_face)) in
            computed_faces.iter().zip(&expected.faces).enumerate()
        {
            faces.exact(&format!("{label}.faces[{index}]"), actual, expected_face);
        }
    }
}

#[test]
fn mesh_thin_surface_matches_aerosandbox_on_a_symmetric_and_an_asymmetric_wing() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_mesh");

    let mut points_comparison = Comparison::new(
        "alas-geom::asb::mesh (mesh_thin_surface points)",
        Tier::Closed,
    );
    let mut faces_comparison = Comparison::new(
        "alas-geom::asb::mesh (mesh_thin_surface faces)",
        Tier::Exact,
    );

    compare_mesh(
        &mut points_comparison,
        &mut faces_comparison,
        "main_wing",
        &build_main_wing(),
        &fixture.mesh_thin_surface["main_wing"],
    );
    compare_mesh(
        &mut points_comparison,
        &mut faces_comparison,
        "vstab",
        &build_vstab(),
        &fixture.mesh_thin_surface["vstab"],
    );

    points_comparison.finish();
    faces_comparison.finish();
}

#[test]
fn mesh_line_matches_aerosandbox_at_a_zero_and_a_nonzero_x_nondim() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_mesh");
    let wing = build_main_wing();

    let mut comparison = Comparison::new("alas-geom::asb::mesh (mesh_line)", Tier::Closed);
    for (key, case) in &fixture.mesh_line.cases {
        let points = wing
            .mesh_line(
                XsecStation::Scalar(case.x_nondim),
                XsecStation::Scalar(0.0),
                true,
            )
            .expect("scalar stations never mismatch a length");
        if points.len() != case.points.len() {
            comparison.exact(
                &format!("mesh_line[{key}] (point count)"),
                &points.len(),
                &case.points.len(),
            );
            continue;
        }
        for (index, (actual, expected)) in points.iter().zip(&case.points).enumerate() {
            comparison.slice(&format!("mesh_line[{key}][{index}]"), actual, expected);
        }
    }
    comparison.finish();
}
