// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests build every fixture they assert on, so a failed unwrap or expect is
// the assertion failing rather than a library invariant breaking.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::ModelCgConstraint;
use alas_pipeline::gear_stations::{report_config, MAIN_GEAR_TRANSLATION_KEY};

/// An A220 with its wing 1 m aft has its main gear placed by the search.
/// The reporting replay, the figures' gate assessment and the stage-6
/// feasibility verdict all see the placed gear, whichever configuration the
/// caller holds, and agree with the assessed candidate on the ground checks.
#[test]
fn a_placed_candidate_is_reported_and_drawn_with_its_placed_gear() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).unwrap();
    let mut moved = alas_config::presets::get("A220-300").unwrap().design_vector;
    moved.wing_x_shift_m += 1.0;
    let assessment = alas_opt::assess_product_candidate(&config, &moved).unwrap();
    let placement = assessment
        .resolved
        .main_gear_placement
        .expect("the A220 fixture places its gear");
    let ground = |id: &str| {
        assessment
            .residuals
            .iter()
            .find(|row| row.id == id)
            .is_some_and(|row| row.violated())
    };
    assert!(!ground("tip_back") && !ground("min_nose_gear_load"));

    let mut replay = config.clone();
    let design =
        alas_pipeline::acceptance::apply_assessed_aircraft(&assessment.resolved, &mut replay);
    assert_eq!(replay.landing_gear.derived_main_gear, Some(placement));
    let report = alas_pipeline::FullAnalysis::new(replay.clone())
        .run_sized_candidate(&design, &assessment.sized)
        .unwrap();
    assert_eq!(
        report
            .geometry_summary
            .get(MAIN_GEAR_TRANSLATION_KEY)
            .copied(),
        Some(placement.translation_m)
    );
    assert_eq!(
        report_config(&config, &report)
            .landing_gear
            .derived_main_gear,
        Some(placement)
    );

    // The figures read the report's gear from the unplaced configuration.
    let figure = super::model_cg_gate_assessment(&report, &config).unwrap();
    let placed = super::model_cg_gate_assessment(&report, &replay).unwrap();
    assert_eq!(
        figure.main_gear_station_pct_mac.to_bits(),
        placed.main_gear_station_pct_mac.to_bits()
    );
    let mut published_report = report.clone();
    published_report
        .geometry_summary
        .remove(MAIN_GEAR_TRANSLATION_KEY);
    let published = super::model_cg_gate_assessment(&published_report, &config).unwrap();
    let moved_pct_mac = figure.main_gear_station_pct_mac - published.main_gear_station_pct_mac;
    let expected_pct_mac = 100.0 * placement.translation_m / report.airplane.c_ref;
    assert!(
        (moved_pct_mac - expected_pct_mac).abs() < 1e-9 * expected_pct_mac.abs(),
        "figure station moved {moved_pct_mac} % MAC, placement {expected_pct_mac} % MAC"
    );

    // The stage-6 verdict, built from the unplaced configuration, measures
    // the same placed gear and reports no ground mechanism the search met.
    let feasibility =
        alas_pipeline::feasibility::assess_physical_feasibility(&config, &design, &report, None);
    let verdict = feasibility.model_cg.as_ref().expect("a model CG verdict");
    assert_eq!(
        verdict.main_gear_station_pct_mac.to_bits(),
        placed.main_gear_station_pct_mac.to_bits()
    );
    let missed: Vec<_> = verdict
        .loading_states
        .iter()
        .flat_map(|state| state.constraints.iter())
        .filter(|c| {
            c.violated
                && matches!(
                    c.constraint,
                    ModelCgConstraint::TipBack | ModelCgConstraint::MinimumNoseGearLoad
                )
        })
        .collect();
    assert!(missed.is_empty(), "{missed:?}");
}
