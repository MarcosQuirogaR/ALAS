// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The printed masses are the numerical evidence this test records.
#![allow(clippy::print_stderr)]

//! Quick Analysis payload contract on the default aircraft: every
//! payload-range corner respects the declared MTOW, the achievable payload
//! is bounded by `MTOW - OEW`, and an empty mass at or above MTOW fails the
//! diagram honestly instead of drawing it.

use std::sync::atomic::AtomicBool;

use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::full_analysis::{AnalysisReport, FullAnalysis};
use alas_pipeline::quick_analysis::{
    payload_range_corners, report_oew_kg, run_quick_analysis, sandbox_config,
    PayloadRangeUnavailable, QuickAnalysisRequest, QuickBasis, QuickMetric, QuickOutcome,
};

fn baseline_report(config: &AlasConfig) -> (AlasConfig, AnalysisReport) {
    let sandbox = sandbox_config(config);
    let report = FullAnalysis::new(sandbox.clone())
        .run(&DesignVector::default(), true)
        .expect("full baseline analysis runs on the default aircraft");
    (sandbox, report)
}

#[test]
fn every_payload_range_corner_respects_the_declared_mtow() {
    let (config, report) = baseline_report(&AlasConfig::default());
    let oew_kg = report_oew_kg(&report);
    let corners = payload_range_corners(&config, &report).expect("corners on the default aircraft");
    assert_eq!(corners.oew_kg, oew_kg);
    eprintln!(
        "default aircraft: OEW {oew_kg:.0} kg, MTOW {:.0} kg, declared cap {:.0} kg, achievable {:.0} kg ({}), fuel capacity {:.0} kg ({}), points {:?}",
        corners.mtow_kg,
        config.requirements.max_structural_payload_kg,
        corners.max_payload_kg,
        corners.payload_basis,
        corners.fuel_capacity_kg,
        corners.fuel_capacity_basis,
        corners.points
    );
    assert!(corners.max_payload_kg <= corners.mtow_kg - oew_kg + 1e-9);
    // The default aircraft declares no structural cap (0 kg), so the corner
    // carries the analysed payload, as the report figure does.
    assert_eq!(config.requirements.max_structural_payload_kg, 0.0);
    assert!(corners
        .payload_basis
        .starts_with("analysed carried payload"));
    assert_eq!(corners.points[0].1, corners.max_payload_kg);
    assert!(corners.points[1].0 > 0.0, "max-payload range is positive");
    // Point B carries the maximum payload plus its fuel; that fuel is at most
    // the MTOW budget left after OEW and payload.
    let fuel_b_kg = corners
        .fuel_capacity_kg
        .min(corners.mtow_kg - oew_kg - corners.max_payload_kg);
    assert!(fuel_b_kg >= 0.0);
    assert!(oew_kg + corners.max_payload_kg + fuel_b_kg <= corners.mtow_kg + 1e-6);
    // Point C trades payload for the full fuel capacity, still under MTOW.
    assert!(oew_kg + corners.points[2].1 + corners.fuel_capacity_kg <= corners.mtow_kg + 1e-6);
    assert!(corners.points.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert!(!corners.payload_basis.is_empty());
}

#[test]
fn a_declared_mtow_that_a_payload_cap_would_exceed_bounds_the_maximum_payload() {
    let mut config = AlasConfig::default();
    let (sandbox, report) = baseline_report(&config);
    let oew_kg = report_oew_kg(&report);
    let budget_kg = 0.5 * (sandbox.requirements.mtow_kg - oew_kg);
    config.requirements.mtow_kg = oew_kg + budget_kg;
    config.requirements.max_structural_payload_kg = 2.0 * budget_kg;
    let (tight, report) = baseline_report(&config);
    let corners = payload_range_corners(&tight, &report).expect("corners with a tight budget");
    let oew_kg = corners.oew_kg;
    assert!(corners.max_payload_kg <= corners.mtow_kg - oew_kg + 1e-9);
    assert!(corners.max_payload_kg < tight.requirements.max_structural_payload_kg);
    assert_eq!(
        corners.payload_basis,
        "MTOW less operating empty mass budget"
    );
    for &(_, payload_kg) in &corners.points {
        assert!(oew_kg + payload_kg <= corners.mtow_kg + 1e-6);
    }
}

#[test]
fn an_empty_mass_at_or_above_mtow_fails_the_diagram_instead_of_drawing_it() {
    let mut config = AlasConfig::default();
    let (_, report) = baseline_report(&config);
    config.requirements.mtow_kg = report_oew_kg(&report) * 0.3;
    let (sandbox, report) = baseline_report(&config);
    match payload_range_corners(&sandbox, &report) {
        Err(PayloadRangeUnavailable::Infeasible(reason)) => {
            assert!(reason.contains("not below the declared MTOW"), "{reason}");
        }
        other => panic!("expected an infeasible diagram, got {other:?}"),
    }
}

#[test]
fn the_quick_payload_capacity_is_an_achievable_estimate_bounded_by_the_declared_cap() {
    let config = AlasConfig::default();
    let request = QuickAnalysisRequest {
        revision: 3,
        config: config.clone(),
        design: DesignVector::default(),
    };
    let mut events = Vec::new();
    let summary = run_quick_analysis(
        &request,
        &mut |event| events.push(event),
        &AtomicBool::new(false),
    );
    assert!(!summary.cancelled);
    let capacity = events
        .iter()
        .find(|event| event.metric == QuickMetric::PayloadCapacity)
        .expect("payload capacity terminates");
    let QuickOutcome::Value(value) = &capacity.outcome else {
        panic!("payload capacity is a value: {:?}", capacity.outcome);
    };
    let oew = events
        .iter()
        .find(|event| event.metric == QuickMetric::OperatingEmptyMass)
        .expect("OEW terminates");
    let QuickOutcome::Value(oew) = &oew.outcome else {
        panic!("OEW is a value");
    };
    // No cap is declared on the default aircraft, so nothing is "requested".
    assert_eq!(value.requested, None);
    assert!(value.achieved > 0.0);
    assert!(value.achieved <= config.requirements.mtow_kg - oew.achieved + 1e-9);
    assert!(value.note.contains("achievable"), "{}", value.note);
    let diagram = events
        .iter()
        .find(|event| event.metric == QuickMetric::PayloadRange)
        .expect("payload-range terminates");
    let QuickOutcome::PayloadRange(corners) = &diagram.outcome else {
        panic!("payload-range is drawn: {:?}", diagram.outcome);
    };
    assert!(corners.max_payload_kg <= config.requirements.mtow_kg - corners.oew_kg + 1e-9);
}

/// The extended stage is the sandbox's Full Analysis: on the A340-300, whose
/// full baseline raises a forward CG-range violation, the published corners
/// are the full analysis' corners to the bit and every finding of the full
/// feasibility assessment is published, nothing dropped or added beyond the
/// closure's own dispatch flags. The cruise L/D, static margin and usable
/// fuel capacity are the values the Full Analysis applies to that report.
#[test]
fn the_extended_stage_publishes_the_full_baseline_values() {
    let preset = alas_config::presets::get("A340-300").expect("A340-300 preset");
    let config =
        AlasConfig::from_value(&serde_json::json!({ "preset": "A340-300" })).expect("preset");
    let sandbox = sandbox_config(&config);
    let report = FullAnalysis::new(sandbox.clone())
        .run(&preset.design_vector, true)
        .expect("full baseline analysis");
    let corners = payload_range_corners(&sandbox, &report).expect("full corners");
    let feasibility = alas_pipeline::feasibility::assess_physical_feasibility(
        &sandbox,
        &preset.design_vector,
        &report,
        None,
    );

    let mut events = Vec::new();
    run_quick_analysis(
        &QuickAnalysisRequest {
            config,
            design: preset.design_vector,
            revision: 1,
        },
        &mut |event| events.push(event),
        &AtomicBool::new(false),
    );
    let outcome = |metric: QuickMetric| {
        events
            .iter()
            .find(|event| event.metric == metric)
            .map(|event| event.outcome.clone())
            .expect("metric terminates")
    };
    let QuickOutcome::PayloadRange(quick) = outcome(QuickMetric::PayloadRange) else {
        panic!("payload-range is drawn");
    };
    assert_eq!(quick, corners);
    let value = |metric: QuickMetric| match outcome(metric) {
        QuickOutcome::Value(value) => value.achieved,
        other => panic!("{metric:?} is a value: {other:?}"),
    };
    let trimmed = report.trimmed_design_point.as_ref().expect("cruise trim");
    assert_eq!(value(QuickMetric::CruiseLiftToDrag), trimmed.l_over_d);
    assert_eq!(
        value(QuickMetric::StaticMargin),
        report.static_margin * 100.0
    );
    let capacity =
        alas_pipeline::feasibility::assess_fuel_capacity(&sandbox, &preset.design_vector, &report);
    assert_eq!(Some(value(QuickMetric::FuelCapacity)), capacity.capacity_kg);
    // Every scalar labelled as the Full Analysis' own value is that value.
    let full_value = |metric: QuickMetric| match metric {
        QuickMetric::OperatingEmptyMass => report_oew_kg(&report),
        QuickMetric::PayloadCapacity => corners.max_payload_kg,
        QuickMetric::CarriedPayload => report.component_masses["Payload"],
        QuickMetric::FuelCapacity => capacity.capacity_kg.expect("capacity"),
        QuickMetric::CruiseLiftToDrag => trimmed.l_over_d,
        QuickMetric::StaticMargin => report.static_margin * 100.0,
        other => panic!("{other:?} has no full-analysis scalar here"),
    };
    for metric in QuickMetric::ALL {
        if metric.basis() == QuickBasis::FullAnalysis
            && !matches!(
                metric,
                // The route fuel is checked against the mission stage in
                // `quick_analysis_closure`.
                QuickMetric::PayloadRange | QuickMetric::Feasibility | QuickMetric::RouteFuelBurn
            )
        {
            let quick = value(metric);
            let full = full_value(metric);
            assert!(
                (quick - full).abs() <= 1e-9 * full.abs().max(1.0),
                "{metric:?}: sandbox {quick} against full {full}"
            );
        }
    }
    let QuickOutcome::Feasibility(flags) = outcome(QuickMetric::Feasibility) else {
        panic!("feasibility is published");
    };
    let published: Vec<String> = flags
        .flags
        .iter()
        .filter(|flag| {
            !matches!(
                flag.code.as_str(),
                "Dispatch" | "LandingMassLimit" | "ZeroFuelMassLimit" | "SizingNotClosed"
            )
        })
        .map(|flag| flag.code.clone())
        .collect();
    let full: Vec<String> = feasibility
        .findings
        .iter()
        .map(|finding| format!("{:?}", finding.code))
        .collect();
    assert_eq!(published, full);
    assert!(
        full.iter()
            .any(|code| code == "ModelCgForwardRangeViolation"),
        "precondition: the full baseline flags the forward CG range, {full:?}"
    );
}
