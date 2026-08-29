// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W3.8 data-contract checks for structural and external-result scenes.
//!
//! These assertions inspect scene semantics: axis labels, unavailable-state
//! text, and whether optional external series are absent. A non-empty SVG is
//! not evidence that a structural figure is scientifically populated.

// The fixture assertions intentionally fail at the test boundary when the
// checked-in contract is malformed; production code contains no such calls.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use alas_pipeline::structural::StructuralAnalysisResult;
use alas_report::families::structures;
use alas_report::scene::SceneElement;
use alas_struct::analytical::{LoadCaseResult, ModalResult, StructuralAnalysisReport};
use alas_struct::sizing::{MassBreakdown, SparSizing, WingboxSizing};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w38.json"
    ))
    .expect("W3.8 contract fixture is valid JSON")
}

fn text_nodes(scene: &alas_report::scene::Scene) -> Vec<&str> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn has_text(scene: &alas_report::scene::Scene, needle: &str) -> bool {
    text_nodes(scene)
        .into_iter()
        .any(|text| text.contains(needle))
}

fn sample_structural_result() -> StructuralAnalysisResult {
    let y = vec![0.0, 1.0, 2.0];
    let spar = SparSizing {
        chord_fraction: 0.2,
        h: vec![0.5, 0.4, 0.3],
        a_cap: vec![0.01, 0.008, 0.005],
        frac_moment: vec![1.0, 1.0, 1.0],
        t_cap: vec![0.01, 0.008, 0.005],
        w_cap: vec![0.2, 0.18, 0.15],
        t_web: 0.005,
        margin_of_safety: vec![0.0, 1.0, 2.0],
    };
    let sizing = WingboxSizing {
        y_stations: y.clone(),
        eta_stations: vec![0.0, 0.5, 1.0],
        chord: vec![6.0, 4.0, 2.0],
        spar_fracs: vec![0.2],
        spars: vec![spar.clone()],
        t_skin: 0.004,
        num_ribs: 10,
        rib_spacing_m: 1.8,
        mass_breakdown_kg: MassBreakdown {
            spar_caps: 100.0,
            spar_webs: 50.0,
            skin: 200.0,
            ribs: 60.0,
        },
        total_mass_kg: 410.0,
        sizing_load_case: "pull-up",
    };
    let load_case = LoadCaseResult {
        name: "pull-up",
        load_factor: 2.5,
        y: y.clone(),
        q_net: vec![0.0; 3],
        shear_n: vec![0.0; 3],
        moment_nm: vec![0.0; 3],
        deflection_m: vec![0.0; 3],
        tip_deflection_m: 0.0,
        spar_stress: vec![alas_struct::analytical::SparStressResult {
            chord_fraction: 0.2,
            stress_pa: vec![1.0; 3],
            margin_of_safety: vec![0.0, 1.0, 2.0],
        }],
    };
    StructuralAnalysisResult {
        status: "ok".to_owned(),
        error: None,
        wsg: None,
        sizing: Some(sizing),
        mesh_health: None,
        analysis: Some(StructuralAnalysisReport {
            y,
            ei_nm2: vec![1.0; 3],
            load_cases: vec![load_case],
            modal: ModalResult {
                frequencies_hz: vec![2.0],
                mode_shapes: vec![vec![0.0, 0.5, 1.0]],
            },
        }),
        nastran95: None,
        nastran: None,
        patran: None,
        torenbeek_wing_mass_kg: 400.0,
    }
}

#[test]
fn w38_fixture_names_every_required_axis_and_external_contract() {
    let fixture = fixture();
    let sizing = &fixture["figures"]["structures_sizing"]["axis_labels"];
    assert_eq!(sizing[0], "Spanwise position Y [m]");
    assert_eq!(sizing[1], "Chordwise position X [m]");
    assert_eq!(fixture["figures"]["structures_sizing"]["bars"], 2);
    assert_eq!(
        fixture["figures"]["structures_patran"]["external_result"],
        true
    );
    assert_eq!(
        fixture["figures"]["structures_patran"]["ordered_images"],
        true
    );
}

#[test]
fn unavailable_structural_scene_uses_the_actionable_reference_reason() {
    let scene = structures::figure_structures_sizing(None, Some("light"));
    let contract = fixture();
    let expected = contract["unavailable_reasons"]["structural"]
        .as_str()
        .expect("structural reason is a string");
    assert!(has_text(&scene, expected));
}

#[test]
fn modes_do_not_fabricate_a_nastran_series_when_the_pipeline_has_no_solver_result() {
    let scene = structures::figure_structures_modes(Some(&sample_structural_result()), None);
    let contract = fixture();
    let expected = contract["unavailable_reasons"]["modes_nastran"]
        .as_str()
        .expect("mode reason is a string");
    assert!(has_text(&scene, expected));
    assert!(!has_text(
        &scene,
        "NASTRAN SOL 103 (nearest-frequency match)"
    ));
}

#[test]
fn vibration_and_patran_report_missing_external_results_instead_of_substituting_data() {
    let result = sample_structural_result();
    let vibration = structures::figure_structures_vibration(Some(&result), None);
    let patran = structures::figure_structures_patran(Some(&result), None);
    let contract = fixture();
    let reasons = contract["unavailable_reasons"]
        .as_object()
        .expect("unavailable reasons are an object");
    assert!(has_text(
        &vibration,
        reasons["vibration"]
            .as_str()
            .expect("vibration reason is a string")
    ));
    assert!(has_text(
        &patran,
        reasons["patran"]
            .as_str()
            .expect("Patran reason is a string")
    ));
    assert!(!vibration
        .elements
        .iter()
        .any(|element| matches!(element, SceneElement::Polyline { .. })));
    assert!(!patran
        .elements
        .iter()
        .any(|element| matches!(element, SceneElement::Image { .. })));
}

#[test]
fn stress_scene_exposes_units_and_numeric_ticks_on_the_real_analysis() {
    let scene = structures::figure_structures_stress(Some(&sample_structural_result()), None);
    assert!(has_text(&scene, "Spanwise position Y [m]"));
    assert!(has_text(&scene, "Margin of safety [-]"));
    assert!(scene
        .elements
        .iter()
        .any(|element| { matches!(element, SceneElement::Line { .. }) }));
}
