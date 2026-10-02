// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Acceptance matrix tests verifying end-to-end multi-preset execution,
//! disciplinary consistency, figure generation, solver degradation, and CLI integration.

// Test suite uses unwraps and assertions to validate expectations.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_acceptance::matrix::{
    evaluate_preset, format_matrix_json, format_matrix_report, generate_scenes_for_preset,
    run_acceptance_matrix, AcceptanceMatrixReport, PresetAcceptanceResult,
    PresetDesignMissionStatus,
};
use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::tanks::FuelTankLayout;
use alas_payload::{build_payload_layout, LayoutSummary};
use alas_pipeline::{
    DesignPipeline, FindingCode, FullAnalysis, PipelineOptions, PlanningCgStatus, RunEnvironment,
};
use alas_report::render_svg;
use std::sync::OnceLock;

/// The A380 evaluation, shared by the two tests that read it: it is the
/// slowest preset, and `evaluate_preset` is deterministic in its name.
fn a380() -> &'static PresetAcceptanceResult {
    static A380: OnceLock<PresetAcceptanceResult> = OnceLock::new();
    A380.get_or_init(|| evaluate_preset("A380-800").expect("A380 evaluation"))
}

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
    // With `lower_deck_uld == "BLK"` (the A220's bulk-only lower hold, per its
    // weight-and-balance manual) `alas-payload`'s cargo loader places checked
    // baggage in real bulk-hold positions (`alas-payload/src/cargo/headroom.rs`),
    // so `place_baggage`'s trim loop can balance the aircraft at its
    // empty-aircraft CG and the A220 closes within its published planning
    // limit; the public-frame planning check below
    // (`public_planning_cg_status`) stays clean. Tail scrape (6.66 deg here)
    // is a diagnostic warning until the preset aft-fuselage contour is
    // validated (`ModelCgConstraint::is_diagnostic`), so the A220 model
    // envelope passes.
    assert!(result.model_cg_envelope_ok);
    assert_eq!(
        result.public_planning_cg_status,
        PlanningCgStatus::WithinPublishedLimits
    );
    assert!(result.physical_passed);
    assert!(
        !result
            .physical_findings
            .iter()
            .any(|finding| finding.code == FindingCode::PublicPlanningCgEnvelopeViolation),
        "the bulk-hold trim keeps the loaded CG inside the public planning envelope: {:?}",
        result.physical_findings
    );
    // No Error-severity finding: tail scrape is a warning (see above), and
    // no nose-load shortfall or bare-OEW forward-range error is raised.
    assert!(
        !result
            .physical_findings
            .iter()
            .any(|finding| finding.severity == alas_pipeline::FindingSeverity::Error),
        "{:?}",
        result.physical_findings
    );
    // The remaining warnings are the operational-envelope findings (usable
    // CG-range shortfall and a potato-boundary excursion), not a bare-OEW
    // forward-range error.
    assert!(result
        .physical_findings
        .iter()
        .filter(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation)
        .all(|finding| !finding.message.contains("OEW")));
    assert!(result.mission_fuel_within_available);
    assert!(!result
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::TrimUnavailable));

    // The analyzed takeoff CG in the two frames is checked against the
    // published flight envelope at the analyzed takeoff mass (A220-300
    // Aircraft Recovery Publication BD500-3AB48-10400-00, p. 119: forward
    // 12.0 %MAC, aft 31.0 %MAC at 36,287 kg rising to 37.3 %MAC at 54,431 kg),
    // not against a pin: the loaded CG moves with every mass-model change, the
    // published limits do not. The model frame is compared unshifted; its MAC
    // differs from the manufacturer's, so this is a plausibility bound on the
    // frame, while the public frame is the conformance check.
    let public_pct_mac = result
        .public_planning_cg_pct_mac
        .expect("A220 has a source planning frame");
    let published = presets::get("A220-300")
        .expect("registered A220")
        .reference
        .planning_cg_envelope
        .expect("A220 planning envelope");
    let limits = published
        .limits_at(
            alas_config::CgEnvelopeCondition::Flight,
            result.analyzed_takeoff_mass_kg,
        )
        .expect("the analyzed takeoff mass lies inside the published mass range");
    let aft = limits
        .aft_pct_mac
        .expect("published aft limit at takeoff mass");
    for (frame, value) in [
        ("public", public_pct_mac),
        ("model", result.model_cg_pct_mac),
    ] {
        assert!(
            value >= limits.forward_pct_mac && value <= aft,
            "{frame}-frame CG {value} %MAC outside the published [{}, {aft}] %MAC",
            limits.forward_pct_mac
        );
    }
    // One mechanism guard: the model frame reads the detailed item ledger,
    // which places this loading state further aft than the lumped-station
    // model the public frame sums (the `MassModelDisagreement` finding). The
    // offset is 4.6 %MAC today; the band is wide enough for mass-model changes
    // and fails if the ledger preference or the frame mapping is lost.
    let ledger_offset = result.model_cg_pct_mac - public_pct_mac;
    assert!(
        (2.0..8.0).contains(&ledger_offset),
        "ledger-minus-lumped CG offset {ledger_offset} %MAC"
    );

    // The public frame is the manufacturer's LEMAC and MAC from the
    // recovery publication; the model frame is `Airplane::mac_frame()` (the
    // main wing's own `mac_station().x_le_m`, resolved once, never
    // reconstructed from `c_ref`: `c_ref` is the projected-planform MAC while
    // `aerodynamic_center` integrates the unfolded span).
    //
    // `public_pct_mac` and `model_cg_pct_mac` do not name the same physical
    // station: the model-CG gate prefers the detailed item ledger for
    // `model_cg_pct_mac` whenever it builds, while the public-planning
    // conversion (`alas-pipeline/src/feasibility/planning.rs`) sums the lumped
    // per-component stations -- the basis divergence the
    // `MassModelDisagreement` finding reports. So this check independently
    // reconstructs the *lumped* station the public frame is built from (the
    // same component list and analyzed carried fuel
    // `planning.rs::analyzed_mass_and_cg_pct_mac` sums), and confirms that
    // maps to `public_pct_mac` through the manufacturer LEMAC/MAC, i.e. the
    // frame mapping itself, independent of the ledger-vs-lumped basis.
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
    let mac_frame = airplane.mac_frame().expect("A220 has a main wing");
    let x_mac_le_m = mac_frame.x_lemac_m;
    let lumped_full_report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .expect("A220 lumped-basis analysis");
    let lumped_names = [
        alas_mass::breakdown::WING,
        alas_mass::breakdown::H_STAB,
        alas_mass::breakdown::V_STAB,
        alas_mass::breakdown::FUSELAGE,
        alas_mass::breakdown::GEAR,
        alas_mass::breakdown::PROPULSION,
        alas_mass::breakdown::SYSTEMS,
        alas_mass::breakdown::FURNISHINGS,
        alas_mass::breakdown::PAYLOAD,
    ];
    let mut lumped_mass_kg = result.analyzed_carried_fuel_kg;
    let mut lumped_moment_kg_m = result.analyzed_carried_fuel_kg
        * lumped_full_report.mass_coordinates[alas_mass::breakdown::FUEL][0];
    for name in lumped_names {
        let mass = lumped_full_report.component_masses[name].max(0.0);
        lumped_mass_kg += mass;
        lumped_moment_kg_m += mass * lumped_full_report.mass_coordinates[name][0];
    }
    let x_lumped_m = lumped_moment_kg_m / lumped_mass_kg;
    assert!(
        (x_public_m - x_lumped_m).abs() < 1.0e-6,
        "public frame station {x_public_m} m and independently-reconstructed lumped station {x_lumped_m} m differ"
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
    // The bulk-hold trim clears every error-level finding for this preset
    // (tail scrape is a warning), so the planning frame reports "WITHIN" its
    // published limit and the physical column "PASS".
    assert!(text.contains("Physical Verdict: 0 preset finding(s) require investigation"));
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
fn a220_full_analysis_uses_registered_percent_capacity_before_mass_build() {
    let preset = presets::get("A220-300").expect("registered A220 preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .expect("A220 preset configuration");

    // This is the same registered configuration/design path used by the
    // acceptance evaluator. The brief's 140 passengers is the manufacturer's
    // typical cabin (`planning_seats`); in a fixed-aircraft design basis it is
    // also the ceiling on the percent-mode layout, below the 145-seat exit
    // limit, so the analyzed aircraft carries the seats its reference
    // operating-empty mass belongs to.
    assert_eq!(config.requirements.num_passengers, 140);
    assert_eq!(config.cabin.passenger.class_mix_mode, "percent");
    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, false)
        .expect("A220 full analysis");
    let layout = report
        .payload_layout
        .as_ref()
        .expect("A220 full analysis payload layout");
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("A220 full analysis must use passenger layout");
    };

    assert_eq!(summary.source_capacity_cap, Some(140));
    assert_eq!(summary.source_exit_layout, Some("C-III-C"));
    assert_eq!(summary.geometric_capacity, 145);
    assert_eq!(summary.max_certifiable_capacity, 140);
    assert_eq!(summary.capacity_binding, "source_exit_layout");
    assert_eq!(summary.total_pax, 140);
    assert_eq!(summary.seated_pax, 140);
    assert_eq!(summary.unseated_pax, 0);
    let row_seats: i64 = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            alas_payload::ItemMeta::Seat(meta) => Some(meta.filled),
            _ => None,
        })
        .sum();
    assert_eq!(row_seats, summary.seated_pax);
}

#[test]
fn a380_soft_static_margin_target_does_not_become_a_hard_model_constraint() {
    let result = a380();

    assert!(result.execution_passed);
    assert!(result.model_cg_minimum_loading_static_margin >= result.model_cg_static_margin_floor);
    // The corrected A380 now clears its default preference. Raise only the
    // test preference above the measured takeoff margin to exercise a
    // shortfall without changing the geometry, loading or hard floor.
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" })).expect("A380 config");
    config.requirements.target_static_margin =
        result.model_cg_analyzed_takeoff_static_margin + result.model_cg_static_margin_floor;
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
    let raised_preference = DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .expect("A380 with raised soft preference");
    let assessment = raised_preference
        .feasibility
        .model_cg
        .as_ref()
        .expect("A380 model CG assessment");
    assert!(!assessment.target_static_margin.met_or_exceeded());
    assert_eq!(
        assessment.minimum_physical_static_margin,
        result.model_cg_static_margin_floor
    );
    assert_eq!(
        assessment.target_static_margin.actual,
        result.model_cg_analyzed_takeoff_static_margin
    );
    let stability_constraints: Vec<_> = assessment
        .loading_states
        .iter()
        .flat_map(|state| &state.constraints)
        .filter(|constraint| {
            constraint.constraint == alas_opt::ModelCgConstraint::StaticStabilityFloor
        })
        .collect();
    assert!(!stability_constraints.is_empty());
    for constraint in stability_constraints {
        assert_eq!(constraint.limit, result.model_cg_static_margin_floor);
        assert!(
            !constraint.violated,
            "soft preference changed {constraint:?}"
        );
    }
    assert!(!raised_preference
        .feasibility
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::InsufficientStaticMargin));
    // The drawn planform/engine stations and separate ground/flight wing
    // shapes changed the mass ledger and CG. The analyzed zero-fuel state
    // now lies aft of the minimum-nose-load boundary derived from Airbus
    // AC Rev 20 Figure 7-3-0-991-006-A01 (WV000, MRW):
    // 1 - (2 * 106920 + 2 * 160380) / 562000. This is a real model ground
    // constraint, independent of the soft static-margin target; retaining
    // the old whole-envelope PASS would hide it.
    assert!(
        !result.model_cg_envelope_ok,
        "the zero-fuel nose-load shortfall must remain visible: {:?}",
        result.physical_findings
    );
    let nose_load = result
        .physical_findings
        .iter()
        .find(|finding| finding.code == FindingCode::MinimumNoseGearLoadViolation)
        .expect("the zero-fuel state violates the sourced minimum nose load");
    assert_eq!(nose_load.severity, alas_pipeline::FindingSeverity::Error);
    assert!(nose_load.message.contains("analyzed ZFW"));
    assert!(
        nose_load.actual.expect("measured nose-load fraction")
            < nose_load.limit.expect("sourced minimum nose-load fraction")
    );
    // The unvalidated aft-fuselage contour still makes tail scrape a
    // diagnostic warning (`ModelCgConstraint::is_diagnostic`).
    assert!(result
        .physical_findings
        .iter()
        .any(
            |finding| finding.severity == alas_pipeline::FindingSeverity::Warning
                && finding.message.contains("tail-scrape")
        ));
    assert!(!result.physical_findings.iter().any(|finding| matches!(
        finding.code,
        FindingCode::InsufficientStaticMargin
            | FindingCode::NoseGearStrengthViolation
            | FindingCode::MainGearStrengthViolation
    )));
    // The same zero-fuel point starts the fuel vector, so its aft excursion
    // appears there as well as on the loading potato. These and the usable
    // CG-range shortfall remain warnings, not hard static-margin findings.
    let forward_findings: Vec<_> = result
        .physical_findings
        .iter()
        .filter(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation)
        .collect();
    assert!(forward_findings
        .iter()
        .all(|finding| finding.severity == alas_pipeline::FindingSeverity::Warning));
    for mechanism in ["usable CG range", "potato extreme", "fuel-vector"] {
        assert!(
            forward_findings
                .iter()
                .any(|finding| finding.message.contains(mechanism)),
            "missing {mechanism} diagnostic: {forward_findings:?}"
        );
    }

    let matrix = alas_acceptance::matrix::AcceptanceMatrixReport {
        presets: vec![result.clone()],
        all_executed: true,
        all_passed: false,
        all_physical_passed: result.physical_passed,
        all_design_missions_verified: false,
    };
    let text = format_matrix_report(&matrix);
    assert!(text.contains("A380-800: hard constraints FAIL"));
    assert!(text.contains("hard floor 5.000%"));
    assert!(text.contains("target preference 10.000%"));
    assert!(!text.to_ascii_lowercase().contains("certif"));
}

#[test]
fn acceptance_narrowbody_and_widebody_mass_calibrations() {
    // Independent presets, evaluated concurrently rather than one after
    // another; the A380 comes from the shared evaluation.
    let (a320, ave) = std::thread::scope(|scope| {
        let a320 = scope.spawn(|| evaluate_preset("A320-200").expect("A320 evaluation"));
        let ave = scope.spawn(|| evaluate_preset("AVE").expect("AVE evaluation"));
        let shared = scope.spawn(a380);
        shared.join().expect("A380 evaluation thread");
        (
            a320.join().expect("A320 evaluation thread"),
            ave.join().expect("AVE evaluation thread"),
        )
    });
    let a380 = a380();

    // A320 narrowbody MTOW is around 70-80 tonnes
    assert!(
        a320.mtow_kg > 60_000.0 && a320.mtow_kg < 90_000.0,
        "A320 MTOW in expected range: {}",
        a320.mtow_kg
    );
    // The closure field is the usable-fuel remainder after the resolved
    // unusable tank inventory is reserved.  It is load-dependent, so the
    // source maximum-payload value is covered by the dedicated explicit-load
    // test below rather than by this ordinary zero-belly baseline.  Keep this
    // acceptance case focused on finite mass correlation and the separate
    // fuel-capacity/load identities.
    assert!(a320.oew_kg.is_finite() && a320.oew_kg > 0.0);
    assert!(a320.payload_kg.is_finite() && a320.payload_kg > 0.0);
    assert!(a320.mtow_closure_fuel_kg.is_finite() && a320.mtow_closure_fuel_kg > 0.0);
    // The interactive route (LEMD-LEPA) is flown at the takeoff mass the
    // EASA basic fuel scheme requires, closed by re-flying the native
    // mission until the mass settles.  The route result is a policy/load
    // diagnostic rather than an independently sourced operational flight
    // plan, so retain the source-backed capacity and test the resulting
    // contract instead of pinning one product-state fuel value.
    // The inventory is the published 24,167 L at 0.8 kg/L; the published
    // 19,334 kg is that figure rounded to the kilogram at source.
    let usable_capacity_kg = a320
        .usable_fuel_capacity_kg
        .expect("A320 source-backed usable fuel capacity");
    assert!((usable_capacity_kg - 19_334.0).abs() <= 0.5);
    assert!(a320.analyzed_carried_fuel_kg.is_finite() && a320.analyzed_carried_fuel_kg > 0.0);
    assert!(a320.analyzed_carried_fuel_kg <= usable_capacity_kg + 1.0e-6);
    assert!(a320.analyzed_carried_fuel_kg <= a320.mtow_closure_fuel_kg + 1.0e-6);
    let a320_zero_fuel_mass_kg = a320.mtow_kg - a320.mtow_closure_fuel_kg;
    assert!(a320_zero_fuel_mass_kg.is_finite() && a320_zero_fuel_mass_kg > 0.0);
    assert!(a320.analyzed_takeoff_mass_kg.is_finite() && a320.analyzed_takeoff_mass_kg > 0.0);
    assert!(
        (a320.analyzed_takeoff_mass_kg - (a320_zero_fuel_mass_kg + a320.analyzed_carried_fuel_kg))
            .abs()
            < 1.0e-6
    );
    assert!(a320.analyzed_takeoff_mass_kg <= a320.mtow_kg + 1.0e-6);
    // This field describes the maximum takeoff-mass bound imposed by the
    // usable tanks, independently of the lower policy takeoff mass actually
    // flown on the route.
    let expected_mtow_shortfall_kg = (a320.mtow_closure_fuel_kg - usable_capacity_kg).max(0.0);
    assert!(
        (a320.mtow_shortfall_kg - expected_mtow_shortfall_kg).abs() < 1.0e-6,
        "A320 MTOW shortfall must close the analyzed load: {} kg vs {} kg",
        a320.mtow_shortfall_kg,
        expected_mtow_shortfall_kg
    );
    // At the policy takeoff mass the route completes within the loaded fuel
    // and lands below the WV017 maximum landing mass. With
    // `lower_deck_uld == "BLK"`, `place_baggage`'s trim loop (`hold_target_cg`,
    // which spreads the hold load to balance the aircraft at its
    // empty-aircraft CG) has real bulk-hold positions to spread checked
    // baggage across (`alas-payload/src/cargo/headroom.rs`; shallow bulk
    // slots inherit their scaled usable volume and load limits), so the
    // declared-bulk aircraft's bags are trimmed rather than landing in one
    // loose block at the aft bulkhead, and the A320 closes cleanly.
    assert!(a320.mission_fuel_within_available);
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::MissionFuelShortfall));
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::LandingMassLimitViolation));
    // What else this default A320 loading state raises:
    // 1. The A320 preset's `root_datum_x_m` is anchored at 11.891 m to match
    //    the Airbus LEMAC, so every %MAC value for a given physical CG
    //    station is larger by about a quarter of the 4.196 m MAC than under
    //    an aft datum.
    // 2. The model-CG gate prefers the detailed item ledger over the
    //    lumped-station model whenever it builds, which for this state is a
    //    further +3.7 %MAC (see the `MassModelDisagreement` finding).
    // 3. The tip-back/tail-scrape constraints are geometric
    //    ground-clearance-at-rotation checks (`min_tip_back_deg` default
    //    15 deg, `required_rotation_angle_deg` default 10 deg).
    // 4. The fuel-vector/potato operational-envelope checks raise two
    //    `Warning`-severity `ModelCgForwardRangeViolation` findings (a
    //    usable-range shortfall and a potato/fuel-vector excursion).
    let a320_errors: Vec<_> = a320
        .physical_findings
        .iter()
        .filter(|finding| finding.severity == alas_pipeline::FindingSeverity::Error)
        .collect();
    // The preset carries its sourced fuselage ground clearance (Airbus AC
    // Jun 01/24 Figure 2-3-0-991-004-A01: 1.79 m,
    // `fuselage_ground_clearance_m`) and anchored gear stations, so the
    // tip-back angle clears its 15 deg limit and no hard error remains. The
    // structural stage contributes none either: the wing box meshes and
    // passes its native strength, rib and packaging checks (an unconfigured
    // MSC Nastran is a warning, not a veto).
    assert!(
        a320_errors.is_empty(),
        "unexpected error findings: {a320_errors:?}"
    );
    // Tail scrape (8.19 deg vs 10 deg) is a warning
    // (`ModelCgConstraint::is_diagnostic`). The `ModelCgForwardRangeViolation`
    // warnings are the usable CG-range shortfall and the potato boundary
    // excursion. The full-fuel point of the fuel vector clears the aft limit
    // with the 150-seat planning cabin.
    assert_eq!(
        a320.physical_findings
            .iter()
            .filter(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation)
            .count(),
        2,
        "{:?}",
        a320.physical_findings
    );
    // Ledger-preferred model-frame CG (see the wing-datum and ledger
    // discussion above), checked against the published A320 limits rather than
    // a pin: 17 %MAC forward (Airbus A320 Aircraft Characteristics for Airport
    // Planning, Fig. 7-3-0-991-010) and 40 %MAC aft (the same document at MRW
    // with 6.0 % nose load). Both are the ACAP CG used in the pavement-load
    // analysis at MRW, not certified limits, so they bound the loaded CG
    // loosely; the model frame is compared unshifted.
    assert!(
        (17.0..=40.0).contains(&a320.model_cg_pct_mac),
        "model-frame CG {}% MAC outside the published ACAP [17, 40] %MAC",
        a320.model_cg_pct_mac
    );
    // With tip-back cleared and only warnings left, the default A320 loading
    // state clears every physical check.
    assert!(a320.physical_passed);
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
    // With the 150-seat planning cabin the payload is 30 passengers lighter, so
    // the MTOW closure fuel exceeds the usable tanks: the report lists the load
    // as tank-limited instead of "None". That is the shortfall identity above,
    // read through the report, not a new limit.
    assert!(
        expected_mtow_shortfall_kg > 0.0,
        "closure fuel {} kg vs usable {usable_capacity_kg} kg",
        a320.mtow_closure_fuel_kg
    );
    assert!(a320_text.contains("- A320-200: the tanks admit a takeoff mass of"));
    assert!(!a320_text.contains("- None"));
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
}

#[test]
fn ave_usable_cg_range_and_tail_scrape_are_warnings() {
    let ave = evaluate_preset("AVE").expect("AVE evaluation");
    // Each state's forward limit is the most aft of the boundaries its own
    // phase admits; AVE reports the usable-range shortfall as a warning, and
    // tail scrape is a diagnostic warning as well.
    let ave_forward_finding = ave
        .physical_findings
        .iter()
        .find(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation)
        .expect("AVE still reports a usable-CG-range finding");
    assert_eq!(
        ave_forward_finding.severity,
        alas_pipeline::FindingSeverity::Warning
    );
    // The usable range is the aft ground limit (minimum nose load) minus the
    // rotation forward limit at the analyzed takeoff state (stall-branch V_R,
    // V_R/V_S = 1.10, thrust and rolling-friction term), so it moves with the
    // mass model and the rotation physics and is not pinned. The physical
    // property asserted: the range is positive (the limits do not cross) and
    // it is reported as a warning because it falls short of the configured
    // 30 %MAC minimum.
    let ave_usable_range = ave_forward_finding.actual.expect("AVE CG range actual");
    let ave_range_limit = ave_forward_finding.limit.expect("AVE CG range limit");
    assert!(
        ave_usable_range > 0.0 && ave_usable_range < ave_range_limit,
        "AVE usable CG range {ave_usable_range} %MAC against the {ave_range_limit} %MAC minimum"
    );
    assert!((ave_range_limit - 30.0).abs() < 0.01);
    // AVE has no Error finding: tail scrape is a warning
    // (`ModelCgConstraint::is_diagnostic`).
    assert!(!ave
        .physical_findings
        .iter()
        .any(|finding| finding.severity == alas_pipeline::FindingSeverity::Error));
    let ave_scrape = ave
        .physical_findings
        .iter()
        .find(|finding| finding.message.contains("tail-scrape"))
        .expect("AVE still reports its tail-scrape shortfall as a warning");
    assert_eq!(ave_scrape.severity, alas_pipeline::FindingSeverity::Warning);
    // The ground plane hangs below the lowest belly point, 0.70 m above the
    // nose-tip-centreline datum used before (0.7 m nose droop on AVE), so the
    // tail sits closer to the runway: 6.79 -> 5.18 deg.
    assert!(
        (ave_scrape.actual.expect("AVE tail-scrape actual") - 5.175_316_341_745_55).abs() < 0.01
    );
    assert!((ave_scrape.limit.expect("AVE tail-scrape limit") - 10.0).abs() < 0.01);
    assert_eq!(ave_forward_finding.unit, "% MAC");
    assert!(!ave
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::FuelCapacityUnavailable));
}

#[test]
fn a320_source_max_payload_case_separates_net_tare_gross_and_usable_fuel() {
    // The Airbus source case is MZFW - OEW = 21,256 kg gross payload.  The
    // ordinary registered-preset path deliberately leaves revenue belly cargo
    // at zero; this test names the source maximum-payload load explicitly so
    // its net freight, ULD tare, gross payload and fuel basis cannot be
    // mistaken for that ordinary product load.
    //
    // The source maximum-payload load is the 180-seat certified cabin, not the
    // 150-seat planning cabin the fixed-aircraft basis caps the registered
    // preset at. The clean-sheet basis applies the certified exit limit only,
    // so this case names its cabin explicitly.
    let preset = presets::get("A320-200").expect("A320 preset");
    let requested_belly_cargo_kg = 2_682.0;
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset.name
    }))
    .expect("A320 preset configuration");
    config.cabin.passenger.belly_cargo_kg = requested_belly_cargo_kg;
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::CleanSheet;
    // The registered preset declares the *delivered* WV017 arrangement, whose
    // lower hold has no installed loading system (`lower_deck_uld == "BLK"`,
    // see `preset_flops::declared_cargo_loading`). This source maximum-payload
    // case is the cargo-loading-system option/STC: its net freight (20,682 kg)
    // and a six-ULD hold with 492 kg combined tare (6 x 82 kg, the
    // reduced-height LD3-45's tare) are the containerized variant's numbers,
    // not the bulk baseline's. Select that variant explicitly rather than
    // inheriting the preset's delivered-aircraft default.
    // With the A320 wing datum anchored to the Airbus LEMAC
    // (`root_datum_x_m` 11.891 m), the lower forward hold, bounded by
    // `wing_box_x_range()`, has room for six LD3-45 rows: the net
    // freight/bag/belly-cargo demand (20,682 kg) is carried in six containers
    // with 492 kg combined tare, so total gross payload is 21,174 kg.
    config.cabin.cargo.lower_deck_uld = "LD3-45".to_owned();

    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A320 source case geometry");
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0)
        .expect("A320 source maximum-payload layout");
    let summary = match &layout.summary {
        LayoutSummary::Passenger(summary) => summary,
        LayoutSummary::Cargo(_) => panic!("A320 source case must use passenger layout"),
    };

    assert_eq!(summary.total_pax, 180);
    assert_eq!(summary.seated_pax, summary.total_pax);
    assert_eq!(summary.unseated_pax, 0);
    assert!((summary.belly_cargo_t * 1_000.0 - requested_belly_cargo_kg).abs() < 1.0e-6);
    assert_eq!(summary.hold_ulds, 6);

    let net_revenue_payload_kg =
        1_000.0 * (summary.seat_mass_t + summary.bag_mass_t + summary.belly_cargo_t);
    let hold_contents_kg = summary.hold_used_t * 1_000.0;
    let uld_tare_kg = hold_contents_kg - 1_000.0 * (summary.bag_mass_t + summary.belly_cargo_t);
    assert!((net_revenue_payload_kg - 20_682.0).abs() < 1.0e-6);
    // 6 x 82 kg tare (see the comment above the `lower_deck_uld` assignment).
    assert!((uld_tare_kg - 492.0).abs() < 1.0e-6);
    assert!((layout.total_mass - 21_174.0).abs() < 1.0e-6);
    assert!((layout.total_mass - (net_revenue_payload_kg + uld_tare_kg)).abs() < 1.0e-6);
    assert!((summary.payload_t * 1_000.0 - layout.total_mass).abs() < 1.0e-6);

    // Run the actual product analysis for this load case.  The fuel closure
    // is intentionally reported in both bases: the model component is gross
    // tank fuel, while usable closure removes resolved unusable fuel from the
    // same tank geometry.  The identity is tested instead of treating one
    // basis as the other.
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .expect("A320 source maximum-payload analysis");
    let actual_layout = report
        .payload_layout
        .as_ref()
        .expect("A320 source case detailed product layout");
    let actual_summary = match &actual_layout.summary {
        LayoutSummary::Passenger(summary) => summary,
        LayoutSummary::Cargo(_) => panic!("A320 source case must retain passenger layout"),
    };
    let loaded_belly_cargo_kg = actual_summary.belly_cargo_t * 1_000.0;
    assert!(
        (loaded_belly_cargo_kg - requested_belly_cargo_kg).abs() < 1.0e-6,
        "A320 source case requested {requested_belly_cargo_kg} kg net belly cargo, loaded {loaded_belly_cargo_kg} kg"
    );
    let actual_hold_contents_kg = actual_summary.hold_used_t * 1_000.0;
    let actual_uld_tare_kg = actual_hold_contents_kg
        - 1_000.0 * (actual_summary.bag_mass_t + actual_summary.belly_cargo_t);
    // The declared structural payload is the 21,256 kg the preset carries
    // (the OEW reference registry records no sourced OEW); it is an input,
    // not a registry OEW, so it is read from the requirements.
    assert_eq!(preset.reference.oew_kg, None);
    let source_gross_payload_kg = config.requirements.max_structural_payload_kg;
    assert!((source_gross_payload_kg - 21_256.0).abs() < 1.0e-6);
    assert!(actual_layout.total_mass <= source_gross_payload_kg + 1.0e-6);
    // The product loader trims the requested net freight against the actual
    // target CG and may choose a different number of ULDs than the explicit
    // source-layout case above.  Keep the resulting source residual visible:
    // any gross-payload shortfall is exactly the difference in carried ULD
    // tare when net passenger, bag and belly masses are held fixed.
    let source_payload_residual_kg = source_gross_payload_kg - actual_layout.total_mass;
    assert!(source_payload_residual_kg.is_finite() && source_payload_residual_kg >= -1.0e-6);
    assert!((source_payload_residual_kg - (574.0 - actual_uld_tare_kg)).abs() < 1.0e-6);

    let gross_fuel_kg = *report
        .component_masses
        .get("Fuel")
        .expect("A320 source case gross fuel component");
    let modeled_mass_without_fuel_kg: f64 = report
        .component_masses
        .iter()
        .filter(|(name, _)| name.as_str() != "Fuel")
        .map(|(_, mass)| *mass)
        .sum();
    assert!(gross_fuel_kg.is_finite() && gross_fuel_kg > 0.0);
    assert!(
        (modeled_mass_without_fuel_kg + gross_fuel_kg - config.requirements.mtow_kg).abs() < 1.0e-6
    );

    let density_kg_m3 = preset
        .reference
        .fuel_density_kg_l
        .expect("A320 source fuel density")
        * 1_000.0;
    let tanks = FuelTankLayout::resolve(
        &report.airplane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        preset.reference.usable_fuel_volume_l,
    )
    .expect("A320 source case fuel tanks");
    let unusable_fuel_kg = tanks.unusable_fuel_kg();
    let usable_closure_fuel_kg = gross_fuel_kg - unusable_fuel_kg;
    assert!(gross_fuel_kg > usable_closure_fuel_kg);
    assert!(unusable_fuel_kg.is_finite() && unusable_fuel_kg > 0.0);
    assert!((usable_closure_fuel_kg - (gross_fuel_kg - unusable_fuel_kg)).abs() < 1.0e-9);
}

#[test]
fn a320_delivered_bulk_hold_carries_the_requested_belly_cargo() {
    // The delivered WV017 A320-200 (the registered preset's own default, no
    // override) has no installed cargo-loading system
    // (`lower_deck_uld == "BLK"`, `preset_flops::declared_cargo_loading`),
    // unlike the option/STC-equipped source case above. Loose bulk freight
    // has no rigid envelope of its own, so it is carried at the local hold
    // clearance rather than requiring the generic `BULK` type's oversized
    // 1.5 m nominal block to fit whole (`alas_payload::cargo::headroom`);
    // this proves the same 2,682 kg net revenue request the source case
    // above names is carried in this delivered, bulk-only arrangement too.
    let preset = presets::get("A320-200").expect("A320 preset");
    let requested_belly_cargo_kg = 2_682.0;
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset.name
    }))
    .expect("A320 preset configuration");
    config.cabin.passenger.belly_cargo_kg = requested_belly_cargo_kg;
    assert_eq!(config.cabin.cargo.lower_deck_uld, "BLK");

    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A320 delivered-arrangement geometry");
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0)
        .expect("A320 delivered-arrangement layout");
    let summary = match &layout.summary {
        LayoutSummary::Passenger(summary) => summary,
        LayoutSummary::Cargo(_) => panic!("A320 delivered arrangement must use passenger layout"),
    };
    assert!(
        (summary.belly_cargo_t * 1_000.0 - requested_belly_cargo_kg).abs() < 1.0e-6,
        "A320 delivered bulk hold carried {} kg of the requested {requested_belly_cargo_kg} kg",
        summary.belly_cargo_t * 1_000.0
    );
    // Loose bulk, not containers: no ULD count and no container tare.
    assert_eq!(summary.hold_ulds, 0);
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
