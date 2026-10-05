// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bit-identity guard for the performance work on `vorlax.rs`, `analysis.rs`,
//! `analysis/wave.rs` and `vlm/system.rs`.
//!
//! Each test here serializes every `f64`/`f32` output of the touched call
//! paths through [`to_bits`], not through `{:?}`'s rounded display, and
//! compares the joined string against a baseline file captured from the code
//! before that day's change. A single differing bit anywhere in the string
//! fails the test. The baseline files live in `tests/fixtures/perf_aero/` and
//! are checked in so this remains a real regression test after the
//! performance work lands, not only a tool used once while writing it.
//!
//! Panel counts are chosen deliberately: 12 (well under both the 128-panel
//! rayon threshold and the vorlax/VLM kernel-cache size) and 96 spanwise x 3
//! chordwise = 576-panel and 3-wing meshes that cross the 128-panel
//! threshold, so the parallel paths are exercised bit-for-bit, not only the
//! serial fallback.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::path::Path;

use alas_aero::analysis::AeroAnalysis;
use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmSystem};
use alas_aero::vorlax::{self, VlmCondition, VlmGeometry, VlmSettings, VlmWing};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::wing::{Wing, WingXSec};

fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("valid 4-digit NACA name")
}

/// Append `value`'s bit pattern to `out`, one line per call: the test's own
/// serialization primitive, used for every scalar this file checks.
fn push_bits(out: &mut String, label: &str, value: f64) {
    let _ = writeln!(out, "{label} = {:#018x}", value.to_bits());
}

fn push_bits32(out: &mut String, label: &str, value: f32) {
    let _ = writeln!(out, "{label} = {:#010x}", value.to_bits());
}

/// Compare `actual` against the checked-in baseline at `name`, byte for
/// byte, ignoring line-ending style (a checkout may rewrite it). Run with
/// `PERF_AERO_WRITE_BASELINE=1` to (re)write it.
fn check_baseline(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/perf_aero")
        .join(name);
    if std::env::var_os("PERF_AERO_WRITE_BASELINE").is_some() {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        std::fs::write(&path, actual).expect("write baseline");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read baseline {}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        actual,
        expected,
        "bit-identity regression against {}",
        path.display()
    );
}

/// Like [`check_baseline`], but off Windows the baseline is compared to a
/// numeric tolerance instead of bit for bit.
///
/// The baseline was captured on a Windows host. The vorlax solve is a dense LU
/// whose last bits follow the host's math runtime and SIMD dispatch, so a
/// Linux runner reproduces every value to rounding but not every bit: the
/// hosted ubuntu run fails the bit comparison while hosted Windows passes it.
/// Windows keeps the strict bit check; elsewhere the tolerances are far below
/// any change the training grid could show from a real regression.
///
/// - `residual_norm` / `normalized_residual` are roundoff-level (about 1e-14
///   and 1e-16): absolute tolerance 1e-10.
/// - Other `f64` values: relative 1e-9 with an absolute floor of 1e-12.
/// - `f32` values (the 10-hex-digit lines, `gamma`): relative 1e-4 with an
///   absolute floor of 1e-6.
fn check_baseline_host_tolerant(name: &str, actual: &str) {
    if cfg!(windows) || std::env::var_os("PERF_AERO_WRITE_BASELINE").is_some() {
        check_baseline(name, actual);
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/perf_aero")
        .join(name);
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read baseline {}: {e}", path.display()))
        .replace("\r\n", "\n");
    let parse = |line: &str| -> (String, f64, bool) {
        let (label, hex) = line.split_once(" = ").expect("`label = 0x...` line");
        let digits = hex.trim().trim_start_matches("0x");
        let bits = u64::from_str_radix(digits, 16).expect("hex bit pattern");
        if digits.len() == 8 {
            (
                label.to_string(),
                f64::from(f32::from_bits(bits as u32)),
                true,
            )
        } else {
            (label.to_string(), f64::from_bits(bits), false)
        }
    };
    let actual_lines: Vec<&str> = actual.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();
    assert_eq!(
        actual_lines.len(),
        expected_lines.len(),
        "baseline {} has a different number of values",
        path.display()
    );
    for (got_line, want_line) in actual_lines.iter().zip(&expected_lines) {
        let (label, got, single) = parse(got_line);
        let (want_label, want, _) = parse(want_line);
        assert_eq!(label, want_label, "baseline order changed");
        let tolerance =
            if label.ends_with("residual_norm") || label.ends_with("normalized_residual") {
                1.0e-10
            } else if single {
                1.0e-4 * want.abs().max(1.0e-2)
            } else {
                1.0e-9 * want.abs() + 1.0e-12
            };
        assert!(
            got.is_finite() && (got - want).abs() <= tolerance,
            "{label}: {got:e} against baseline {want:e} (tolerance {tolerance:e})"
        );
    }
}

fn probe_wing(tag: &str, symmetric: bool, span: f64) -> VlmWing {
    VlmWing {
        tag: tag.to_string(),
        symmetric,
        vertical: false,
        vortex_lift: false,
        span_projected_m: span,
        chord_root_m: 2.2,
        chord_tip_m: 1.1,
        taper: 0.5,
        aspect_ratio: span * span / 7.5,
        sweep_quarter_chord_rad: 0.15,
        sweep_leading_edge_rad: None,
        twist_root_rad: 0.015,
        twist_tip_rad: -0.008,
        dihedral_rad: 0.02,
        area_reference_m2: 7.5,
        origin_m: [0.0, 0.0, 0.0],
    }
}

fn probe_vorlax_geometry() -> VlmGeometry {
    VlmGeometry {
        reference_area_m2: 7.5,
        center_of_gravity_m: [0.0, 0.0, 0.0],
        mean_aerodynamic_chord_m: 1.6,
        reference_span_m: 10.0,
        moment_reference_m: [0.5, 0.0],
        wings: vec![
            probe_wing("main_wing", true, 10.0),
            probe_wing("tail", true, 4.0),
        ],
    }
}

fn level(alpha_deg: f64, mach: f64) -> VlmCondition {
    VlmCondition {
        angle_of_attack_rad: alpha_deg.to_radians(),
        mach,
        side_slip_angle_rad: 0.02,
        pitch_rate_rad_s: 0.01,
        roll_rate_rad_s: -0.005,
        yaw_rate_rad_s: 0.002,
        velocity_m_s: 68.0,
    }
}

/// Task 1: the mission lift-surrogate training grid, hoisted `sin_cos` and
/// rayon-parallel Mach groups.
#[test]
fn vorlax_training_grid_is_bit_identical() {
    let geometry = probe_vorlax_geometry();
    let settings = VlmSettings::default();
    // Several Mach numbers, each with several angles of attack sharing that
    // Mach's influence matrix, and one Mach revisited out of order later in
    // the list so grouping and result order are both exercised.
    let conditions: Vec<VlmCondition> = [0.1, 0.3, 0.5, 0.7, 0.85, 0.3]
        .into_iter()
        .flat_map(|mach| {
            [-4.0, -1.0, 0.0, 2.0, 5.0, 8.0]
                .into_iter()
                .map(move |alpha| level(alpha, mach))
        })
        .collect();

    let results = vorlax::run(&geometry, &settings, &conditions).expect("solves");
    let mut out = String::new();
    for (i, diagnostics) in results.solve_diagnostics.iter().enumerate() {
        push_bits(
            &mut out,
            &format!("diag[{i}].residual_norm"),
            diagnostics.residual_norm,
        );
        push_bits(
            &mut out,
            &format!("diag[{i}].normalized_residual"),
            diagnostics.normalized_residual,
        );
        push_bits(
            &mut out,
            &format!("diag[{i}].pivot_ratio"),
            diagnostics.pivot_ratio,
        );
        push_bits(
            &mut out,
            &format!("diag[{i}].minimum_pivot"),
            diagnostics.minimum_pivot,
        );
    }
    for (i, case) in results.cases.iter().enumerate() {
        push_bits(&mut out, &format!("case[{i}].cl"), case.cl);
        push_bits(&mut out, &format!("case[{i}].cdi"), case.cdi);
        push_bits(&mut out, &format!("case[{i}].cm"), case.cm);
        push_bits(&mut out, &format!("case[{i}].cytot"), case.cytot);
        push_bits(&mut out, &format!("case[{i}].crtot"), case.crtot);
        push_bits(&mut out, &format!("case[{i}].crmtot"), case.crmtot);
        push_bits(&mut out, &format!("case[{i}].cntot"), case.cntot);
        push_bits(&mut out, &format!("case[{i}].cymtot"), case.cymtot);
        for (j, &v) in case.cl_wing.iter().enumerate() {
            push_bits(&mut out, &format!("case[{i}].cl_wing[{j}]"), v);
        }
        for (j, &v) in case.cdi_wing.iter().enumerate() {
            push_bits(&mut out, &format!("case[{i}].cdi_wing[{j}]"), v);
        }
        for (j, &v) in case.cl_y.iter().enumerate() {
            push_bits(&mut out, &format!("case[{i}].cl_y[{j}]"), v);
        }
        for (j, &v) in case.cdi_y.iter().enumerate() {
            push_bits(&mut out, &format!("case[{i}].cdi_y[{j}]"), v);
        }
        for (j, &v) in case.cp.iter().enumerate() {
            push_bits32(&mut out, &format!("case[{i}].cp[{j}]"), v);
        }
        for (j, &v) in case.gamma.iter().enumerate() {
            push_bits32(&mut out, &format!("case[{i}].gamma[{j}]"), v);
        }
    }
    check_baseline_host_tolerant("vorlax_training_grid.txt", &out);
}

/// A minimal airplane: one main wing, one tail surface, a fuselage and a
/// nacelle, big enough that the thickness/sweep buildup visits both the
/// area-weighted (index 0) and root-section (index > 0) branches.
fn probe_airplane() -> Airplane {
    let main_airfoil = naca("naca2412");
    let main_wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 4.0, 2.0, main_airfoil.clone()),
            WingXSec::new([1.5, 10.0, 0.3], 2.0, -1.0, main_airfoil.clone()),
            WingXSec::new([2.2, 16.0, 0.6], 1.2, -2.0, main_airfoil),
        ],
        true,
    );
    let tail_airfoil = naca("naca0012");
    let tail = Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([16.0, 0.0, 1.0], 2.0, 0.0, tail_airfoil.clone()),
            WingXSec::new([17.0, 5.0, 1.2], 1.0, 0.0, tail_airfoil),
        ],
        true,
    );
    let station = |x: f64, radius: f64| {
        FuselageXSec::new([x, 0.0, 0.0], Some(radius), None, None, 2.0)
            .expect("a radius with no width or height")
    };
    let body = Fuselage::new(
        "Fuselage",
        vec![station(0.0, 0.5), station(10.0, 2.0), station(30.0, 0.3)],
    );
    let nacelle_station = |x: f64, y: f64, radius: f64| {
        FuselageXSec::new([x, y, 0.0], Some(radius), None, None, 1.5)
            .expect("a radius with no width or height")
    };
    let nacelle = Fuselage::new(
        "Nacelle",
        vec![
            nacelle_station(4.0, 6.0, 1.0),
            nacelle_station(8.0, 6.0, 1.0),
        ],
    );
    Airplane {
        name: "perf-aero probe".to_owned(),
        xyz_ref: [10.0, 0.0, 0.5],
        wings: vec![main_wing, tail],
        fuselages: vec![body, nacelle],
        s_ref: 120.0,
        c_ref: 4.0,
        b_ref: 32.0,
    }
}

/// Task 2: the geometry-only thickness caching `drag_components` reads.
#[test]
fn drag_components_sweep_is_bit_identical() {
    let plane = probe_airplane();
    let analysis = AeroAnalysis::new(&plane, 30.0, None, None, None);
    let mut out = String::new();
    for &mach in &[0.3, 0.6, 0.78, 0.82, 0.87, 0.6] {
        for &cl in &[0.2, 0.45, 0.7, 1.1] {
            let components = analysis.drag_components(mach, 10_000.0, cl, 0.012, None);
            let label = format!("mach={mach} cl={cl}");
            push_bits(
                &mut out,
                &format!("{label} cd_parasite"),
                components.cd_parasite,
            );
            push_bits(
                &mut out,
                &format!("{label} cd_induced"),
                components.cd_induced,
            );
            push_bits(&mut out, &format!("{label} cd_wave"), components.cd_wave);
            push_bits(
                &mut out,
                &format!("{label} cd_total"),
                components.cd_total(),
            );
        }
    }
    // The reference-compatibility path takes the frozen single-thickness
    // convention through the same cache and must stay identical too.
    let reference = AeroAnalysis::new_reference_compatibility(&plane, 30.0, None, None, None);
    for &mach in &[0.3, 0.82] {
        let components = reference.drag_components(mach, 10_000.0, 0.5, 0.012, None);
        push_bits(
            &mut out,
            &format!("reference mach={mach} cd_parasite"),
            components.cd_parasite,
        );
        push_bits(
            &mut out,
            &format!("reference mach={mach} cd_wave"),
            components.cd_wave,
        );
    }
    check_baseline("drag_components_sweep.txt", &out);
}

/// Task 3: the VLM `velocity_at_points` kernel cache, at a panel count above
/// the 128-panel rayon threshold.
#[test]
fn vlm_solve_over_threshold_is_bit_identical() {
    let plane = probe_airplane();
    let system = VlmSystem::assemble(&plane, 12, 6).expect("well-posed mesh");
    assert!(
        system.panel_count() > 128,
        "this case exists to exercise the parallel path: {} panels",
        system.panel_count()
    );
    let mut out = String::new();
    for &alpha in &[-3.0, 0.0, 2.5, 6.0, 9.0] {
        let op_point = OperatingPoint::new(
            Atmosphere::new(9000.0),
            220.0,
            alpha,
            1.0,
            0.02,
            -0.01,
            0.005,
        );
        let solved = system.solve(&op_point).expect("well-posed solve");
        let label = format!("alpha={alpha}");
        for (name, v) in [
            ("lift", solved.lift),
            ("drag", solved.drag),
            ("side_force", solved.side_force),
            ("roll_moment", solved.roll_moment),
            ("pitch_moment", solved.pitch_moment),
            ("yaw_moment", solved.yaw_moment),
            ("cl_lift", solved.cl_lift),
            ("cd_drag", solved.cd_drag),
            ("cy_side", solved.cy_side),
            ("cl_roll", solved.cl_roll),
            ("cm_pitch", solved.cm_pitch),
            ("cn_yaw", solved.cn_yaw),
        ] {
            push_bits(&mut out, &format!("{label} {name}"), v);
        }
        for (j, &v) in solved.vortex_strengths.iter().enumerate() {
            push_bits(&mut out, &format!("{label} vortex_strengths[{j}]"), v);
        }
        for (j, force) in solved.panel_forces_geometry.iter().enumerate() {
            for (k, &component) in force.iter().enumerate() {
                push_bits(
                    &mut out,
                    &format!("{label} panel_forces_geometry[{j}][{k}]"),
                    component,
                );
            }
        }
    }
    check_baseline("vlm_solve_over_threshold.txt", &out);
}

/// The same solve, below the 128-panel threshold and below the vorlax and
/// VLM kernel-cache size caps, so the serial/uncached fallback paths stay
/// covered too.
#[test]
fn vlm_solve_under_threshold_is_bit_identical() {
    let airfoil = naca("naca0012");
    let wing = Wing::new(
        "Flat",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, airfoil.clone()),
            WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, airfoil),
        ],
        true,
    );
    let plane = Airplane {
        name: "small probe".to_owned(),
        xyz_ref: [0.25, 0.0, 0.0],
        s_ref: wing.reference_area(),
        c_ref: wing.mean_aerodynamic_chord(),
        b_ref: wing.reference_span(),
        wings: vec![wing],
        fuselages: Vec::new(),
    };
    let mut out = String::new();
    for &alpha in &[-2.0, 0.0, 4.0] {
        let op_point = OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha, 0.0, 0.0, 0.0, 0.0);
        let solved = vlm::run(&plane, &op_point, 1, 4).expect("well-posed solve");
        let label = format!("alpha={alpha}");
        push_bits(&mut out, &format!("{label} lift"), solved.lift);
        push_bits(&mut out, &format!("{label} cl_lift"), solved.cl_lift);
        push_bits(&mut out, &format!("{label} cd_drag"), solved.cd_drag);
        for (j, &v) in solved.vortex_strengths.iter().enumerate() {
            push_bits(&mut out, &format!("{label} vortex_strengths[{j}]"), v);
        }
    }
    check_baseline("vlm_solve_under_threshold.txt", &out);
}
