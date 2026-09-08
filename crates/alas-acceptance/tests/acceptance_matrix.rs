// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Acceptance matrix tests verifying end-to-end multi-preset execution,
//! disciplinary consistency, figure generation, solver degradation, and CLI integration.

// Test suite uses unwraps and assertions to validate expectations.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_acceptance::matrix::{
    evaluate_preset, format_matrix_json, format_matrix_report, generate_scenes_for_preset,
    run_acceptance_matrix, AcceptanceMatrixReport, PresetDesignMissionStatus,
};
use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::{
    DesignPipeline, FindingCode, PipelineOptions, PlanningCgStatus, RunEnvironment,
};
use alas_report::render_svg;

#[test]
fn acceptance_matrix_evaluates_all_registered_presets() {
    let report = run_acceptance_matrix();
    assert!(
        report.all_executed,
        "Every registered preset should execute"
    );
    assert_eq!(
        report.presets.len(),
        presets::available().len(),
        "Matrix should evaluate all published presets"
    );
    assert_eq!(
        report.all_physical_passed,
        report.presets.iter().all(|preset| preset.physical_passed)
    );
    assert_eq!(
        report.all_passed,
        report.all_executed && report.all_physical_passed && report.all_design_missions_verified
    );

    for p in &report.presets {
        assert!(p.geometry_valid, "Geometry must be valid for {}", p.name);
        assert!(p.mtow_kg > p.oew_kg, "MTOW > OEW for {}", p.name);
        assert!(p.payload_kg > 0.0, "Positive payload for {}", p.name);
        assert!(
            p.cruise_l_over_d > 8.0,
            "Aerodynamic L/D > 8.0 for {}",
            p.name
        );
        assert!(p.neutral_point_x > 0.0, "Positive NP for {}", p.name);
        assert_eq!(
            p.design_mission_status,
            PresetDesignMissionStatus::Unverified,
            "{} has no registered source-backed design mission",
            p.name
        );
        assert!(
            p.figure_scenes_count >= 8,
            "Generates at least 8 figure scenes for {}",
            p.name
        );
        let equilibrium = p
            .cruise_equilibrium
            .as_ref()
            .unwrap_or_else(|| panic!("{} must retain cruise force telemetry", p.name));
        assert!(
            equilibrium.is_finite(),
            "{} cruise force telemetry must be finite",
            p.name
        );
    }

    for name in ["A220-300", "A320-200"] {
        let result = report
            .presets
            .iter()
            .find(|preset| preset.name == name)
            .expect("narrowbody preset in acceptance matrix");
        assert_eq!(
            result.design_mission_status,
            PresetDesignMissionStatus::Unverified
        );
    }
}

#[test]
fn public_planning_cg_uses_the_source_frame_without_becoming_a_certification_claim() {
    let result = evaluate_preset("A220-300").expect("A220 evaluation");

    assert!(result.execution_passed);
    // Current product state of the A220-300 preset, with its PW1521G-3
    // binding at the ICAO rating and the bulk-only lower hold from its
    // weight-and-balance manual, flown at the EASA basic-scheme takeoff mass
    // with geometry-derived component stations: the takeoff CG lies inside
    // the published planning envelope at that mass, the operating-empty
    // state sits forward of the model's configured forward range, and the
    // published tanks cannot reach MTOW. These are open physical findings;
    // the assertions record them as reported rather than passed.
    assert!(!result.model_cg_envelope_ok);
    assert_eq!(
        result.public_planning_cg_status,
        PlanningCgStatus::WithinPublishedLimits
    );
    assert!(!result.physical_passed);
    for (code, severity) in [
        (
            FindingCode::ModelCgForwardRangeViolation,
            alas_pipeline::FindingSeverity::Error,
        ),
        (
            FindingCode::TankLimitedTakeoffMass,
            alas_pipeline::FindingSeverity::Warning,
        ),
    ] {
        assert!(
            result
                .physical_findings
                .iter()
                .any(|finding| finding.code == code && finding.severity == severity),
            "{code:?} is not reported: {:?}",
            result.physical_findings
        );
    }
    // The maximum-fuel case reported these; the policy load case does not.
    for code in [
        FindingCode::PublicPlanningCgEnvelopeViolation,
        FindingCode::MinimumNoseGearLoadViolation,
        FindingCode::LandingMassLimitViolation,
    ] {
        assert!(
            !result
                .physical_findings
                .iter()
                .any(|finding| finding.code == code),
            "{code:?} is reported for the policy load case: {:?}",
            result.physical_findings
        );
    }
    assert!(result.mission_fuel_within_available);
    assert!(!result
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::TrimUnavailable));

    // Regression pins of the same analyzed takeoff state in the two frames.
    // They are product-state values, not validated aircraft data.
    assert!(
        (result.model_cg_pct_mac - 38.797_602_412_110_95).abs() < 1.0e-6,
        "model-frame CG was {}% MAC",
        result.model_cg_pct_mac
    );
    let public_pct_mac = result
        .public_planning_cg_pct_mac
        .expect("A220 has a source planning frame");
    assert!(
        (public_pct_mac - 35.420_764_248_083_17).abs() < 1.0e-6,
        "public-frame CG was {public_pct_mac}% MAC"
    );

    // The two percentages must name one physical station. The public frame
    // is the manufacturer's LEMAC and MAC from the recovery publication; the
    // model frame is the built wing's quarter-chord aerodynamic centre and
    // the airplane's reference chord. Recomputing both from those primitives
    // checks the frame mapping independently of the pipeline's conversion.
    let preset = presets::get("A220-300").expect("registered A220");
    let envelope = preset
        .reference
        .planning_cg_envelope
        .expect("A220 planning envelope");
    let reference = envelope.mac_reference;
    let x_public_m = reference.lemac_from_aircraft_nose_m
        + public_pct_mac / 100.0 * reference.mean_aerodynamic_chord_m;
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .expect("preset configuration");
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A220 airplane");
    let wing = airplane.wings.first().expect("main wing");
    let x_mac_le_m = wing.aerodynamic_center(0.25)[0] - 0.25 * airplane.c_ref;
    let x_model_m = x_mac_le_m + result.model_cg_pct_mac / 100.0 * airplane.c_ref;
    assert!(
        (x_public_m - x_model_m).abs() < 1.0e-6,
        "public frame station {x_public_m} m and model frame station {x_model_m} m differ"
    );
    assert!(
        (reference.mean_aerodynamic_chord_m - airplane.c_ref).abs() > 1.0e-3
            || (reference.lemac_from_aircraft_nose_m - x_mac_le_m).abs() > 1.0e-3,
        "the published frame is expected to differ from the model frame"
    );
    assert!(envelope.controlling_document.contains("WBM"));
    assert!(envelope
        .mac_reference
        .source
        .document
        .contains("Aircraft Recovery Publication"));

    let report = alas_acceptance::matrix::AcceptanceMatrixReport {
        presets: vec![result],
        all_executed: true,
        all_passed: false,
        all_physical_passed: false,
        all_design_missions_verified: false,
    };
    let text = format_matrix_report(&report);
    assert!(text.contains("Execution Verdict: ALL PRESETS EXECUTED"));
    assert!(text.contains("Physical Verdict: 1 preset finding(s) require investigation"));
    assert!(text.contains("Design mission evidence:"));
    assert!(text.contains("A220-300: UNVERIFIED - no source-backed mission registered"));
    assert!(text.contains("Interactive route diagnostics (not preset design-mission validation):"));
    assert!(text.contains("Acceptance Verdict: NOT PASSED - DESIGN MISSIONS ALSO UNVERIFIED"));
    assert!(text.contains("Cruise force-balance telemetry"));
    // The planning frame is reported as public planning evidence, separate
    // from the model assessment, whether or not a limit is violated.
    assert!(text.contains("separate from public planning evidence"));
    assert!(text.contains("WITHIN"));
    assert!(!text.to_ascii_lowercase().contains("certif"));

    let json = format_matrix_json(&report).expect("acceptance JSON artifact");
    let artifact: serde_json::Value = serde_json::from_str(&json).expect("valid acceptance JSON");
    assert_eq!(
        artifact["presets"][0]["fuel_loading"]["usable_capacity_evidence"],
        "published preset"
    );
    assert_eq!(
        artifact["presets"][0]["cruise_equilibrium"]["status"],
        "finite"
    );
}

#[test]
fn a380_soft_static_margin_target_does_not_become_a_hard_model_constraint() {
    let result = evaluate_preset("A380-800").expect("A380 evaluation");

    assert!(result.execution_passed);
    assert!(result.model_cg_envelope_ok);
    assert!(result.model_cg_minimum_loading_static_margin >= result.model_cg_static_margin_floor);
    assert!(!result.physical_findings.iter().any(|finding| matches!(
        finding.code,
        FindingCode::InsufficientStaticMargin
            | FindingCode::ModelCgForwardRangeViolation
            | FindingCode::NoseGearStrengthViolation
            | FindingCode::MainGearStrengthViolation
            | FindingCode::MinimumNoseGearLoadViolation
    )));

    let matrix = alas_acceptance::matrix::AcceptanceMatrixReport {
        presets: vec![result],
        all_executed: true,
        all_passed: false,
        all_physical_passed: true,
        all_design_missions_verified: false,
    };
    let text = format_matrix_report(&matrix);
    assert!(text.contains("A380-800: hard constraints PASS"));
    assert!(text.contains("hard floor 5.000%"));
    assert!(text.contains("target preference 10.000%"));
    assert!(!text.to_ascii_lowercase().contains("certif"));
}

#[test]
fn acceptance_narrowbody_and_widebody_mass_calibrations() {
    let a320 = evaluate_preset("A320-200").expect("A320 evaluation");
    let a380 = evaluate_preset("A380-800").expect("A380 evaluation");
    let ave = evaluate_preset("AVE").expect("AVE evaluation");

    // A320 narrowbody MTOW is around 70-80 tonnes
    assert!(
        a320.mtow_kg > 60_000.0 && a320.mtow_kg < 90_000.0,
        "A320 MTOW in expected range: {}",
        a320.mtow_kg
    );
    // Regression pin of the MTOW closure remainder over the source-corrected
    // A320-214 WV017 preset (Airbus/EASA planform, empennage, fuselage height
    // and 21,256 kg structural payload) with its typed CFM56-5B4/3 binding at
    // the 120.1 kN ICAO rating. It is a product-state pin, not a validated
    // fuel figure: the typed rating alone moves it by about 100 kg through
    // the thrust-scaled propulsion mass. Mass conservation is asserted below
    // independently of the pinned value.
    assert!(
        (a320.mtow_closure_fuel_kg - 16_882.220_987_229_135).abs() < 1.0e-3,
        "A320 MTOW closure fuel was {} kg",
        a320.mtow_closure_fuel_kg
    );
    // The interactive route (LEMD-LEPA) is flown at the takeoff mass the
    // EASA basic fuel scheme requires, closed by re-flying the native
    // mission until the mass settles, so the carried fuel is the policy
    // load rather than the closure remainder. The pinned values are
    // product-state figures of that closure, not an operational flight plan.
    assert_eq!(a320.usable_fuel_capacity_kg, Some(19_334.0));
    assert!(
        (a320.analyzed_carried_fuel_kg - 9_917.501_715_558).abs() < 1.0e-3,
        "A320 policy takeoff fuel was {} kg",
        a320.analyzed_carried_fuel_kg
    );
    assert!(a320.analyzed_carried_fuel_kg < a320.mtow_closure_fuel_kg);
    assert!(a320.analyzed_carried_fuel_kg < a320.usable_fuel_capacity_kg.unwrap());
    assert!(
        (a320.analyzed_takeoff_mass_kg - 71_035.280_728_329).abs() < 1.0e-3,
        "A320 policy takeoff mass was {} kg",
        a320.analyzed_takeoff_mass_kg
    );
    let a320_zero_fuel_mass_kg = a320.mtow_kg - a320.mtow_closure_fuel_kg;
    assert!(
        (a320.analyzed_takeoff_mass_kg - (a320_zero_fuel_mass_kg + a320.analyzed_carried_fuel_kg))
            .abs()
            < 1.0e-6
    );
    assert!(a320.analyzed_takeoff_mass_kg < a320.mtow_kg);
    assert!(a320.mtow_shortfall_kg.abs() < 1.0e-6);
    // At the policy takeoff mass the route completes within the loaded fuel
    // and lands below the WV017 maximum landing mass. The open physical
    // finding is the operating-empty centre of gravity, which the
    // geometry-derived stations place forward of the model's configured
    // forward range; it is reported rather than passed.
    assert!(a320.mission_fuel_within_available);
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::MissionFuelShortfall));
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::LandingMassLimitViolation));
    assert!(a320.physical_findings.iter().any(|finding| {
        finding.code == FindingCode::ModelCgForwardRangeViolation
            && finding.severity == alas_pipeline::FindingSeverity::Error
    }));
    assert!(!a320.physical_passed);
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| { finding.code == FindingCode::FuelCapacityUnavailable }));
    let a320_text = format_matrix_report(&AcceptanceMatrixReport {
        presets: vec![a320.clone()],
        all_executed: true,
        all_passed: false,
        all_physical_passed: false,
        all_design_missions_verified: false,
    });
    assert!(a320_text.contains("Tank-limited load cases:"));
    assert!(a320_text.contains("- None"));
    assert!(!a320_text.contains("usable fuel was exhausted during mission segment"));

    // A380 mega-widebody MTOW is around 500-600 tonnes
    assert!(
        a380.mtow_kg > 450_000.0 && a380.mtow_kg < 650_000.0,
        "A380 MTOW in expected range: {}",
        a380.mtow_kg
    );

    // AVE 777X-class widebody reference twin MTOW is around 320-380 tonnes
    assert!(
        ave.mtow_kg > 300_000.0 && ave.mtow_kg < 400_000.0,
        "AVE MTOW in expected range: {}",
        ave.mtow_kg
    );
    let ave_forward_finding = ave
        .physical_findings
        .iter()
        .find(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation)
        .expect("the notional AVE structural model exposes its forward-CG finding");
    assert_eq!(
        ave_forward_finding.severity,
        alas_pipeline::FindingSeverity::Error
    );
    // `evaluate_preset` intentionally follows the product path, whose
    // geometry-derived component stations (nacelles at mid-length ahead of
    // the wing, wing mass in the spar box) are distinct from the frozen
    // compatibility coordinates used to establish the historical 14.749%
    // reference; the finding is reported at the analyzed zero-fuel state.
    assert!(
        (ave_forward_finding.actual.expect("AVE CG actual") - 12.417_881_232_502_797).abs() < 0.01
    );
    assert!((ave_forward_finding.limit.expect("AVE CG limit") - 18.194).abs() < 0.01);
    assert_eq!(ave_forward_finding.unit, "% MAC");
    assert!(!ave
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::FuelCapacityUnavailable));
}

#[test]
fn acceptance_scene_and_svg_export_integrity() {
    let preset = presets::get("A320-200").expect("preset");
    // Select the preset through the loader, as `evaluate_preset` does. A
    // hand-assembled config skips the preset's cabin and design seed, and a
    // widebody default cabin over an A320 shell exceeds the A320 MTOW before
    // any figure is rendered.
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .expect("preset configuration");

    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let airplane = builder
        .build(Some(&preset.design_vector), true)
        .expect("airplane build");

    let pipeline = DesignPipeline::new(config.clone());
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let res = pipeline
        .run(&options, &RunEnvironment::default())
        .expect("pipeline run");
    let scenes = generate_scenes_for_preset(&res, &config, &airplane);

    assert!(
        !scenes.is_empty(),
        "Should generate disciplinary figure scenes"
    );
    for scene in &scenes {
        let svg = render_svg(scene);
        assert!(
            svg.starts_with("<svg") && svg.contains("</svg>"),
            "Scene {:?} should render valid SVG document",
            scene.title
        );
    }
}

#[test]
fn acceptance_solver_degradation_without_external_tools() {
    let config = AlasConfig::default();
    let pipeline = DesignPipeline::new(config);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    // Running with None for external tools should succeed using analytical fallbacks
    let result = pipeline
        .run(&options, &RunEnvironment::default())
        .expect("pipeline run");
    assert!(
        result.baseline_report.is_some(),
        "Baseline analysis succeeds without external solvers"
    );
    assert!(
        result.structural_result.is_some(),
        "Structural sizing succeeds with analytical method"
    );
}

#[test]
fn acceptance_cli_headless_dry_run() {
    use alas_app::cli::{load_config, parse_args};

    let args = vec![
        "--no-optimize".to_string(),
        "--no-mission".to_string(),
        "--quiet".to_string(),
    ];
    let cli = parse_args(&args).expect("parse args").expect("some args");
    assert!(cli.no_optimize);
    assert!(cli.no_mission);
    assert!(cli.quiet);

    let config = load_config(&cli).expect("load config");
    assert!(!config.mission.enabled);
}
