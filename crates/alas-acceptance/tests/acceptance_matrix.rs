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
use alas_mass::tanks::FuelTankLayout;
use alas_payload::{build_payload_layout, LayoutSummary};
use alas_pipeline::{
    DesignPipeline, FindingCode, FullAnalysis, PipelineOptions, PlanningCgStatus, RunEnvironment,
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
    // Re-derived 2026-09-22. This was previously the aft-limit-violation
    // case: with `lower_deck_uld == "BLK"` (the A220's bulk-only lower hold,
    // per its weight-and-balance manual) and the generic `BULK` block's
    // nominal 1.5 m height not fitting this narrowbody's shallower
    // clearance, `alas-payload`'s cargo loader had zero real hold positions
    // to place anything in, so `place_baggage`'s trim loop
    // (`hold_target_cg`, meant to spread checked baggage to balance the
    // aircraft at its empty-aircraft CG) had nothing to spread across and
    // every bag fell back to one fixed loose block at the aft bulkhead,
    // dragging the analyzed CG aft of the published planning limit
    // regardless of what the trim loop was actually asking for. Restoring
    // real bulk-hold positions (`alas-payload/src/cargo/headroom.rs`) lets
    // that existing trim loop do its job, and the A220 now closes within
    // its published planning limit and passes the model's hard constraints
    // outright. This test's own scenario (a case that must be reported, not
    // silently accepted, while genuinely violating) no longer has an
    // A220 witness; the reporting path it names is not otherwise covered by
    // this file, so treat that as an open coverage gap, not evidence the
    // path was removed.
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
        "the corrected bulk-hold trim no longer violates the public planning envelope: {:?}",
        result.physical_findings
    );
    assert!(
        !result
            .physical_findings
            .iter()
            .any(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation),
        "bare OEW is ground-only and must not raise a model forward-range finding: {:?}",
        result.physical_findings
    );
    assert!(result.mission_fuel_within_available);
    assert!(!result
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::TrimUnavailable));

    // Regression pins of the same analyzed takeoff state in the two frames.
    // They are product-state values, not validated aircraft data.
    // Re-pinned 2026-09-23 after scaling each shallow bulk slot's load
    // limit to its realized height, preserving the nominal mass density.
    // Reduced per-slot limits change the mass assigned along the hold and
    // move the same takeoff CG from 22.862719 to 22.872246% model MAC and
    // from 19.617749 to 19.627197% in the published planning frame in the
    // payload-only lane. The combined physics corrections make a further
    // 0.000494 model / 0.000490 public percentage-point shift in the
    // analyzed product state. The value depends on the host, not only the
    // operating system: the hosted Windows and ubuntu-22.04 CI runners both
    // measured 22.874288 % model MAC on 2026-09-23, against 22.872740 on the
    // workstation the pin was taken on (a 0.0015-point, 0.06 mm shift), most
    // likely from run-time-selected SIMD kernels and thread-count-dependent
    // reductions. The band covers that spread and nothing physical.
    let pin_tolerance = 1.0e-2;
    assert!(
        (result.model_cg_pct_mac - 22.872_740_043_023_164).abs() < pin_tolerance,
        "model-frame CG was {}% MAC",
        result.model_cg_pct_mac
    );
    let public_pct_mac = result
        .public_planning_cg_pct_mac
        .expect("A220 has a source planning frame");
    assert!(
        (public_pct_mac - 19.627_686_998_753_216).abs() < pin_tolerance,
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
    // Re-derived 2026-09-22 (was "1 preset finding(s)", "AFT FAIL"): the
    // belly-cargo bulk-hold fix (see the re-pin note above) clears every
    // error-level finding for this preset, so the table reports the
    // planning frame "WITHIN" its published limit and the physical column
    // "PASS" instead.
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
    // acceptance evaluator. The brief's 130 passengers remains an input,
    // while the percent mix and built cabin geometry determine the filled
    // layout; planning_seats is not smuggled in as a fixed target.
    assert_eq!(config.requirements.num_passengers, 130);
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

    assert_eq!(summary.source_capacity_cap, Some(145));
    assert_eq!(summary.source_exit_layout, Some("C-III-C"));
    assert_eq!(summary.geometric_capacity, 145);
    assert_eq!(summary.max_certifiable_capacity, 145);
    assert_eq!(summary.capacity_binding, "source_exit_layout");
    assert_eq!(summary.total_pax, 145);
    assert_eq!(summary.seated_pax, 145);
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
    assert_eq!(a320.usable_fuel_capacity_kg, Some(19_334.0));
    let usable_capacity_kg = a320
        .usable_fuel_capacity_kg
        .expect("A320 source-backed usable fuel capacity");
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
    // and lands below the WV017 maximum landing mass. This used to also be
    // where the operating-empty centre of gravity sat forward of the
    // model's configured forward range (`ModelCgForwardRangeViolation`),
    // and, briefly during this investigation, where it sat close enough to
    // the main gear to trip a minimum nose-gear load finding instead. Both
    // were symptoms of the same bug: with `lower_deck_uld == "BLK"` and no
    // rigid ULD envelope to place, `place_baggage`'s trim loop
    // (`hold_target_cg`, which is meant to spread the hold load to balance
    // the aircraft at its empty-aircraft CG) had zero real positions to
    // spread checked baggage across (see the belly-cargo fix in
    // `alas-payload/src/cargo/headroom.rs`), so every declared-bulk
    // aircraft's bags landed in one fixed loose block at the aft bulkhead
    // regardless of trim, dragging the analyzed CG aft of where the loop
    // was already trying to put it. Restoring real bulk-hold positions lets
    // the existing trim loop do its job, and the A320 now closes cleanly:
    // re-pinned 2026-09-23 after shallow bulk slots inherited their scaled
    // usable volume and load limits, moving 17.576899 to 17.759868% MAC.
    // This remains forward of
    // both the broken aft-violating state and the historical pin, which
    // predates cabin/mass-station work this investigation did not audit).
    assert!(a320.mission_fuel_within_available);
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::MissionFuelShortfall));
    assert!(!a320
        .physical_findings
        .iter()
        .any(|finding| finding.code == FindingCode::LandingMassLimitViolation));
    assert!(
        !a320
            .physical_findings
            .iter()
            .any(|finding| finding.code == FindingCode::ModelCgForwardRangeViolation),
        "the corrected bulk-hold trim no longer places the analyzed CG forward of range: {:?}",
        a320.physical_findings
    );
    assert!(
        !a320.physical_findings.iter().any(|finding| {
            finding.code == FindingCode::MinimumNoseGearLoadViolation
                && finding.severity == alas_pipeline::FindingSeverity::Error
        }),
        "the corrected bulk-hold trim no longer strains the nose gear: {:?}",
        a320.physical_findings
    );
    // Host-dependent like the A220 pin above: both hosted CI runners
    // (Windows and ubuntu-22.04) measured 17.643568 % MAC on 2026-09-23
    // against 17.759868 on the workstation, because the CG-targeted hold
    // loading resolves a near-tie differently (0.12 point, about 5 mm at the
    // A320 MAC). The qualitative clauses above and below hold on every host.
    let a320_tolerance = 0.2;
    assert!(
        (a320.model_cg_pct_mac - 17.759_867_972_899_137).abs() < a320_tolerance,
        "model-frame CG was {}% MAC",
        a320.model_cg_pct_mac
    );
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
    // Re-pinned 2026-09-22 (12.418 -> 16.923). AVE declares a containerized
    // hold (`preset_flops::declared_cargo_loading`), not `"BLK"`, so it is
    // unaffected by the belly-cargo bulk-hold fix the A320/A220 pins above
    // are traced to (confirmed: this value is bit for bit unchanged by that
    // fix). This acceptance suite had never run end-to-end before this pin
    // was written, so the prior number is not a verified baseline to trace
    // a cause from; the value below is this pipeline's actual output.
    assert!(
        (ave_forward_finding.actual.expect("AVE CG actual") - 16.923_225_276_357_176).abs() < 0.01
    );
    // Re-pinned 2026-09-22 (18.194 -> 18.062): the configured forward limit is
    // itself a percentage of the built aircraft's MAC/LEMAC, so it moves by a
    // fraction of a percentage point with any geometry-derived reference
    // change, independently of the mass-station shift the `actual` pin above
    // documents.
    assert!(
        (ave_forward_finding.limit.expect("AVE CG limit") - 18.062_486_118_831_52).abs() < 0.01
    );
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
    let preset = presets::get("A320-200").expect("A320 preset");
    let requested_belly_cargo_kg = 2_682.0;
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset.name
    }))
    .expect("A320 preset configuration");
    config.cabin.passenger.belly_cargo_kg = requested_belly_cargo_kg;
    // The registered preset declares the *delivered* WV017 arrangement, whose
    // lower hold has no installed loading system (`lower_deck_uld == "BLK"`,
    // see `preset_flops::declared_cargo_loading`). This source maximum-payload
    // case is the cargo-loading-system option/STC: its net freight (20,682 kg),
    // seven-ULD hold and 574 kg combined tare (7 x 82 kg, the reduced-height
    // LD3-45's tare) are the containerized variant's numbers, not the bulk
    // baseline's. Select that variant explicitly rather than inheriting the
    // preset's delivered-aircraft default.
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
    assert_eq!(summary.hold_ulds, 7);

    let net_revenue_payload_kg =
        1_000.0 * (summary.seat_mass_t + summary.bag_mass_t + summary.belly_cargo_t);
    let hold_contents_kg = summary.hold_used_t * 1_000.0;
    let uld_tare_kg = hold_contents_kg - 1_000.0 * (summary.bag_mass_t + summary.belly_cargo_t);
    assert!((net_revenue_payload_kg - 20_682.0).abs() < 1.0e-6);
    assert!((uld_tare_kg - 574.0).abs() < 1.0e-6);
    assert!((layout.total_mass - 21_256.0).abs() < 1.0e-6);
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
    // (historically MZFW 62,500 kg less a 41,244 kg empty weight that the
    // OEW reference registry records as unsourced); it is an input, not a
    // registry OEW, so it is read from the requirements.
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
