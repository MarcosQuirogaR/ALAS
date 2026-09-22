// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The printed masses are the numerical evidence this test records.
#![allow(clippy::print_stderr)]

//! Quick Analysis payload contract on the reduced default aircraft: every
//! payload-range corner respects the declared MTOW, the achievable payload
//! is bounded by `MTOW - OEW`, and an empty mass at or above MTOW fails the
//! diagram honestly instead of drawing it.

use std::sync::atomic::AtomicBool;

use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::full_analysis::{AnalysisReport, FullAnalysis};
use alas_pipeline::quick_analysis::{
    payload_range_corners, reduced_config, report_oew_kg, run_quick_analysis,
    PayloadRangeUnavailable, QuickAnalysisRequest, QuickMetric, QuickOutcome,
};

fn reduced_report(config: &AlasConfig) -> (AlasConfig, AnalysisReport) {
    let reduced = reduced_config(config);
    let report = FullAnalysis::new(reduced.clone())
        .run(&DesignVector::default(), true)
        .expect("reduced full analysis runs on the default aircraft");
    (reduced, report)
}

#[test]
fn every_payload_range_corner_respects_the_declared_mtow() {
    let (config, report) = reduced_report(&AlasConfig::default());
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
    let (reduced, report) = reduced_report(&config);
    let oew_kg = report_oew_kg(&report);
    let budget_kg = 0.5 * (reduced.requirements.mtow_kg - oew_kg);
    config.requirements.mtow_kg = oew_kg + budget_kg;
    config.requirements.max_structural_payload_kg = 2.0 * budget_kg;
    let (tight, report) = reduced_report(&config);
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
    let (_, report) = reduced_report(&config);
    config.requirements.mtow_kg = report_oew_kg(&report) * 0.3;
    let (reduced, report) = reduced_report(&config);
    match payload_range_corners(&reduced, &report) {
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
