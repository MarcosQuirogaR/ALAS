// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::analytical` against
//! `alas.physics.structural_analysis`, via
//! `golden/generators/gen_struct_analytical.py`.
//!
//! The solve is closed-form arithmetic over the (already `green`) sized
//! wingbox and load model: I-section stiffness, virtual-work deflection,
//! cap stress, and a Rayleigh-quotient modal estimate, so every continuous
//! quantity is checked at `Tier::Closed`, matching `docs/PORTING.md`. A `+inf`
//! margin of safety is recorded as JSON `null` and mapped back to an infinity
//! check. The `WingStructureGeometry` and configs are rebuilt exactly as
//! `parity_sizing.rs` does, and `size_wingbox` (itself `green`) is recomputed
//! to feed `analyze_structure`.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::materials;
use alas_config::{
    DesignRequirements, DesignVector, GeometryConfig, MassModelConfig, StructuresConfig, WingConfig,
};
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::analytical::{
    analyze_structure_reference_compatibility, LoadCaseResult, StructuralAnalysisReport,
};
use alas_struct::sizing::size_wingbox_reference_compatibility;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct SparStressRecord {
    chord_fraction: f64,
    stress_pa: Vec<f64>,
    margin_of_safety: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct LoadCaseRecord {
    name: String,
    load_factor: f64,
    y: Vec<f64>,
    q_net: Vec<f64>,
    shear_n: Vec<f64>,
    moment_nm: Vec<f64>,
    deflection_m: Vec<f64>,
    tip_deflection_m: f64,
    spar_stress: Vec<SparStressRecord>,
}

#[derive(Debug, Deserialize)]
struct ModalRecord {
    frequencies_hz: Vec<f64>,
    mode_shapes: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct ReportRecord {
    y: Vec<f64>,
    ei_nm2: Vec<f64>,
    load_cases: Vec<LoadCaseRecord>,
    modal: ModalRecord,
}

#[derive(Debug, Deserialize)]
struct MaterialsRecord {
    skin: String,
    web: String,
    cap: String,
    rib: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    config: Map<String, Value>,
    spar_chord_fractions: Vec<f64>,
    spar_full_span: Vec<bool>,
    materials: MaterialsRecord,
    report: ReportRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn structures_config_for(overrides: &Map<String, Value>) -> StructuresConfig {
    let mut cfg = StructuresConfig::default();
    for (key, value) in overrides {
        match key.as_str() {
            "center_spar_enabled" => cfg.center_spar_enabled = value.as_bool().unwrap(),
            "num_ribs_override" => cfg.num_ribs_override = Some(value.as_i64().unwrap()),
            other => panic!("fixture set an unhandled StructuresConfig field: {other}"),
        }
    }
    cfg
}

fn build_geometry(case: &Case) -> WingStructureGeometry {
    let dv = DesignVector::default();
    let wing_cfg = WingConfig::default();
    let root_base =
        AirfoilLibrary::get(&wing_cfg.root_airfoil).expect("the configured root airfoil resolves");
    let root_section =
        build_section(&dv, &root_base.coordinates).expect("the root section repanels cleanly");
    let tip_airfoil =
        AirfoilLibrary::get(&wing_cfg.tip_airfoil).expect("the configured tip airfoil resolves");
    WingStructureGeometry::new(
        &dv,
        &wing_cfg,
        &root_section,
        &tip_airfoil,
        &case.spar_chord_fractions,
        Some(&case.spar_full_span),
    )
    .expect("the fixture's spar list is valid")
}

fn compare_vec(comparison: &mut Comparison, label: &str, actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{label}: length");
    for (i, (&a, &e)) in actual.iter().zip(expected).enumerate() {
        comparison.scalar(&format!("{label}[{i}]"), a, e);
    }
}

fn compare_margin(
    comparison: &mut Comparison,
    label: &str,
    actual: &[f64],
    expected: &[Option<f64>],
) {
    assert_eq!(actual.len(), expected.len(), "{label}: length");
    for (i, (&a, e)) in actual.iter().zip(expected).enumerate() {
        match e {
            None => assert!(
                a.is_infinite() && a > 0.0,
                "{label}[{i}]: expected +inf, got {a}"
            ),
            Some(value) => {
                comparison.scalar(&format!("{label}[{i}]"), a, *value);
            }
        }
    }
}

fn compare_load_case(comparison: &mut Comparison, got: &LoadCaseResult, want: &LoadCaseRecord) {
    let name = &want.name;
    assert_eq!(got.name, want.name, "load case name");
    assert_eq!(
        got.spar_stress.len(),
        want.spar_stress.len(),
        "{name}: spar count"
    );
    comparison.scalar(
        &format!("{name}: load_factor"),
        got.load_factor,
        want.load_factor,
    );
    comparison.scalar(
        &format!("{name}: tip_deflection_m"),
        got.tip_deflection_m,
        want.tip_deflection_m,
    );
    compare_vec(comparison, &format!("{name}: y"), &got.y, &want.y);
    compare_vec(
        comparison,
        &format!("{name}: q_net"),
        &got.q_net,
        &want.q_net,
    );
    compare_vec(
        comparison,
        &format!("{name}: shear_n"),
        &got.shear_n,
        &want.shear_n,
    );
    compare_vec(
        comparison,
        &format!("{name}: moment_nm"),
        &got.moment_nm,
        &want.moment_nm,
    );
    compare_vec(
        comparison,
        &format!("{name}: deflection_m"),
        &got.deflection_m,
        &want.deflection_m,
    );
    for (i, (gs, ws)) in got.spar_stress.iter().zip(&want.spar_stress).enumerate() {
        let sp = format!("{name}: spar_stress[{i}]");
        comparison.scalar(
            &format!("{sp}.chord_fraction"),
            gs.chord_fraction,
            ws.chord_fraction,
        );
        compare_vec(
            comparison,
            &format!("{sp}.stress_pa"),
            &gs.stress_pa,
            &ws.stress_pa,
        );
        compare_margin(
            comparison,
            &format!("{sp}.margin_of_safety"),
            &gs.margin_of_safety,
            &ws.margin_of_safety,
        );
    }
}

fn compare(
    comparison: &mut Comparison,
    name: &str,
    got: &StructuralAnalysisReport,
    want: &ReportRecord,
) {
    compare_vec(comparison, &format!("{name}: y"), &got.y, &want.y);
    compare_vec(
        comparison,
        &format!("{name}: ei_nm2"),
        &got.ei_nm2,
        &want.ei_nm2,
    );

    assert_eq!(
        got.load_cases.len(),
        want.load_cases.len(),
        "{name}: load-case count"
    );
    for (gc, wc) in got.load_cases.iter().zip(&want.load_cases) {
        compare_load_case(comparison, gc, wc);
    }

    compare_vec(
        comparison,
        &format!("{name}: modal.frequencies_hz"),
        &got.modal.frequencies_hz,
        &want.modal.frequencies_hz,
    );
    assert_eq!(
        got.modal.mode_shapes.len(),
        want.modal.mode_shapes.len(),
        "{name}: mode-shape count"
    );
    for (i, (gs, ws)) in got
        .modal
        .mode_shapes
        .iter()
        .zip(&want.modal.mode_shapes)
        .enumerate()
    {
        compare_vec(
            comparison,
            &format!("{name}: modal.mode_shape[{i}]"),
            gs,
            ws,
        );
    }
}

#[test]
fn analyze_structure_matches_python_across_structures_config_cases() {
    let fixture: Fixture = alas_testkit::load("struct", "analytical");
    // The frozen Python fixture was generated with the historical 9.81 m/s^2
    // default. Preserve that input for this reference-compatibility replay;
    // production requirements use standard gravity (9.80665 m/s^2).
    let req = DesignRequirements {
        gravity_m_s2: 9.81,
        ..DesignRequirements::default()
    };
    let engine_cfg = GeometryConfig::default().engine;
    let mass_cfg = MassModelConfig::default();

    let mut comparison =
        Comparison::new("alas-struct::analytical::analyze_structure", Tier::Closed);
    for case in &fixture.cases {
        let wsg = build_geometry(case);
        let cfg = structures_config_for(&case.config);
        let skin_mat = materials::get(&case.materials.skin).expect("skin material resolves");
        let web_mat = materials::get(&case.materials.web).expect("web material resolves");
        let cap_mat = materials::get(&case.materials.cap).expect("cap material resolves");
        let rib_mat = materials::get(&case.materials.rib).expect("rib material resolves");

        let sizing = size_wingbox_reference_compatibility(
            &wsg, &cfg, &req, skin_mat, web_mat, cap_mat, rib_mat,
        );
        let report = analyze_structure_reference_compatibility(
            &wsg,
            &sizing,
            &cfg,
            &req,
            &engine_cfg,
            &mass_cfg,
            skin_mat,
            web_mat,
            cap_mat,
        );
        compare(&mut comparison, &case.name, &report, &case.report);
    }
    comparison.finish();
}
