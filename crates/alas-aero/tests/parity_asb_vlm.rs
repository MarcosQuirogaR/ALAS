// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::asb_vlm` against AeroSandbox's `VortexLatticeMethod`,
//! via `golden/generators/gen_aero_vlm.py`.
//!
//! `docs/PORTING.md` names `Tier::Linalg` for this row: the AIC assembly is
//! closed-form, but the solved circulation vector and everything built on it
//! have been through a dense linear solve, and the row is compared at the
//! tier that construction earns as a whole rather than splitting the
//! closed-form parts out to a tighter one.
//!
//! `vortex_strengths` is checked first and separately from the rest of
//! `run`'s output, per the module doc's own reasoning: a wrong AIC assembly
//! can still integrate to a coincidentally close total force, so the solved
//! circulation vector is the stronger check.
//!
//! The `fine_spanwise` case is the only one with `spanwise_resolution > 1`
//! and is therefore the only one exercising
//! `Wing::subdivide_sections`'s `SpacingFunction::Cosspace` branch through
//! this row -- see `alas-geom::asb::wing`'s module doc for the branch this
//! closes.
//!
//! # Forces and moments a symmetric condition drives to an exact zero
//!
//! A symmetric aircraft at zero sideslip and zero roll/yaw rate carries no
//! side force and no rolling or yawing moment: those quantities are exactly
//! zero in closed-form arithmetic (the `baseline` case is that condition).
//! In floating point they are not -- and the residual is *not* bounded by
//! this row's `Tier::Linalg` relative bound, because the reference value it
//! would be relative to is itself only machine-epsilon-scale noise. The AIC
//! solve agrees with LAPACK to far better than `Tier::Linalg` (the
//! `vortex_strengths` comparison confirms it), but that sub-tier residual,
//! near machine epsilon relative to the O(10^4 N) panel forces the near-field
//! integration is built from, still lands as an *absolute* difference around
//! 1e-12 N or Nm once ~15 panels' worth of those terms cancel to what should
//! be nothing -- and the moment terms, position-weighted, cancel from an even
//! larger magnitude. Comparing two independent near-zero floating-point sums
//! by relative tolerance is not a meaningful question; both sides clearing
//! [`NEGLIGIBLE`] -- a floor set eight orders of magnitude below any force or
//! moment of physical interest here, and six above the observed noise -- is.
//! This is a comparison-methodology decision local to this test, not a change
//! to the tier or its bounds; the non-degenerate cases (`sideslip`,
//! `rotation_rates`, `negative`, `fine_spanwise`, all with nonzero sideslip)
//! carry the real side-force/roll/yaw check at the full `Tier::Linalg`.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::asb_vlm::{self, VlmResult};
use alas_aero::operating_point::OperatingPoint;
use alas_atmo::Atmosphere;
use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Inputs {
    altitude_m: f64,
    velocity: f64,
    alpha: f64,
    beta: f64,
    p: f64,
    q: f64,
    r: f64,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
}

#[derive(Debug, Deserialize)]
struct RunResultFixture {
    force_geometry: [f64; 3],
    force_body: [f64; 3],
    force_wind: [f64; 3],
    moment_geometry: [f64; 3],
    moment_body: [f64; 3],
    moment_wind: [f64; 3],
    lift: f64,
    drag: f64,
    side_force: f64,
    roll_moment: f64,
    pitch_moment: f64,
    yaw_moment: f64,
    cl_lift: f64,
    cd_drag: f64,
    cy_side: f64,
    cl_roll: f64,
    cm_pitch: f64,
    cn_yaw: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    inputs: Inputs,
    vortex_strengths: Vec<f64>,
    result: RunResultFixture,
}

#[derive(Debug, Deserialize)]
struct DerivativesFixture {
    cl_lift: f64,
    cd_drag: f64,
    cy_side: f64,
    cl_roll: f64,
    cm_pitch: f64,
    cn_yaw: f64,
}

#[derive(Debug, Deserialize)]
struct StabilityFixture {
    inputs: Inputs,
    base: RunResultFixture,
    d_alpha: DerivativesFixture,
    d_beta: DerivativesFixture,
    d_p: DerivativesFixture,
    d_q: DerivativesFixture,
    d_r: DerivativesFixture,
    x_np: f64,
    x_np_lateral: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    xyz_ref: [f64; 3],
    s_ref: f64,
    c_ref: f64,
    b_ref: f64,
    cases: HashMap<String, Case>,
    stability_derivatives: StabilityFixture,
}

/// The same small 3-wing airplane `gen_aero_vlm.py`'s `_build_airplane`
/// constructs, kept in exact correspondence with it -- geometry this small
/// is reproduced literally rather than loaded from a shared fixture, the
/// same choice `gen_geom_asb_mesh.py` makes for its own probe wings.
fn build_airplane(fixture: &Fixture) -> Airplane {
    let naca4412 = Airfoil::from_name("naca4412").expect("valid 4-digit NACA name");
    let naca0012 = Airfoil::from_name("naca0012").expect("valid 4-digit NACA name");

    let main_wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 2.0, 2.0, naca4412.clone()),
            WingXSec::new([0.5, 4.0, 0.2], 1.0, 0.0, naca4412),
        ],
        true,
    );
    let hstab = Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([6.0, 0.0, 0.0], 1.0, -1.0, naca0012.clone()),
            WingXSec::new([6.2, 1.5, 0.05], 0.6, -1.0, naca0012.clone()),
        ],
        true,
    );
    let vstab = Wing::new(
        "Vertical Stabilizer",
        vec![
            WingXSec::new([6.0, 0.0, 0.0], 1.2, 0.0, naca0012.clone()),
            WingXSec::new([6.3, 0.0, 1.4], 0.7, 0.0, naca0012),
        ],
        false,
    );

    Airplane {
        name: "ALAS VLM Probe".to_owned(),
        xyz_ref: fixture.xyz_ref,
        wings: vec![main_wing, hstab, vstab],
        fuselages: Vec::new(),
        s_ref: fixture.s_ref,
        c_ref: fixture.c_ref,
        b_ref: fixture.b_ref,
    }
}

fn build_op_point(inputs: &Inputs) -> OperatingPoint {
    OperatingPoint::new(
        Atmosphere::new(inputs.altitude_m),
        inputs.velocity,
        inputs.alpha,
        inputs.beta,
        inputs.p,
        inputs.q,
        inputs.r,
    )
}

/// Below this magnitude a force (N) or moment (Nm) is floating-point noise
/// around a value a symmetric, no-sideslip condition drives to an exact zero
/// in closed form -- see the module doc.
const NEGLIGIBLE: f64 = 1e-6;

/// Compare one value, treating "both sides are noise around zero" as
/// agreement rather than asking a meaningless relative question -- see the
/// module doc.
fn compare_scalar(comparison: &mut Comparison, name: &str, actual: f64, expected: f64) {
    if actual.abs() < NEGLIGIBLE && expected.abs() < NEGLIGIBLE {
        return;
    }
    comparison.scalar(name, actual, expected);
}

/// Compare a 3-vector componentwise through [`compare_scalar`].
fn compare_vec3(comparison: &mut Comparison, name: &str, actual: [f64; 3], expected: [f64; 3]) {
    for i in 0..3 {
        compare_scalar(comparison, &format!("{name}[{i}]"), actual[i], expected[i]);
    }
}

fn compare_result(
    comparison: &mut Comparison,
    name: &str,
    actual: &VlmResult,
    expected: &RunResultFixture,
) {
    compare_vec3(
        comparison,
        &format!("{name}.force_geometry"),
        actual.force_geometry,
        expected.force_geometry,
    );
    compare_vec3(
        comparison,
        &format!("{name}.force_body"),
        actual.force_body,
        expected.force_body,
    );
    compare_vec3(
        comparison,
        &format!("{name}.force_wind"),
        actual.force_wind,
        expected.force_wind,
    );
    compare_vec3(
        comparison,
        &format!("{name}.moment_geometry"),
        actual.moment_geometry,
        expected.moment_geometry,
    );
    compare_vec3(
        comparison,
        &format!("{name}.moment_body"),
        actual.moment_body,
        expected.moment_body,
    );
    compare_vec3(
        comparison,
        &format!("{name}.moment_wind"),
        actual.moment_wind,
        expected.moment_wind,
    );
    compare_scalar(
        comparison,
        &format!("{name}.lift"),
        actual.lift,
        expected.lift,
    );
    compare_scalar(
        comparison,
        &format!("{name}.drag"),
        actual.drag,
        expected.drag,
    );
    compare_scalar(
        comparison,
        &format!("{name}.side_force"),
        actual.side_force,
        expected.side_force,
    );
    compare_scalar(
        comparison,
        &format!("{name}.roll_moment"),
        actual.roll_moment,
        expected.roll_moment,
    );
    compare_scalar(
        comparison,
        &format!("{name}.pitch_moment"),
        actual.pitch_moment,
        expected.pitch_moment,
    );
    compare_scalar(
        comparison,
        &format!("{name}.yaw_moment"),
        actual.yaw_moment,
        expected.yaw_moment,
    );
    // Coefficients are the dimensional quantities above divided by q*s_ref
    // (and b_ref/c_ref), which for the near-zero ones lands them far below
    // NEGLIGIBLE and so never reaches the relative comparison in the
    // degenerate case either; the non-degenerate cases carry the real check.
    compare_scalar(
        comparison,
        &format!("{name}.cl_lift"),
        actual.cl_lift,
        expected.cl_lift,
    );
    compare_scalar(
        comparison,
        &format!("{name}.cd_drag"),
        actual.cd_drag,
        expected.cd_drag,
    );
    compare_scalar(
        comparison,
        &format!("{name}.cy_side"),
        actual.cy_side,
        expected.cy_side,
    );
    compare_scalar(
        comparison,
        &format!("{name}.cl_roll"),
        actual.cl_roll,
        expected.cl_roll,
    );
    compare_scalar(
        comparison,
        &format!("{name}.cm_pitch"),
        actual.cm_pitch,
        expected.cm_pitch,
    );
    compare_scalar(
        comparison,
        &format!("{name}.cn_yaw"),
        actual.cn_yaw,
        expected.cn_yaw,
    );
}

#[test]
fn vortex_strengths_match_aerosandbox_for_every_case() {
    let fixture: Fixture = alas_testkit::load("aero", "asb_vlm");
    let airplane = build_airplane(&fixture);
    let mut comparison = Comparison::new("alas-aero::asb_vlm (vortex_strengths)", Tier::Linalg);

    for (name, case) in &fixture.cases {
        let op_point = build_op_point(&case.inputs);
        let result = asb_vlm::run_reference_compatibility(
            &airplane,
            &op_point,
            case.inputs.spanwise_resolution,
            case.inputs.chordwise_resolution,
        )
        .expect("every fixture case is a well-posed solve");
        comparison.slice(
            &format!("{name}.vortex_strengths"),
            &result.vortex_strengths,
            &case.vortex_strengths,
        );
    }
    comparison.finish();
}

#[test]
fn run_matches_aerosandbox_for_every_case() {
    let fixture: Fixture = alas_testkit::load("aero", "asb_vlm");
    let airplane = build_airplane(&fixture);
    let mut comparison = Comparison::new("alas-aero::asb_vlm (run)", Tier::Linalg);

    for (name, case) in &fixture.cases {
        let op_point = build_op_point(&case.inputs);
        let result = asb_vlm::run_reference_compatibility(
            &airplane,
            &op_point,
            case.inputs.spanwise_resolution,
            case.inputs.chordwise_resolution,
        )
        .expect("every fixture case is a well-posed solve");
        compare_result(&mut comparison, name, &result, &case.result);
    }
    comparison.finish();
}

/// Compare one axis's six coefficient derivatives. Every entry is
/// structurally nonzero for the fixture's alpha=4/beta=3 base point (the
/// smallest is a few times 1e-3, orders above `Tier::Linalg`'s absolute
/// floor), so a plain relative comparison frames each one -- no near-zero
/// methodology is needed here, unlike the degenerate `run` cases above.
fn compare_derivatives(
    comparison: &mut Comparison,
    name: &str,
    actual: &asb_vlm::CoefficientDerivatives,
    expected: &DerivativesFixture,
) {
    comparison.scalar(&format!("{name}.cl_lift"), actual.cl_lift, expected.cl_lift);
    comparison.scalar(&format!("{name}.cd_drag"), actual.cd_drag, expected.cd_drag);
    comparison.scalar(&format!("{name}.cy_side"), actual.cy_side, expected.cy_side);
    comparison.scalar(&format!("{name}.cl_roll"), actual.cl_roll, expected.cl_roll);
    comparison.scalar(
        &format!("{name}.cm_pitch"),
        actual.cm_pitch,
        expected.cm_pitch,
    );
    comparison.scalar(&format!("{name}.cn_yaw"), actual.cn_yaw, expected.cn_yaw);
}

#[test]
fn run_with_stability_derivatives_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("aero", "asb_vlm");
    let airplane = build_airplane(&fixture);
    let case = &fixture.stability_derivatives;
    let op_point = build_op_point(&case.inputs);

    let result = asb_vlm::run_with_stability_derivatives_reference_compatibility(
        &airplane,
        &op_point,
        case.inputs.spanwise_resolution,
        case.inputs.chordwise_resolution,
    )
    .expect("the stability-derivative sweep is a well-posed set of solves");

    let mut comparison = Comparison::new(
        "alas-aero::asb_vlm (run_with_stability_derivatives)",
        Tier::Linalg,
    );
    // The base point is a plain `run`, checked the same way the `run` cases
    // are.
    compare_result(&mut comparison, "base", &result.base, &case.base);
    compare_derivatives(&mut comparison, "d_alpha", &result.d_alpha, &case.d_alpha);
    compare_derivatives(&mut comparison, "d_beta", &result.d_beta, &case.d_beta);
    compare_derivatives(&mut comparison, "d_p", &result.d_p, &case.d_p);
    compare_derivatives(&mut comparison, "d_q", &result.d_q, &case.d_q);
    compare_derivatives(&mut comparison, "d_r", &result.d_r, &case.d_r);
    comparison.scalar("x_np", result.x_np, case.x_np);
    comparison.scalar("x_np_lateral", result.x_np_lateral, case.x_np_lateral);
    comparison.finish();
}

#[test]
fn the_fixture_exercises_the_cosspace_subdivide_branch() {
    // A fixture that never used spanwise_resolution > 1 would not catch a
    // wrong spacing function in Wing::subdivide_sections -- see the module
    // doc and CLAUDE.md's brief for this row.
    let fixture: Fixture = alas_testkit::load("aero", "asb_vlm");
    assert!(
        fixture
            .cases
            .values()
            .any(|case| case.inputs.spanwise_resolution > 1),
        "no fixture case uses spanwise_resolution > 1"
    );
}
