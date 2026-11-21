// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{FullAnalysis, PolarFit, PolarFitStatus};
use alas_aero::analysis::PolarSweep;
use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, AnalysisConfig};
use alas_geom::builder::AircraftBuilder;

fn sweep(cl: Vec<f64>, cd: Vec<f64>) -> PolarSweep {
    let n = cl.len();
    PolarSweep {
        alpha_deg: vec![0.0; n],
        geometric_alpha_deg: vec![0.0; n],
        compressible_mach: None,
        cl,
        cd,
        cd_induced: vec![0.0; n],
        cd_wave: vec![0.0; n],
        cd_parasite: vec![0.0; n],
        cm: vec![0.0; n],
        l_over_d: vec![0.0; n],
    }
}

#[test]
fn a_retained_mass_model_does_not_change_the_wing_the_aerodynamics_sees() {
    let preset = alas_config::presets::get("A320-200").expect("registered preset");
    let product =
        AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).expect("product case");
    let retained = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "mass_model": { "mass_architecture": "legacy_reference_compatible_comparison" }
    }))
    .expect("retained mass model case");
    let area = |config: AlasConfig| {
        FullAnalysis::new(config)
            .run(&preset.design_vector, true)
            .expect("the analysis runs")
            .geometry_summary["wing_area_m2"]
    };
    assert_eq!(area(product), area(retained));
}

#[test]
fn product_full_analysis_preserves_the_live_engine_configuration() {
    let mut config = AlasConfig::default();
    config.geometry.engine.engine_name = "Trent 900".to_owned();
    config.geometry.engine.nacelle_profile = vec![(0.0, 0.33), (2.5, 1.0), (6.2, 0.41)];
    config.geometry.engine.radius_scale_m = 1.91;
    config
        .geometry
        .engine
        .turbofan
        .as_mut()
        .unwrap()
        .rated_thrust_kn = 399.0;
    config.geometry.engine.bypass_ratio = 9.2;
    config.geometry.engine.overall_pressure_ratio = 43.0;
    config.geometry.engine.fan_pressure_ratio = 1.59;
    config.geometry.engine.turbine_inlet_temp_k = 1712.0;
    config.geometry.engine.cruise_tsfc_kg_kgf_hr = 0.481;
    config.geometry.engine.fan_diameter_m = 3.11;
    let expected = config.geometry.engine.clone();

    let analysis = FullAnalysis::new(config);

    assert_eq!(analysis.config.geometry.engine, expected);
    let builder = AircraftBuilder::new(Some(analysis.config.geometry.clone()));
    assert_eq!(builder.geometry.engine, expected);
}

/// A
/// product run carries the full multi-condition neutral-point set
/// alongside the existing fixed-condition `x_neutral_point`, and its
/// `critical` station is never more aft than any of its own rigid
/// conditions (by construction, `neutral_point_conditions` takes the
/// minimum). The frozen `reference_compatibility` path carries none.
#[test]
fn a_product_report_carries_the_full_neutral_point_conditions_set() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .expect("A320-200 preset");
    let design = alas_config::presets::get("A320-200")
        .expect("registered preset")
        .design_vector;

    let analysis = FullAnalysis::new(config.clone());
    let report = analysis
        .run(&design, true)
        .expect("the registered A320-200 must analyze");
    let conditions = report
        .neutral_point_conditions
        .expect("a product analysis carries the conditions set");
    assert!(conditions.critical <= conditions.clean_low_speed);
    assert!(conditions.critical <= conditions.cruise);
    assert!(conditions.critical <= conditions.high_lift);
    assert!(conditions.critical.is_finite());

    let reference = FullAnalysis::new_reference_compatibility(config);
    let reference_report = reference
        .run(&design, true)
        .expect("the registered A320-200 must analyze in compatibility mode");
    assert!(reference_report.neutral_point_conditions.is_none());
}

fn assert_failed_fit_carries_no_coefficients(fit: &PolarFit) {
    assert!(!fit.status.is_fitted());
    assert!(fit.cd0.is_nan() && fit.c1.is_nan() && fit.k.is_nan() && fit.oswald_e.is_nan());
    let json = serde_json::to_string(fit).expect("a failed fit serializes");
    let read: PolarFit = serde_json::from_str(&json).expect("a failed fit reads back");
    assert_eq!(read.status, fit.status);
    assert!(read.cd0.is_nan() && read.k.is_nan());
}

#[test]
fn a_fit_with_non_positive_curvature_is_a_failed_fit() {
    // Drag falling with lift squared: the solve succeeds with k < 0.
    let fit = FullAnalysis::fit_polar_values(
        &sweep(
            vec![0.35, 0.45, 0.55, 0.65],
            vec![0.030, 0.028, 0.025, 0.021],
        ),
        10.0,
        &AnalysisConfig::default(),
    );

    assert_eq!(fit.status, PolarFitStatus::NonPhysicalCoefficients);
    assert_failed_fit_carries_no_coefficients(&fit);
}

#[test]
fn degenerate_polar_fit_carries_a_failed_status_and_no_coefficients() {
    let fit = FullAnalysis::fit_polar_values(
        &sweep(vec![0.0], vec![0.03]),
        10.0,
        &AnalysisConfig::default(),
    );

    assert_eq!(fit.status, PolarFitStatus::FallbackInsufficientPoints);
    assert_eq!(fit.status.as_str(), "failed_insufficient_points");
    assert_failed_fit_carries_no_coefficients(&fit);
}

#[test]
fn rank_deficient_polar_fit_carries_a_failed_status_and_no_coefficients() {
    let fit = FullAnalysis::fit_polar_values(
        &sweep(vec![0.4, 0.4, 0.4], vec![0.03, 0.04, 0.05]),
        10.0,
        &AnalysisConfig::default(),
    );

    assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
    assert_eq!(fit.status.as_str(), "failed_least_squares");
    assert_failed_fit_carries_no_coefficients(&fit);
}

#[test]
fn nonfinite_selected_polar_values_fail_the_fit() {
    for invalid_cd in [f64::NAN, f64::INFINITY] {
        let fit = FullAnalysis::fit_polar_values(
            &sweep(vec![0.35, 0.45, 0.55], vec![0.0249, invalid_cd, 0.0321]),
            10.0,
            &AnalysisConfig::default(),
        );

        assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
        assert_failed_fit_carries_no_coefficients(&fit);
    }
}

#[test]
fn nonfinite_least_squares_solution_fails_the_fit() {
    let config = AnalysisConfig {
        polar_fit_cl_min: 0.0,
        polar_fit_cl_max: f64::MAX,
        ..AnalysisConfig::default()
    };

    // These source values are finite, but CL squared overflows while building the
    // fit matrix. The non-finite QR result must not acquire fitted status.
    let fit = FullAnalysis::fit_polar_values(
        &sweep(vec![1.0e200, 2.0e200, 3.0e200], vec![0.02, 0.03, 0.04]),
        10.0,
        &config,
    );

    assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
    assert_failed_fit_carries_no_coefficients(&fit);
}

#[test]
fn successful_fits_identify_their_window_provenance() {
    let primary = FullAnalysis::fit_polar_values(
        &sweep(vec![0.35, 0.45, 0.55], vec![0.0249, 0.0281, 0.0321]),
        10.0,
        &AnalysisConfig::default(),
    );
    assert_eq!(primary.status, PolarFitStatus::Fitted);

    let fallback_window = FullAnalysis::fit_polar_values(
        &sweep(vec![0.2, 0.4, 0.7], vec![0.0216, 0.0264, 0.0396]),
        10.0,
        &AnalysisConfig::default(),
    );
    assert_eq!(fallback_window.status, PolarFitStatus::FittedFallbackWindow);
}

#[test]
fn shifted_wake_polar_preserves_linear_term_and_induced_curvature() {
    // An elliptic planar wake has k=1/(pi AR). A cambered, fixed-incidence
    // tail can add a zero-lift term and shift the minimum; discarding c1
    // changes k and can falsely report a span efficiency above one.
    let ar = 10.0;
    let k = 1.0 / (std::f64::consts::PI * ar);
    let cl = vec![0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    let cd = cl
        .iter()
        .map(|cl| 0.02 - 0.004 * cl + k * cl * cl)
        .collect();
    let config = AnalysisConfig {
        polar_fit_cl_min: 0.1,
        polar_fit_cl_max: 0.8,
        ..AnalysisConfig::default()
    };
    let fit = FullAnalysis::fit_polar_values(&sweep(cl, cd), ar, &config);
    assert_eq!(fit.status, PolarFitStatus::Fitted);
    assert!((fit.cd0 - 0.02).abs() < 1e-12);
    assert!((fit.c1 + 0.004).abs() < 1e-12);
    assert!((fit.k - k).abs() < 1e-12);
    assert!((fit.oswald_e - 1.0).abs() < 1e-12);
}

#[test]
fn the_product_fit_leaves_the_wave_term_out() {
    // A quartic rise above M_crit(CL) on the same parabola: the fit must not
    // read it as lift-dependent drag, and must return the wave-free polar.
    let ar = 10.0;
    let k = 1.0 / (std::f64::consts::PI * ar * 0.8);
    let cl = vec![0.32, 0.41, 0.5, 0.58];
    let wave: Vec<f64> = cl
        .iter()
        .map(|cl| 20.0 * (0.85_f64 - (0.80 - 0.15 * cl)).max(0.0).powi(4))
        .collect();
    let clean: Vec<f64> = cl
        .iter()
        .map(|cl| 0.02 - 0.004 * cl + k * cl * cl)
        .collect();
    let total = clean.iter().zip(&wave).map(|(c, w)| c + w).collect();
    let mut polar = sweep(cl, total);
    polar.cd_wave = wave;
    let fit = FullAnalysis::fit_polar_values(&polar, ar, &AnalysisConfig::default());
    assert_eq!(fit.status, PolarFitStatus::Fitted);
    assert!((fit.cd0 - 0.02).abs() < 1e-12);
    assert!((fit.c1 + 0.004).abs() < 1e-11);
    assert!((fit.oswald_e - 0.8).abs() < 1e-10);
}

#[test]
fn successful_full_analysis_retains_the_detailed_payload_layout() {
    let report = FullAnalysis::new(AlasConfig::default())
        .run(&DesignVector::default(), false)
        .unwrap_or_else(|error| panic!("default full analysis should resolve payload: {error}"));

    let layout = match report.payload_layout.as_ref() {
        Some(layout) => layout,
        None => panic!("a successful full analysis must carry its detailed layout"),
    };
    assert!(layout.total_mass.is_finite());
    assert!(layout.cg_x.is_finite());
    assert!(layout.cg_y.is_finite());
}

/// The derived wing setting of a clean-sheet transport rotates the
/// wing the sized report draws, but the published design vector keeps the
/// caller's tip twist: the rotation rides in `TailSizing::wing_incidence_deg`.
/// The search delivers `report.design` as its finalist and feeds it back; a
/// rotated tip there, with the configuration's unrotated root and break, changed
/// the washout of every rebuild (default design: tip -2.667 deg against 0).
#[test]
fn a_sized_report_publishes_the_design_vector_without_the_derived_wing_setting() {
    // The wing setting is derived in the sizing closure; the mission and
    // structures stages do not take part in it and only cost time here.
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    let design = DesignVector::default();
    let assessment =
        alas_opt::assess_product_candidate(&config, &design).expect("candidate assessment");
    let incidence = assessment
        .sized
        .fuel_artifacts
        .tail_sizing
        .wing_incidence_deg
        .expect("the default clean-sheet transport derives its wing setting");
    let report = FullAnalysis::new(config)
        .run_sized_candidate(&assessment.resolved.design, &assessment.sized)
        .expect("sized candidate report");
    assert_eq!(
        report.design.tip_twist_deg.to_bits(),
        assessment.resolved.design.tip_twist_deg.to_bits()
    );
    // The drawn wing does carry the rotation.
    assert!((incidence[2] - assessment.resolved.design.tip_twist_deg).abs() > 1e-6);
}
