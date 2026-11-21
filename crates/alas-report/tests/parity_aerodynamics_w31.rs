// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodynamics figure contracts: Python's saved reference metadata remains the
//! source of truth for labels, panel titles, and availability semantics.

// Test code: a failed unwrap on a fixture it builds is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::analysis::PolarSweep;
use alas_aero::avl::{AvlModel, AvlPolar, AvlPolarPoint, AvlReference};
use alas_aero::mses::{MsesPressureResult, MsesStatus};
use alas_aero::vspaero::{VspaeroModel, VspaeroPolar, VspaeroPolarPoint, VspaeroReference};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
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
use std::collections::BTreeMap;
use std::path::PathBuf;

fn report() -> AnalysisReport {
    AnalysisReport {
        design: DesignVector::default(),
        airplane: Airplane {
            name: "contract aircraft".to_owned(),
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
            compressible_mach: None,
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
            c1: 0.0,
            k: 0.05,
            oswald_e: 0.85,
            aspect_ratio: 5.0,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.1,
        x_neutral_point: 4.0,
        trimmed_design_point: None,
        component_masses: BTreeMap::new(),
        flops_mass_buildup: None,
        mass_coordinates: BTreeMap::new(),
        physical_cg: [0.0, 0.0, 0.0],
        geometry_summary: BTreeMap::new(),
        payload_layout: None,
        cg_envelope_ok: Some(true),
        neutral_point_conditions: None,
        fuel: Default::default(),
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
        linear_cruise: None,
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
        linear_cruise: None,
    };
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
        None,
        None,
        Some(&avl),
        Some("dark"),
    ));

    text_contract(
        &svg,
        &[
            "Athena AVL (cross-check)",
            "Lift cross-check: takeoff climb midpoint",
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
        Some("dark"),
    ));

    text_contract(&svg, &["ALAS VLM"]);
    assert!(!svg.contains("Induced drag cross-check"));
    assert!(!svg.contains("Athena AVL"));
    assert!(!svg.contains("MSES 2-D section"));
}

#[test]
fn model_comparison_uses_one_bottom_legend_and_unmarked_model_curves() {
    let scene = aerodynamics::figure_model_comparison(&report(), None, None, None, Some("dark"));
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
        linear_cruise: None,
    };

    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
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
fn report_fed_figures_preserve_python_titles_axes_markers_and_legends() {
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
fn model_comparison_draws_no_section_coefficients_on_aircraft_axes() {
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report(),
        None,
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
        Some("dark"),
    ));

    text_contract(&svg, &["Prandtl lifting-line", "Helmbold lift slope"]);
    assert!(!svg.contains("alpha_L=0"));
    assert!(!svg.contains("does not bracket CL=0"));
}

fn vspaero_result(report: &AnalysisReport) -> VspaeroAnalysisResult {
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
    VspaeroAnalysisResult {
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
        outside_validity_alpha_deg: Vec::new(),
    }
}

#[test]
fn model_comparison_overlays_only_compatible_vspaero_lift_and_moment() {
    let report = report();
    let vspaero = vspaero_result(&report);
    let svg = render_svg(&aerodynamics::figure_model_comparison(
        &report,
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
fn optional_figures_are_data_driven_and_honest_without_external_data() {
    let report = report();
    let svg = render_svg(&aerodynamics::figure_span_loading(
        &report,
        &AlasConfig::default(),
        Some("light"),
    ));
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
fn mses_field_uses_the_complete_native_mplot_domain() {
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
fn mses_field_fills_native_mplot_grid_cells_when_row_topology_is_retained() {
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

fn texts(scene: &alas_report::scene::Scene) -> Vec<(String, [f64; 2])> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            alas_report::scene::SceneElement::Text { text, pos, .. } => Some((text.clone(), *pos)),
            _ => None,
        })
        .collect()
}

/// Numeric tick labels in a pixel window.
fn ticks_in(scene: &alas_report::scene::Scene, x: (f64, f64), y: (f64, f64)) -> Vec<f64> {
    texts(scene)
        .into_iter()
        .filter(|(_, pos)| (x.0..=x.1).contains(&pos[0]) && (y.0..=y.1).contains(&pos[1]))
        .filter_map(|(text, _)| text.parse::<f64>().ok())
        .collect()
}

const ALPHA_NOTE: &str =
    "Angle axis: compressibility-corrected angle at the lift of the incompressible lattice sweep";
const CROSS_CHECK_NOTE: &str = "Cross-check rows: geometric angle of the incompressible solves";

/// Every figure that draws the polar sweep uses the reporting
/// (Prandtl-Glauert) angle axis and says so. The fixture's reporting axis
/// spans -2.5..5 deg (ticks -2, 0, 2, 4) and its geometric one -4..8 deg
/// (ticks 0, 5), so the ticks tell the two apart.
#[test]
fn the_polar_figures_share_one_angle_axis_and_state_its_basis() {
    let mut report = report();
    report.polar.alpha_deg = vec![-2.5, 0.0, 2.5, 5.0];
    let panel = aerodynamics::figure_aero_panel(&report, Some("light"));
    // Lift-curve panel: x 70..410, tick labels just below y = 320.
    let ticks = ticks_in(&panel, (70.0, 410.0), (320.0, 335.0));
    assert!(
        ticks.contains(&-2.0) && ticks.iter().all(|tick| *tick < 6.0),
        "{ticks:?}"
    );
    assert!(texts(&panel).iter().any(|(text, _)| text == ALPHA_NOTE));
    // The note (centred at x 450, y 692, in the 700 px scene) sits below the
    // bottom-row axis titles (y 669-681): no other text within 10 px of it at
    // any x, so the clearance holds for a note of any width, Spanish included.
    let crowded = texts(&panel)
        .into_iter()
        .any(|(text, pos)| text != ALPHA_NOTE && (pos[1] - 692.0).abs() < 10.0);
    assert!(!crowded, "a text sits on the angle-axis note");

    // Model comparison with a comparable VSPAERO polar: the first row's lift
    // axes (x 60..) use the same reporting angles, and no cross-check note.
    let vspaero = vspaero_result(&report);
    let comparison =
        aerodynamics::figure_model_comparison(&report, None, Some(&vspaero), None, Some("light"));
    assert!(texts(&comparison)
        .iter()
        .any(|(text, _)| text == ALPHA_NOTE));
    assert!(!texts(&comparison)
        .iter()
        .any(|(text, _)| text == CROSS_CHECK_NOTE));
    // Angle tick labels of the lift-curve axes (x 480..850, below the frame at y 325).
    let first_row = ticks_in(&comparison, (480.0, 850.0), (326.0, 340.0));
    assert!(
        first_row.contains(&-2.0) && first_row.iter().all(|tick| *tick < 6.0),
        "{first_row:?}"
    );

    // Without any external solver: the same axis.
    let bare = aerodynamics::figure_model_comparison(&report, None, None, None, Some("light"));
    assert!(texts(&bare).iter().any(|(text, _)| text == ALPHA_NOTE));
}

/// With an AVL cross-check the extra rows are solved and drawn at the
/// geometric angle: their x axes say "geometric alpha [deg]" and the figure
/// carries a second note, so the two bases are never confused.
#[test]
fn the_avl_cross_check_rows_state_the_geometric_basis() {
    let mut report = report();
    report.polar.alpha_deg = vec![-2.5, 0.0, 2.5, 5.0];
    let avl = avl_result(
        &report,
        AvlAnalysisStatus::CompletedComparable,
        AvlComparisonStatus::Compatible(vec![
            AvlComparableQuantity::LiftCoefficient,
            AvlComparableQuantity::PitchingMomentCoefficient,
        ]),
    );
    let scene =
        aerodynamics::figure_model_comparison(&report, None, None, Some(&avl), Some("light"));
    let all = texts(&scene);
    assert!(all.iter().any(|(text, _)| text == ALPHA_NOTE));
    assert!(all.iter().any(|(text, _)| text == CROSS_CHECK_NOTE));
    assert!(
        all.iter()
            .filter(|(text, _)| text == "geometric alpha [deg]")
            .count()
            >= 2
    );
    // The notes sit below the legend, clear of every other text.
    let note_y = all
        .iter()
        .filter(|(text, _)| text == ALPHA_NOTE || text == CROSS_CHECK_NOTE)
        .map(|(_, pos)| pos[1])
        .fold(f64::INFINITY, f64::min);
    let clash = all.iter().any(|(text, pos)| {
        text != ALPHA_NOTE && text != CROSS_CHECK_NOTE && (pos[1] - note_y).abs() < 10.0
            || text != ALPHA_NOTE && text != CROSS_CHECK_NOTE && pos[1] > note_y
    });
    assert!(!clash, "a text overlaps the note lines");
}
