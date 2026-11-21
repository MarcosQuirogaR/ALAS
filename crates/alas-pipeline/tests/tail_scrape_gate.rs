// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tail-scrape clearance against the required rotation attitude, by the
//! basis of that requirement (`LandingGearConfig::required_rotation_basis`):
//! a published attitude compared in its own gear state rejects the design
//! when missed; an assumed one, or one published without a gear state, is a
//! warning. Gear-extended and state-unspecified attitudes are compared with
//! the model attitude on the ground plane lowered by the lift-off oleo
//! extension.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use alas_config::{presets, AlasConfig, DesignMode, RotationAngleBasis};
use alas_opt::ModelCgConstraint;
use alas_pipeline::{
    assess_physical_feasibility, FeasibilityReport, FindingCode, FindingSeverity, FullAnalysis,
};

fn assess(name: &str) -> (AlasConfig, FeasibilityReport) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let design = presets::get(name).unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let feasibility = assess_physical_feasibility(&config, &design, &report, None);
    (config, feasibility)
}

/// The gated scrape angle, its limit (deg) and whether it is diagnostic.
fn scrape(feasibility: &FeasibilityReport) -> (f64, f64, bool) {
    feasibility
        .model_cg
        .as_ref()
        .expect("model CG assessment")
        .loading_states
        .iter()
        .flat_map(|state| &state.constraints)
        .find(|constraint| constraint.constraint == ModelCgConstraint::TailScrape)
        .map(|constraint| {
            (
                constraint.actual,
                constraint.limit,
                constraint.is_diagnostic(),
            )
        })
        .expect("tail-scrape constraint")
}

/// The static (gear-compressed) scrape angle, deg.
fn static_scrape(feasibility: &FeasibilityReport) -> f64 {
    feasibility
        .model_cg
        .as_ref()
        .and_then(|model_cg| model_cg.scrape_angle_deg)
        .expect("static scrape angle")
}

fn tail_scrape_findings(feasibility: &FeasibilityReport) -> Vec<FindingSeverity> {
    feasibility
        .findings
        .iter()
        .filter(|finding| finding.code == FindingCode::TailScrapeViolation)
        .map(|finding| finding.severity)
        .collect()
}

#[test]
fn a_missed_published_static_requirement_is_an_error_and_not_feasible() {
    // E195-E2: 9.8 deg, the fuselage angle plus tail-skid angular clearance
    // of E-Jets E2 APM 5824 Rev 21 Tables 2.7-2.9 (static, at weight).
    let (config, feasibility) = assess("E195-E2");
    let gear = &config.landing_gear;
    assert_eq!(
        gear.required_rotation_basis,
        RotationAngleBasis::PublishedStatic
    );
    let (actual, limit, diagnostic) = scrape(&feasibility);
    eprintln!("E195-E2 tail scrape {actual:.3} deg against {limit:.3} deg");
    assert!((limit - 9.8).abs() < 1e-12);
    assert!((actual - static_scrape(&feasibility)).abs() < 1e-12);
    assert!(!diagnostic);
    assert!(actual < limit);
    let severities = tail_scrape_findings(&feasibility);
    assert!(!severities.is_empty());
    assert!(severities
        .iter()
        .all(|severity| *severity == FindingSeverity::Error));
    assert!(!feasibility.is_feasible());
}

#[test]
fn an_assumed_requirement_miss_is_only_a_warning() {
    // C919: no published tail-strike attitude, so the 10 deg default is an
    // assumption; its miss is reported, never a rejection.
    let (config, feasibility) = assess("C919");
    assert_eq!(
        config.landing_gear.required_rotation_basis,
        RotationAngleBasis::Assumed
    );
    let (actual, limit, diagnostic) = scrape(&feasibility);
    eprintln!("C919 tail scrape {actual:.3} deg against {limit:.3} deg");
    assert!(diagnostic);
    let severities = tail_scrape_findings(&feasibility);
    assert_eq!(!severities.is_empty(), actual < limit);
    assert!(severities
        .iter()
        .all(|severity| *severity == FindingSeverity::Warning));
}

#[test]
fn extended_and_unspecified_attitudes_are_met_on_the_extended_gear() {
    // B787-9 9.7 deg "struts extended" (UK AAIB Bulletin 11/2020) is matched
    // and rejects; ATR 72-600 8 deg (AAIB Bulletin 8/2006 p.3) and B747-400
    // 12.5 deg (Boeing AERO 2007 Q1 Fig. 1) give no gear state and only warn.
    // All three are compared with the gear-extended model attitude, larger
    // than the static one.
    for (name, published, basis) in [
        ("B787-9", 9.7, RotationAngleBasis::PublishedGearExtended),
        (
            "ATR72-600",
            8.0,
            RotationAngleBasis::PublishedStateUnspecified,
        ),
        (
            "B747-400",
            12.5,
            RotationAngleBasis::PublishedStateUnspecified,
        ),
    ] {
        let (config, feasibility) = assess(name);
        assert_eq!(config.landing_gear.required_rotation_basis, basis);
        let (actual, limit, diagnostic) = scrape(&feasibility);
        let static_deg = static_scrape(&feasibility);
        eprintln!(
            "{name} tail scrape {actual:.3} deg (static {static_deg:.3}) against {limit:.3} deg"
        );
        assert!((limit - published).abs() < 1e-12);
        assert!(
            actual > static_deg + 0.1,
            "{name}: {actual} vs static {static_deg}"
        );
        assert_eq!(diagnostic, !basis.rejects());
        let severities = tail_scrape_findings(&feasibility);
        assert_eq!(!severities.is_empty(), actual < limit, "{name}");
        let expected = if basis.rejects() {
            FindingSeverity::Error
        } else {
            FindingSeverity::Warning
        };
        assert!(
            severities.iter().all(|severity| *severity == expected),
            "{name}"
        );
    }
}

#[test]
fn the_published_static_requirements_reach_the_gate() {
    // A320-200 and A340-300: the published gear-compressed tail-strike
    // attitudes (Airbus, "Avoiding Tail Strike", NTSB docket attachment,
    // PDF p.12: 11.7 and 10.1 deg).
    for (name, published) in [("A320-200", 11.7), ("A340-300", 10.1)] {
        let (config, feasibility) = assess(name);
        let gear = &config.landing_gear;
        assert_eq!(
            gear.required_rotation_basis,
            RotationAngleBasis::PublishedStatic
        );
        let (actual, limit, diagnostic) = scrape(&feasibility);
        eprintln!("{name} tail scrape {actual:.3} deg against {limit:.3} deg");
        assert!((limit - published).abs() < 1e-12);
        assert!(!diagnostic);
        let raised = !tail_scrape_findings(&feasibility).is_empty();
        assert_eq!(raised, actual < limit, "{name}: {actual} against {limit}");
    }
}
