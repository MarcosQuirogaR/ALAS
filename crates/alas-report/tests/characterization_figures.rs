// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Characterization tests verifying scene generation and SVG rendering for all figure families.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_aero::analysis::PolarSweep;
use alas_aero::mses::{MsesPressureResult, MsesStatus};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::asb::airplane::Airplane;
use alas_opt::history::OptimizationHistory;
use alas_perf::performance::VnDiagramData;
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};
use alas_report::families::{
    aerodynamics, geometry, mass_balance, mission, optimization, performance, propulsion,
    screening, stability, structures,
};
use alas_report::registry::{PREVIEW_FIGURES, RESULT_FIGURES, SCREENING_FIGURES};
use alas_report::svg::render_svg;
use alas_screen::AirfoilCandidateResult;
use std::collections::HashMap;
use std::collections::HashSet;

fn sample_report() -> AnalysisReport {
    let mut report = AnalysisReport {
        design: DesignVector::default(),
        airplane: Airplane {
            name: "TestAirplane".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 120.0,
            c_ref: 4.0,
            b_ref: 35.0,
        },
        polar: PolarSweep {
            alpha_deg: Vec::new(),
            geometric_alpha_deg: Vec::new(),
            cl: Vec::new(),
            cd: Vec::new(),
            cd_induced: Vec::new(),
            cd_wave: Vec::new(),
            cd_parasite: Vec::new(),
            cm: Vec::new(),
            l_over_d: Vec::new(),
        },
        design_point: DesignPoint {
            alpha_deg: 2.2,
            cl: 0.52,
            cd: 0.0285,
            l_over_d: 18.24,
        },
        polar_fit: PolarFit {
            cd0: 0.0185,
            k: 0.042,
            oswald_e: 0.86,
            aspect_ratio: 9.8,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.125,
        x_neutral_point: 16.5,
        trimmed_design_point: None,
        component_masses: HashMap::new(),
        mass_coordinates: HashMap::new(),
        physical_cg: [15.2, 0.0, 0.0],
        geometry_summary: HashMap::new(),
        payload_layout: None,
        cg_envelope_ok: Some(true),
    };
    report.component_masses.insert("Wing".to_owned(), 8500.0);
    report
        .component_masses
        .insert("Fuselage".to_owned(), 12000.0);
    report.geometry_summary.insert("span_m".to_owned(), 36.0);
    report
}

#[test]
fn registry_descriptors_are_valid_and_non_empty() {
    assert!(!PREVIEW_FIGURES.is_empty());
    assert!(!RESULT_FIGURES.is_empty());
    assert!(!SCREENING_FIGURES.is_empty());
    for f in PREVIEW_FIGURES
        .iter()
        .chain(RESULT_FIGURES.iter())
        .chain(SCREENING_FIGURES.iter())
    {
        assert!(!f.id.is_empty());
        assert!(!f.title.is_empty());
        assert!(!f.category.is_empty());
    }
}

#[test]
fn registry_descriptors_name_the_stage_that_supplies_their_data() {
    assert!(PREVIEW_FIGURES
        .iter()
        .all(|figure| figure.required_stage == alas_report::RequiredStage::Preview));
    assert_eq!(
        alas_report::find_figure("mission_profile")
            .expect("mission figure is registered")
            .required_stage,
        alas_report::RequiredStage::Mission
    );
    assert_eq!(
        alas_report::find_figure("mses_pressure")
            .expect("MSES figure is registered")
            .required_stage,
        alas_report::RequiredStage::Mses
    );
    for id in [
        "drag_breakdown",
        "dynamic_modes",
        "span_loading",
        "stability_side_view",
    ] {
        assert_eq!(
            alas_report::find_figure(id)
                .expect("stage-collision figure is registered")
                .required_stage,
            alas_report::RequiredStage::FullAnalysis,
            "{id} must be supplied by the full analysis stage"
        );
    }
    for id in [
        "structures_sizing",
        "structures_loads",
        "structures_stress",
        "structures_modes",
        "structures_vibration",
        "structures_patran",
    ] {
        assert_eq!(
            alas_report::find_figure(id)
                .expect("structure figure is registered")
                .required_stage,
            alas_report::RequiredStage::Structures,
            "{id} must be supplied by the structures stage"
        );
    }
    assert!(SCREENING_FIGURES
        .iter()
        .all(|figure| figure.required_stage == alas_report::RequiredStage::Screening));
}

#[test]
fn result_registry_covers_the_desktop_result_screen_in_tab_order() {
    let expected = [
        "optimization_history",
        "design_evolution",
        "airfoil_comparison",
        "airfoil_evolution",
        "polar_comparison",
        "planform_comparison",
        "wireframe_wing",
        "wireframe_fuselage",
        "wireframe_empennage",
        "threeview",
        "openvsp_cad_preview",
        "aero_panel",
        "drag_breakdown",
        "span_loading",
        "vn_diagram",
        "vlm_flow",
        "dynamic_modes",
        "control_surfaces",
        "airfoil_reynolds",
        "mses_pressure",
        "mses_mach_contours",
        "mses_convergence",
        "vspaero_polar",
        "vspaero_wake_convergence",
        "vspaero_load_distribution",
        "mass_breakdown",
        "fuel_volume_check",
        "mass_distribution",
        "cg_envelope",
        "landing_gear_planform",
        "stability_side_view",
        "cabin_payload",
        "cabin_section",
        "propulsion_cycle_summary",
        "propulsion_carpet_plot",
        "propulsion_efficiency_decomposition",
        "propulsion_bpr_sensitivity",
        "propulsion_altitude_sweep",
        "structures_sizing",
        "structures_loads",
        "structures_stress",
        "structures_modes",
        "structures_vibration",
        "structures_patran",
        "mission_route_2d",
        "mission_route_3d",
        "payload_range",
        "mission_profile",
        "mission_velocities",
        "mission_flight_path",
        "mission_aero_coefficients",
        "mission_aero_forces",
        "mission_drag_components",
        "matching_chart",
        "lto_departure",
        "lto_arrival",
        "model_comparison",
    ];
    let actual: Vec<&str> = RESULT_FIGURES.iter().map(|figure| figure.id).collect();
    assert_eq!(actual, expected);
    let unique: HashSet<&str> = actual.iter().copied().collect();
    assert_eq!(unique.len(), actual.len());
}

#[test]
fn threeview_uses_a_stable_export_filename_and_registry_id() {
    assert_eq!(alas_report::export_file_stem("threeview"), "threeview");
    assert_eq!(
        alas_report::export_file_stem("polar_comparison"),
        "polar_comparison"
    );
    assert_eq!(
        alas_report::find_figure("threeview").map(|figure| figure.id),
        Some("threeview")
    );
}

#[test]
fn preview_registry_contains_only_preview_ids_with_a_gui_path() {
    let actual: Vec<&str> = PREVIEW_FIGURES.iter().map(|figure| figure.id).collect();
    assert_eq!(
        actual,
        [
            "exterior_3d",
            "cabin_3d",
            "geometry",
            "drag",
            "mass_cg",
            "landing_gear",
            "control_surfaces",
            "structures",
            "engine",
        ]
    );
}

#[test]
fn screening_registry_matches_python_sweep_display_order() {
    let actual: Vec<&str> = SCREENING_FIGURES.iter().map(|figure| figure.id).collect();
    assert_eq!(
        actual,
        [
            "trade_map",
            "rerank_2d_3d",
            "ranking_bars",
            "section_shapes",
            "mses_verification",
        ]
    );
    assert!(alas_report::find_figure("ranking_bars").is_some());
}

#[test]
fn figures_render_to_valid_svg_across_families() {
    let config = AlasConfig::default();
    let report = sample_report();

    // Geometry
    let sc_geom = geometry::figure_geometry(&report.airplane, Some("dark"));
    let svg_geom = render_svg(&sc_geom);
    assert!(svg_geom.contains("<svg"));

    // Aerodynamics
    let sc_polar = aerodynamics::figure_polar_comparison(
        &report,
        &report,
        ("baseline", "optimized"),
        None,
        Some("light"),
    );
    assert!(render_svg(&sc_polar).contains("<svg"));

    let sc_drag = aerodynamics::figure_drag_breakdown(&report, Some("grey"));
    assert!(render_svg(&sc_drag).contains("rect"));

    let pres = MsesPressureResult {
        status: MsesStatus::Ok,
        error: None,
        alpha_deg: 2.5,
        x_upper: vec![0.0, 0.5, 1.0],
        cp_upper: vec![1.0, -0.6, 0.2],
        mach_upper: vec![0.0, 0.78, 0.4],
        x_lower: vec![0.0, 0.5, 1.0],
        cp_lower: vec![1.0, 0.1, 0.2],
        mach_lower: vec![0.0, 0.65, 0.4],
        field_x: Vec::new(),
        field_y: Vec::new(),
        field_mach: Vec::new(),
        airfoil_x: Vec::new(),
        airfoil_y: Vec::new(),
        ..MsesPressureResult::default()
    };
    let sc_pres = aerodynamics::figure_mses_pressure_distribution(&pres, None);
    assert!(render_svg(&sc_pres).contains("<svg"));

    // Propulsion
    let sc_prop = propulsion::figure_propulsion_cycle_summary(&config, Some("dark"));
    assert!(render_svg(&sc_prop).contains("rect"));

    let sc_preview = propulsion::figure_engine_designer_preview(&config, Some("dark"));
    assert!(render_svg(&sc_preview).contains("<svg"));

    let sc_carpet = propulsion::figure_propulsion_carpet_plot(&config, Some("light"));
    assert!(render_svg(&sc_carpet).contains("polyline"));

    let sc_eff = propulsion::figure_propulsion_efficiency_decomposition(&config, Some("light"));
    assert!(render_svg(&sc_eff).contains("polyline"));

    let sc_bpr = propulsion::figure_propulsion_bpr_sensitivity(&config, Some("dark"));
    assert!(render_svg(&sc_bpr).contains("polyline"));

    let sc_alt_sweep = propulsion::figure_propulsion_altitude_sweep(&config, Some("dark"));
    assert!(render_svg(&sc_alt_sweep).contains("rect"));

    // Mass & Balance
    let sc_mass = mass_balance::figure_mass_breakdown(&report, Some("light"));
    assert!(render_svg(&sc_mass).contains("rect"));

    // Stability
    let sc_stab = stability::figure_dynamic_modes(&report, &config, Some("dark"));
    assert!(render_svg(&sc_stab).contains("<svg"));

    // Mission
    let mission_res = alas_mission::solve::MissionResult {
        segments: Vec::new(),
        solutions: Vec::new(),
        scheduled_segment_count: 0,
        fuel_exhaustion: None,
    };
    let sc_miss = mission::figure_mission_drag_components(&mission_res, Some("dark"));
    assert!(render_svg(&sc_miss).contains("<svg"));

    // Performance
    let vn = VnDiagramData {
        v_kt: vec![0.0, 100.0, 200.0, 300.0],
        n_stall_pos: vec![0.0, 0.8, 2.5, 2.5],
        n_stall_neg: vec![0.0, -0.3, -1.0, -1.0],
        n_lim_pos: 2.5,
        n_lim_neg: -1.0,
        n_ult_pos: 3.75,
        n_ult_neg: -1.5,
        v_s_kt: 110.0,
        v_a_kt: 180.0,
        v_c_kt: 280.0,
        v_d_kt: 340.0,
        v_cruise_op_kt: 250.0,
    };
    let sc_vn = performance::figure_vn_diagram(&vn, None);
    assert!(render_svg(&sc_vn).contains("polyline"));

    // Structures
    let sc_struct = structures::figure_structures_sizing(None, Some("dark"));
    assert!(render_svg(&sc_struct).contains("<svg"));

    // Optimization
    let mut opt_hist = OptimizationHistory::new();
    let design = alas_config::design_variables::DesignVector::default();
    for (cost, l_over_d, span_m) in [(1.45, 12.0, 28.0), (1.12, 15.0, 30.0), (0.85, 13.0, 29.0)] {
        opt_hist.record(design, true, cost, l_over_d, span_m, 0.0, 0.0, 0.0, "");
    }
    let sc_opt = optimization::figure_optimization_history(&opt_hist, Some("grey"));
    assert!(render_svg(&sc_opt).contains("polyline"));

    // Screening
    let cand1 = AirfoilCandidateResult {
        name: "NACA 2412".to_owned(),
        status: "ok".to_owned(),
        score: Some(0.92),
        cl: Some(0.55),
        cd: Some(0.0082),
        max_thickness_frac: Some(0.12),
        ..Default::default()
    };

    let cand2 = AirfoilCandidateResult {
        name: "SC2-0714".to_owned(),
        status: "ok".to_owned(),
        score: Some(0.88),
        cl: Some(0.60),
        cd: Some(0.0089),
        max_thickness_frac: Some(0.14),
        ..Default::default()
    };

    let result = alas_screen::AirfoilScreeningResult {
        candidates: vec![cand1, cand2],
        ..Default::default()
    };
    let sc_screen = screening::fig_ranking_bars(&result, Some("light"))
        .expect("ranking bars should render for candidates");
    assert!(render_svg(&sc_screen).contains("rect"));
}
