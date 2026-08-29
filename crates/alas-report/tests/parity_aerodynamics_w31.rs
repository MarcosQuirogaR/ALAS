// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W3.1 figure contracts: Python's saved reference metadata remains the
//! source of truth for labels, panel titles, and availability semantics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::analysis::PolarSweep;
use alas_aero::avl::{AvlModel, AvlPolar, AvlPolarPoint, AvlReference};
use alas_aero::mses::{MsesPolarResult, MsesPressureResult, MsesStatus};
use alas_aero::vspaero::{VspaeroModel, VspaeroPolar, VspaeroPolarPoint, VspaeroReference};
use alas_config::design_variables::DesignVector;
use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
use alas_pipeline::{
    AvlAnalysisResult, AvlAnalysisStatus, AvlComparableQuantity, AvlComparisonReference,
    AvlComparisonStatus, VspaeroAnalysisResult, VspaeroAnalysisStatus, VspaeroComparableQuantity,
    VspaeroComparisonStatus,
};
use alas_report::families::{aerodynamics, optimization};
use alas_report::svg::render_svg;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn report() -> AnalysisReport {
    AnalysisReport {
        design: DesignVector::default(),
        airplane: Airplane {
            name: "W3.1 contract aircraft".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 20.0,
            c_ref: 2.0,
            b_ref: 10.0,
        },
        polar: PolarSweep {
            alpha_deg: vec![-4.0, 0.0, 4.0, 8.0],
            geometric_alpha_deg: vec![-4.0, 0.0, 4.0, 8.0],
            cl: vec![-0.2, 0.2, 0.6, 0.9],
            cd: vec![0.04, 0.025, 0.03, 0.05],
            cd_induced: vec![0.01, 0.012, 0.02, 0.035],
            cd_wave: vec![0.0; 4],
            cd_parasite: vec![0.03, 0.013, 0.01, 0.015],
            cm: vec![0.04, 0.02, -0.01, -0.04],
            l_over_d: vec![-5.0, 8.0, 20.0, 18.0],
        },
        design_point: DesignPoint {
            alpha_deg: 4.0,
            cl: 0.6,
            cd: 0.03,
            l_over_d: 20.0,
        },
        polar_fit: PolarFit {
            cd0: 0.013,
            k: 0.05,
            oswald_e: 0.85,
            aspect_ratio: 5.0,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.1,
        x_neutral_point: 4.0,
        trimmed_design_point: None,
        component_masses: HashMap::new(),
        mass_coordinates: HashMap::new(),
        physical_cg: [0.0, 0.0, 0.0],
        geometry_summary: HashMap::new(),
        payload_layout: None,
        cg_envelope_ok: Some(true),
    }
}

fn avl_result(
    report: &AnalysisReport,
    status: AvlAnalysisStatus,
    comparison: AvlComparisonStatus,
) -> AvlAnalysisResult {
    let points = report
        .polar
        .alpha_deg
        .iter()
        .map(|&alpha_deg| AvlPolarPoint {
            alpha_deg,
            beta_deg: 0.0,
            mach: 0.3,
            lift_coefficient: 0.1 * alpha_deg,
            total_drag_coefficient: 99.0,
            induced_drag_coefficient: 0.01,
            pitching_moment_coefficient: -0.02 * alpha_deg,
            span_efficiency: Some(0.9),
        })
        .collect();
    let path = PathBuf::from("case.avl");
    AvlAnalysisResult {
        status,
        runtime_executable: Some(PathBuf::from("avl.exe")),
        geometry_path: path.clone(),
        session_path: path.with_extension("session.txt"),
        force_paths: vec![path.with_extension("000.ft")],
        stdout_path: path.with_extension("stdout.txt"),
        stderr_path: path.with_extension("stderr.txt"),
        polar: Some(AvlPolar {
            reference: AvlReference {
                area_m2: report.airplane.s_ref,
                chord_m: report.airplane.c_ref,
                span_m: report.airplane.b_ref,
                moment_reference_m: report.airplane.xyz_ref,
            },
            model: AvlModel::ALAS_LIFTING_SURFACES,
            points,
        }),
        comparison_reference: Some(AvlComparisonReference {
            phase: "takeoff climb midpoint".to_owned(),
            mach: 0.3,
            altitude_m: 1_500.0,
            vlm_polar: report.polar.clone(),
            geometric_alpha_deg: report.polar.alpha_deg.clone(),
        }),
        comparison,
        error: None,
    }
}

#[test]
fn model_comparison_overlays_only_compatible_avl_lift_and_moment() {
    let report = report();
    let points = report
        .polar
        .alpha_deg
        .iter()
        .map(|&alpha_deg| AvlPolarPoint {
            alpha_deg,
            beta_deg: 0.0,
            mach: 0.3,
            lift_coefficient: 0.1 * alpha_deg,
            total_drag_coefficient: 99.0,
            induced_drag_coefficient: 0.01,
            pitching_moment_coefficient: -0.02 * alpha_deg,
            span_efficiency: Some(0.9),
        })
        .collect();
    let path = PathBuf::from("case.avl");
    let avl = AvlAnalysisResult {
        status: AvlAnalysisStatus::CompletedComparable,
        runtime_executable: Some(PathBuf::from("avl.exe")),
        geometry_path: path.clone(),
        session_path: path.with_extension("session.txt"),
        force_paths: vec![path.with_extension("000.ft")],
        stdout_path: path.with_extension("stdout.txt"),
        stderr_path: path.with_extension("stderr.txt"),
        polar: Some(AvlPolar {
            reference: AvlReference {
                area_m2: report.airplane.s_ref,
                chord_m: report.airplane.c_ref,
                span_m: report.airplane.b_ref,
                moment_reference_m: report.airplane.xyz_ref,
            },
            model: AvlModel::ALAS_LIFTING_SURFACES,
            points,
        }),
        comparison_reference: Some(AvlComparisonReference {
            phase: "takeoff climb midpoint".to_owned(),
            mach: 0.3,
            altitude_m: 1_500.0,
            vlm_polar: report.polar.clone(),
            geometric_alpha_deg: report.polar.alpha_deg.clone(),
        }),
        comparison: AvlComparisonStatus::Compatible(vec![
            AvlComparableQuantity::LiftCoefficient,
            AvlComparableQuantity::PitchingMomentCoefficient,
        ]),
        error: None,
    };
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        Some(&avl),
        Some("dark"),
    ));

    text_contract(
        &svg,
        &[
            "Athena AVL (cross-check)",
            "Lift cross-check -- takeoff climb midpoint",
            "Pitching-moment cross-check",
            "Induced drag cross-check",
            "Span efficiency cross-check",
        ],
    );
    assert!(!visible_text(&svg).contains("99.0"));
    assert!(!svg.contains("VLM vs AVL delta summary"));
    assert!(!svg.contains("Trefftz drag is never overlaid on total CD"));
    assert!(!svg.contains("<circle"));
}

#[test]
fn model_comparison_keeps_the_legacy_layout_when_avl_is_absent() {
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report(),
        None,
        None,
        None,
        None,
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS VLM"]);
    assert!(!svg.contains("Induced drag cross-check"));
    assert!(!svg.contains("Athena AVL"));
    assert!(!svg.contains("MSES 2-D section"));
}

#[test]
fn model_comparison_uses_one_bottom_legend_and_unmarked_model_curves() {
    let scene =
        aerodynamics::figure_model_comparison(&report(), None, None, None, None, Some("dark"));
    let legend_positions = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Text { text, pos, .. } if text == "ALAS VLM" => {
                Some(pos[1])
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(legend_positions.len(), 1);
    assert!(legend_positions[0] >= 650.0);
    assert!(scene
        .elements
        .iter()
        .all(|element| !matches!(element, alas_report::scene::SceneElement::Circle { .. })));
    assert!(scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Polyline { stroke, .. } => Some(stroke),
            _ => None,
        })
        .all(|stroke| {
            stroke.color.to_hex_rgb() == "#0072b2" && (stroke.width - 1.8).abs() < f64::EPSILON
        }));
}

#[test]
fn model_comparison_reports_avl_status_without_overlaying_non_comparable_data() {
    let report = report();
    let mut avl = avl_result(
        &report,
        AvlAnalysisStatus::CompletedNotComparable,
        AvlComparisonStatus::Rejected("reference mismatch".to_owned()),
    );
    avl.error = Some("reference mismatch".to_owned());
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        Some(&avl),
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS VLM"]);
    assert!(!svg.contains("Induced drag cross-check"));
    assert!(!visible_text(&svg).contains("Athena AVL"));
    assert!(!svg.contains("reference mismatch"));
}

#[test]
fn model_comparison_names_the_exported_avl_deck_when_the_solver_is_not_configured() {
    let report = report();
    let path = PathBuf::from("outputs/avl/optimized_aircraft.avl");
    let avl = AvlAnalysisResult {
        status: AvlAnalysisStatus::NotConfigured,
        runtime_executable: None,
        geometry_path: path.clone(),
        session_path: path.with_extension("session.txt"),
        force_paths: Vec::new(),
        stdout_path: path.with_extension("stdout.txt"),
        stderr_path: path.with_extension("stderr.txt"),
        polar: None,
        comparison_reference: None,
        comparison: AvlComparisonStatus::NotEvaluated,
        error: Some("native AVL executable is not configured; geometry deck exported".to_owned()),
    };

    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        Some(&avl),
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS VLM"]);
    assert!(!svg.contains("optimized_aircraft.avl"));
}

fn text_contract(svg: &str, words: &[&str]) {
    let visible_text = visible_text(svg);
    for word in words {
        assert!(
            svg.contains(word) || visible_text.contains(word),
            "SVG contract is missing {word:?}"
        );
    }
}

fn visible_text(svg: &str) -> String {
    svg.split("<tspan")
        .skip(1)
        .filter_map(|fragment| fragment.split_once('>'))
        .filter_map(|(_, fragment)| fragment.split_once("</tspan>"))
        .map(|(text, _)| text.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn python_reference_contract_names_every_w31_family_and_theme() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_corpus.json"
    ))
    .expect("reference render corpus is valid JSON");
    let figures = corpus["figures"].as_object().expect("figure map");
    for id in [
        "aero_panel",
        "airfoil_comparison",
        "airfoil_reynolds",
        "drag_breakdown",
        "polar_comparison",
        "model_comparison",
        "span_loading",
        "vlm_flow",
        "mses_pressure",
        "mses_mach_contours",
    ] {
        for theme in ["light", "dark"] {
            assert!(
                figures.contains_key(&format!("{id}:{theme}")),
                "missing {id}:{theme}"
            );
        }
    }
}

#[test]
fn report_fed_w31_figures_preserve_python_titles_axes_markers_and_legends() {
    let report = report();
    let svg = render_svg(&aerodynamics::figure_aero_panel(&report, Some("light")));
    text_contract(
        &svg,
        &["Aerodynamic Polar Set", "\u{03b1} [deg]", "Design CL"],
    );
    assert!(svg.matches("<circle").count() >= 4);

    let svg = render_svg(&aerodynamics::figure_polar_comparison(
        &report,
        &report,
        ("baseline", "optimized"),
        None,
        Some("dark"),
    ));
    text_contract(
        &svg,
        &[
            "Drag Polar Comparison",
            "CD",
            "L/D",
            "Baseline",
            "Optimized",
        ],
    );
    assert!(!svg.contains("SM="));
    assert!(!svg.contains("CG env."));

    let svg = render_svg(&aerodynamics::figure_drag_breakdown(&report, Some("light")));
    text_contract(
        &svg,
        &["Drag Breakdown at Design CL", "CDwave", "Design CL"],
    );

    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        None,
        Some("dark"),
    ));
    text_contract(
        &svg,
        &[
            "Whole-Aircraft Model Comparison",
            "ALAS VLM",
            "Pitching moment",
            "L/D",
        ],
    );
    assert!(!svg.contains("MSES 2-D section"));
    assert!(!svg.contains("Athena AVL"));
    assert!(!svg.contains("<circle"));

    let airfoil = Airfoil::from_name("naca2412").expect("fixture airfoil");
    let svg = render_svg(&optimization::figure_airfoil_comparison(
        &[(0.0, 0.0), (0.5, 0.1), (1.0, 0.0)],
        &airfoil.coordinates,
        Some("light"),
    ));
    text_contract(
        &svg,
        &[
            "Airfoil Section Shape Comparison",
            "x/c",
            "y/c",
            "Initial",
            "Optimized",
        ],
    );
}

#[test]
fn model_comparison_keeps_section_coefficients_out_of_aircraft_axes() {
    let section = MsesPolarResult {
        status: MsesStatus::Ok,
        airfoil_name: "root_demo".to_owned(),
        mach: 0.72,
        reynolds: 12_000_000.0,
        alpha_deg: vec![0.0, 4.0],
        cl: vec![7.0, 9.0],
        cd: vec![3.0, 4.0],
        cm: vec![2.0, 2.5],
        ..MsesPolarResult::default()
    };
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report(),
        None,
        Some(&section),
        None,
        None,
        Some("dark"),
    ));

    text_contract(&svg, &["Whole-Aircraft Model Comparison"]);
    assert!(!svg.contains("root_demo"));
    assert!(!svg.contains("MSES"));
}

#[test]
fn model_comparison_plots_the_geometry_resolving_non_vlm_lift_model() {
    let mut report = report();
    let airfoil = Airfoil::from_name("naca2412").expect("analytical airfoil");
    report.airplane.wings = vec![Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 2.0, 1.0, airfoil.clone()),
            WingXSec::new([1.0, 5.0, 0.0], 1.0, -2.0, airfoil),
        ],
        true,
    )];
    report.airplane.s_ref = 15.0;
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        None,
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS local Fourier lifting-line"]);
    assert!(!svg.contains("in-process Rust"));
}

#[test]
fn classical_prandtl_curve_uses_geometry_when_the_report_grid_never_reaches_zero_lift() {
    let mut report = report();
    report.polar.alpha_deg = vec![3.0, 5.0, 7.0];
    report.polar.cl = vec![0.4, 0.6, 0.8];
    report.polar.cd = vec![0.025, 0.03, 0.04];
    report.polar.cd_induced = vec![0.01, 0.015, 0.025];
    report.polar.cd_wave = vec![0.0; 3];
    report.polar.cd_parasite = vec![0.015; 3];
    report.polar.cm = vec![0.02, 0.01, -0.01];
    report.polar.l_over_d = vec![16.0, 20.0, 20.0];
    let airfoil = Airfoil::from_name("naca2412").expect("analytical airfoil");
    report.airplane.wings = vec![Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 2.0, 1.0, airfoil.clone()),
            WingXSec::new([1.0, 5.0, 0.0], 1.0, -2.0, airfoil),
        ],
        true,
    )];
    report.airplane.s_ref = 15.0;

    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        None,
        None,
        Some("dark"),
    ));

    text_contract(&svg, &["Prandtl lifting-line", "Helmbold lift slope"]);
    assert!(!svg.contains("alpha_L=0"));
    assert!(!svg.contains("does not bracket CL=0"));
}

#[test]
fn model_comparison_overlays_only_compatible_vspaero_lift_and_moment() {
    let report = report();
    let points = report
        .polar
        .alpha_deg
        .iter()
        .map(|&alpha_deg| VspaeroPolarPoint {
            beta_deg: 0.0,
            mach: 0.84,
            alpha_deg,
            reynolds: 1.0e7,
            lift_coefficient: 0.1 * alpha_deg,
            induced_drag_coefficient: 0.01,
            total_drag_coefficient: 99.0,
            side_force_coefficient: 0.0,
            lift_to_drag: 0.0,
            span_efficiency: Some(0.9),
            rolling_moment_coefficient: 0.0,
            pitching_moment_coefficient: -0.02 * alpha_deg,
            yawing_moment_coefficient: 0.0,
        })
        .collect();
    let path = PathBuf::from("case");
    let vspaero = VspaeroAnalysisResult {
        status: VspaeroAnalysisStatus::CompletedComparable,
        runtime_executable: Some(PathBuf::from("vspaero.exe")),
        case_path: path.clone(),
        geometry_path: path.with_extension("vspgeom"),
        setup_path: path.with_extension("vspaero"),
        polar_path: path.with_extension("polar"),
        stdout_path: path.with_extension("stdout.txt"),
        stderr_path: path.with_extension("stderr.txt"),
        polar: Some(VspaeroPolar {
            reference: VspaeroReference {
                area_m2: report.airplane.s_ref,
                chord_m: report.airplane.c_ref,
                span_m: report.airplane.b_ref,
                moment_reference_m: report.airplane.xyz_ref,
            },
            model: VspaeroModel::ALAS_VLM,
            points,
        }),
        comparison: VspaeroComparisonStatus::Compatible(vec![
            VspaeroComparableQuantity::LiftCoefficient,
            VspaeroComparableQuantity::PitchingMomentCoefficient,
        ]),
        error: None,
    };
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        Some(&vspaero),
        None,
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS VLM", "VSPAERO VLM"]);
    assert!(!visible_text(&svg).contains("99.0"));
    assert!(!svg.contains("CL and Cm overlaid after reference/frame checks"));
}

#[test]
fn w31_optional_figures_are_data_driven_and_honest_without_external_data() {
    let report = report();
    let svg = render_svg(&aerodynamics::figure_span_loading(&report, Some("light")));
    text_contract(&svg, &["VLM span loading unavailable"]);
    let svg = render_svg(&aerodynamics::figure_vlm_flow(&report, Some("dark")));
    text_contract(&svg, &["VLM flow unavailable"]);

    let missing = MsesPressureResult {
        status: MsesStatus::NotRun,
        error: Some("MSES was disabled".to_owned()),
        ..MsesPressureResult::default()
    };
    let svg = render_svg(&aerodynamics::figure_mses_pressure_distribution(
        &missing,
        Some("light"),
    ));
    text_contract(&svg, &["MSES figure unavailable", "MSES was disabled"]);
    let svg = render_svg(&aerodynamics::figure_mses_mach_contours(
        &missing,
        None,
        Some("dark"),
    ));
    text_contract(&svg, &["MSES figure unavailable", "MSES was disabled"]);
}

#[test]
fn w31_mses_field_uses_the_complete_native_mplot_domain() {
    let result = MsesPressureResult {
        status: MsesStatus::Ok,
        field_x: vec![-0.4, 0.0, 1.0, 1.4, 8.0],
        field_y: vec![-0.8, 0.2, -0.1, 0.8, 8.0],
        field_mach: vec![0.4, 0.8, 1.2, 0.6, 9.0],
        airfoil_x: vec![0.0, 0.5, 1.0, 0.0],
        airfoil_y: vec![0.0, 0.1, 0.0, 0.0],
        ..MsesPressureResult::default()
    };
    let scene = aerodynamics::figure_mses_mach_contours(&result, None, Some("dark"));
    let raw_sample_count = scene
        .elements
        .iter()
        .filter(|element| matches!(element, alas_report::SceneElement::Circle { .. }))
        .count();
    assert_eq!(
        raw_sample_count, 5,
        "the full native field must be rendered"
    );
    assert!(scene
        .elements
        .iter()
        .any(|element| matches!(element, alas_report::SceneElement::Polygon { .. })));
}

#[test]
fn w31_mses_field_fills_native_mplot_grid_cells_when_row_topology_is_retained() {
    let result = MsesPressureResult {
        status: MsesStatus::Ok,
        field_x: vec![0.0, 1.0, 0.0, 1.0],
        field_y: vec![-0.5, -0.5, 0.5, 0.5],
        field_mach: vec![0.7, 0.8, 0.9, 1.0],
        field_row_offsets: vec![0, 2],
        ..MsesPressureResult::default()
    };
    let scene = aerodynamics::figure_mses_mach_contours(&result, None, Some("dark"));
    assert_eq!(
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, alas_report::SceneElement::Polygon { .. }))
            .count(),
        1,
        "the native 2x2 solver grid must render as one filled cell"
    );
    assert!(!scene
        .elements
        .iter()
        .any(|element| matches!(element, alas_report::SceneElement::Circle { .. })));
    assert!(scene.elements.iter().any(|element| {
        matches!(
            element,
            alas_report::SceneElement::Rect { fill: Some(_), .. }
        )
    }));
    let boundary = scene.elements.iter().find_map(|element| match element {
        alas_report::SceneElement::Polyline { points, .. } => Some(points),
        _ => None,
    });
    assert!(boundary.is_some_and(|points| { points.len() >= 5 && points.first() == points.last() }));
    let svg = render_svg(&scene);
    text_contract(&svg, &["outside the native MSES grid (not solved)"]);
}
