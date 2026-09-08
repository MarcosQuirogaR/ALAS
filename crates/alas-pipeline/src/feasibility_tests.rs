// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::presets;

#[test]
fn warnings_do_not_turn_a_report_into_an_infeasible_aircraft() {
    let report = FeasibilityReport {
        findings: vec![PhysicalFinding {
            code: FindingCode::WingAreaLimit,
            severity: FindingSeverity::Warning,
            message: "review".to_owned(),
            actual: Some(1.0),
            limit: Some(1.0),
            unit: "m^2",
        }],
        ..FeasibilityReport::default()
    };
    assert!(report.is_feasible());
    assert!(report.contains(FindingCode::WingAreaLimit));
}

#[test]
fn cruise_equilibrium_reports_scalar_and_inertial_force_residuals() {
    use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};

    let spec = SegmentSpec {
        tag: "cruise_probe".to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: Some(10_000.0),
            distance_m: 1_000.0,
        },
        air_speed_m_s: 250.0,
        air_speed_reference: alas_config::mission::SpeedReference::TrueAirspeed,
        true_course_rad: 0.0,
        temperature_deviation_k: 0.0,
        number_control_points: 2,
    };
    let mut segment = Segment::new(spec, None).expect("declared cruise altitude");
    segment.numerics.converged = Some(true);
    for point in 0..2 {
        segment.conditions.thrust_force_vector_n[point] = [100_500.0, 0.0, 0.0];
        segment.conditions.wind_drag_force_vector_n[point] = [-100_000.0, 0.0, 0.0];
        segment.conditions.wind_lift_force_vector_n[point] = [0.0, 0.0, -200_250.0];
        segment.conditions.gravity_force_vector_n[point] = [0.0, 0.0, 200_000.0];
        segment.conditions.total_force_vector_n[point] = [400.0, 0.0, -150.0];
        segment.conditions.total_mass_kg[point] = 20_000.0;
        segment.conditions.altitude_m[point] = 10_000.0;
        segment.conditions.velocity_vector_m_s[point] = [250.0, 0.0, 0.0];
    }
    let mission = MissionResult {
        segments: vec![segment],
        solutions: Vec::new(),
        scheduled_segment_count: 1,
        fuel_exhaustion: None,
    };

    let assessment = assess_cruise_equilibrium(&mission);

    assert!(assessment.is_finite());
    assert!(assessment.all_segments_converged);
    assert_eq!(assessment.control_points, 2);
    assert_eq!(assessment.max_abs_thrust_minus_drag_n, Some(500.0));
    assert_eq!(assessment.max_abs_lift_minus_weight_n, Some(250.0));
    assert_eq!(assessment.max_abs_longitudinal_residual_n, Some(400.0));
    assert_eq!(assessment.max_abs_vertical_residual_n, Some(150.0));
    assert_eq!(assessment.minimum_altitude_m, Some(10_000.0));
    assert_eq!(assessment.maximum_true_airspeed_m_s, Some(250.0));
}

#[test]
fn no_cruise_telemetry_is_not_reported_as_force_balance_evidence() {
    let assessment = assess_cruise_equilibrium(&MissionResult {
        segments: Vec::new(),
        solutions: Vec::new(),
        scheduled_segment_count: 0,
        fuel_exhaustion: None,
    });

    assert!(!assessment.is_finite());
    assert!(!assessment.all_segments_converged);
}

#[test]
fn a_tank_limited_mtow_closure_is_reported_without_becoming_an_error() {
    let loading = fuel::plan_from_values(
        78_000.0,
        19_399.954_722_136_66,
        FuelCapacityAssessment {
            capacity_kg: Some(19_334.0),
            evidence: FuelCapacityEvidence::PublishedPreset,
        },
    );
    let findings = fuel::findings(78_000.0, &loading);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, FindingCode::TankLimitedTakeoffMass);
    assert_eq!(findings[0].severity, FindingSeverity::Warning);
    let report = FeasibilityReport {
        findings,
        fuel_loading: loading,
        ..FeasibilityReport::default()
    };
    assert!(report.is_feasible());
    let text = format_feasibility(&report);
    assert!(text.contains("WARNING"));
    assert!(text.contains("77934.045 kg"));
}

#[test]
fn missing_tank_capacity_prevents_a_verified_physical_verdict() {
    let loading = fuel::plan_from_values(10_000.0, 2_000.0, FuelCapacityAssessment::default());
    let findings = fuel::findings(10_000.0, &loading);

    assert!(findings.iter().any(|finding| {
        finding.code == FindingCode::FuelCapacityUnavailable
            && finding.severity == FindingSeverity::Error
    }));
}

#[test]
fn an_error_is_a_physical_failure_even_when_other_checks_pass() {
    let report = FeasibilityReport {
        findings: vec![error(
            FindingCode::NonPositiveFuel,
            "no fuel",
            Some(-1.0),
            Some(0.0),
            "kg",
        )],
        ..FeasibilityReport::default()
    };
    assert!(!report.is_feasible());
    let text = format_feasibility(&report);
    assert!(text.contains("INFEASIBLE"));
    assert!(text.contains("actual -1.000 kg"));
}

#[test]
fn the_a220_reference_check_uses_the_interpolated_public_planning_limits() {
    let reference = &presets::get("A220-300")
        .expect("registered A220 preset")
        .reference;
    let within = assess_reference_limits(reference, Some((56_000.0, 20.0)));
    let forward = assess_reference_limits(reference, Some((56_000.0, 10.0)));
    let aft = assess_reference_limits(reference, Some((56_000.0, 38.0)));

    assert_eq!(
        within.planning_status,
        PlanningCgStatus::WithinPublishedLimits
    );
    assert_eq!(
        forward.planning_status,
        PlanningCgStatus::ForwardLimitViolation
    );
    assert_eq!(aft.planning_status, PlanningCgStatus::AftLimitViolation);
    assert!(within.forward_limit_pct_mac.is_some());
    assert!(within.aft_limit_pct_mac.is_some());
    assert!(within.source.is_some());
}

#[test]
fn public_planning_percent_mac_uses_the_manufacturer_longitudinal_frame() {
    let envelope = presets::get("A220-300")
        .expect("registered A220 preset")
        .reference
        .planning_cg_envelope
        .expect("A220 has a published planning envelope");
    let reference = envelope.mac_reference;
    let cg_x_m = reference.lemac_from_aircraft_nose_m + 0.316 * reference.mean_aerodynamic_chord_m;

    let cg_pct_mac =
        planning_cg_pct_mac(cg_x_m, reference).expect("published reference dimensions are valid");

    assert!((cg_pct_mac - 31.6).abs() < 1.0e-12);
    assert_eq!(reference.lemac_from_aircraft_nose_m, 16.535_349_2);
    assert_eq!(reference.mean_aerodynamic_chord_m, 3.781_044);
}

#[test]
fn the_mrw_station_does_not_claim_to_have_an_aft_planning_limit() {
    let reference = &presets::get("A220-300")
        .expect("registered A220 preset")
        .reference;
    let assessment = assess_reference_limits(reference, Some((68_039.0, 20.0)));

    assert_eq!(
        assessment.planning_status,
        PlanningCgStatus::AftLimitNotPublished
    );
    assert_eq!(assessment.forward_limit_pct_mac, Some(18.6));
    assert_eq!(assessment.aft_limit_pct_mac, None);
}

#[test]
fn an_afm_required_preset_is_not_given_a_synthetic_planning_check() {
    let mut reference = presets::get("A220-300")
        .expect("registered A220 preset")
        .reference
        .clone();
    reference.cg_evidence = CgEnvelopeEvidence::AfmRequired;
    let assessment = assess_reference_limits(&reference, Some((67_000.0, 20.0)));

    assert_eq!(assessment.evidence, CgEnvelopeEvidence::AfmRequired);
    assert_eq!(assessment.planning_status, PlanningCgStatus::NotEvaluated);
    assert_eq!(assessment.forward_limit_pct_mac, None);
    assert_eq!(assessment.aft_limit_pct_mac, None);
}

#[test]
fn public_planning_output_names_the_wbm_and_makes_no_certification_claim() {
    let reference = &presets::get("A220-300")
        .expect("registered A220 preset")
        .reference;
    let report = FeasibilityReport {
        cg_envelope: assess_reference_limits(reference, Some((67_585.0, 25.0))),
        ..FeasibilityReport::default()
    };
    let text = format_feasibility(&report);

    assert!(text.contains("PUBLIC PLANNING LIMITS"));
    assert!(text.contains("WBM"));
    assert!(!text.to_ascii_lowercase().contains("certif"));
}

#[test]
fn tank_limited_model_cg_uses_the_analyzed_fuel_and_names_the_load_case_honestly() {
    let preset = presets::get("A320-200").expect("registered A320 preset");
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        landing_gear: preset.landing_gear.clone(),
        ..Default::default()
    };
    if let Some(mass_model) = &preset.mass_model {
        config.mass_model = mass_model.clone();
    }
    // Keep this conservation regression on the frozen A320 fixture. Product
    // high-lift/gear mass corrections are intentionally exercised by the
    // ordinary constructor and can legitimately change the closure remainder
    // relative to the published tank-capacity case.
    let mut report =
        crate::full_analysis::FullAnalysis::new_reference_compatibility(config.clone())
            .run(&preset.design_vector, true)
            .expect("A320 full analysis");
    // Construct a tank-limited mass budget independently of preset calibration:
    // preserve dry mass and allow 1,000 kg more fuel than the usable tank volume.
    let capacity = assess_fuel_capacity(&config, &preset.design_vector, &report)
        .capacity_kg
        .expect("A320 usable capacity");
    let closure = capacity + 1_000.0;
    let previous = report
        .component_masses
        .insert(FUEL.to_owned(), closure)
        .expect("analysis fuel mass");
    config.requirements.mtow_kg += closure - previous;
    let fuel_loading = plan_fuel_loading(&config, &preset.design_vector, &report);

    assert!(fuel_loading.mtow_closure_fuel_kg > fuel_loading.analyzed_carried_fuel_kg);
    let assessment = model_cg_assessment(&config, &report, &fuel_loading)
        .expect("fuel-capped model CG assessment");
    let takeoff = assessment
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
        .expect("analyzed takeoff state");

    assert_eq!(takeoff.state.label(), "analyzed TOW");
    assert!((takeoff.mass_kg - fuel_loading.analyzed_takeoff_mass_kg).abs() < 1.0e-9);
    assert!(takeoff.mass_kg < config.requirements.mtow_kg);
}
