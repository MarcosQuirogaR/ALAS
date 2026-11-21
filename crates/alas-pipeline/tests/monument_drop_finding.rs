// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A galley or lavatory the section is too narrow to hold is reported, not
//! lost: the passenger summary counts the monuments it placed, lists each one
//! it dropped, and every dropped monument raises a payload warning carrying
//! the placed and the provisioned count.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_payload::LayoutSummary;
use alas_pipeline::{
    assess_physical_feasibility, FindingCode, FindingSeverity, FullAnalysis, PhysicalFinding,
};

fn run(name: &str) -> (alas_payload::layout::PassengerSummary, Vec<PhysicalFinding>) {
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
    let summary = (**summary).clone();
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    (summary, physical.findings)
}

#[test]
fn every_dropped_monument_is_a_finding_and_placed_plus_dropped_is_what_was_provisioned() {
    // The ATR 72-600's provisioning is two galleys and two lavatories (45
    // passengers per lavatory, 100 per galley and the one every cabin has).
    let (summary, findings) = run("ATR72-600");
    let dropped_galleys = summary
        .dropped_monuments
        .iter()
        .filter(|line| line.starts_with("Galley"))
        .count() as i64;
    let dropped_lavatories = summary
        .dropped_monuments
        .iter()
        .filter(|line| line.starts_with("Lav"))
        .count() as i64;
    assert_eq!(summary.galleys + dropped_galleys, 2);
    assert_eq!(summary.lavatories + dropped_lavatories, 2);
    let raised: Vec<_> = findings
        .iter()
        .filter(|finding| finding.code == FindingCode::MonumentDropped)
        .collect();
    assert_eq!(raised.len(), summary.dropped_monuments.len());
    for finding in raised {
        assert_eq!(finding.severity, FindingSeverity::Warning);
        assert!(summary.dropped_monuments.contains(&finding.message));
        assert_eq!(finding.limit.map(|limit| limit as i64), Some(2));
    }
    assert_eq!(FindingCode::MonumentDropped.as_str(), "monument_dropped");
}

#[test]
fn a_cabin_that_places_every_monument_raises_no_finding() {
    let (summary, findings) = run("A320-200");
    assert!(summary.dropped_monuments.is_empty());
    assert!(!findings
        .iter()
        .any(|finding| finding.code == FindingCode::MonumentDropped));
}
