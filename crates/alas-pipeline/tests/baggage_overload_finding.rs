// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Baggage the modelled hold compartments cannot take is reported, not
//! dropped: the ATR 72-600 has no under-floor hold, so its derived main-deck
//! compartments leave an overload that must surface as a payload warning with
//! the overload mass as its numeric detail.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_payload::LayoutSummary;
use alas_pipeline::{
    assess_physical_feasibility, FindingCode, FindingSeverity, FullAnalysis, PhysicalFinding,
};

fn findings_of(name: &str) -> (Vec<PhysicalFinding>, f64) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let design = presets::get(name).unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let Some(LayoutSummary::Passenger(summary)) =
        report.payload_layout.as_ref().map(|layout| &layout.summary)
    else {
        panic!("{name} publishes a passenger layout");
    };
    let overload_kg = summary.overload_kg;
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    (physical.findings, overload_kg)
}

#[test]
fn an_overloaded_hold_raises_a_baggage_warning_carrying_the_overload_mass() {
    let (findings, overload_kg) = findings_of("ATR72-600");
    assert!(overload_kg > 0.0, "the ATR derived holds overflow");
    let finding = findings
        .iter()
        .find(|finding| finding.code == FindingCode::BaggageOverload)
        .expect("the overload raises a baggage finding");
    assert_eq!(finding.severity, FindingSeverity::Warning);
    assert_eq!(
        finding.message,
        "Baggage exceeds the modelled hold compartments"
    );
    assert_eq!(finding.actual, Some(overload_kg));
    assert_eq!(finding.unit, "kg");
    assert_eq!(FindingCode::BaggageOverload.as_str(), "baggage_overload");
}

#[test]
fn a_hold_that_takes_every_bag_raises_no_baggage_warning() {
    let (findings, overload_kg) = findings_of("A320-200");
    assert_eq!(overload_kg, 0.0);
    assert!(!findings
        .iter()
        .any(|finding| finding.code == FindingCode::BaggageOverload));
}
