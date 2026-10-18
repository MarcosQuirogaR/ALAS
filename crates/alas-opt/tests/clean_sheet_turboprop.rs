// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The regional turboprop class start of a clean-sheet brief
//! (`alas_config::clean_sheet`): evaluated through the product assessment,
//! the derived start must sit within a few percent of the engine-out climb
//! requirements, which the jet-derived start missed by 16 percent (second
//! segment, with the jet high-lift level and an aspect ratio of 8.6), with
//! the catalogue engine's installed power unchanged. The start is not asked
//! to be feasible: the search closes the remaining 1-5 percent (the class
//! start is 0.75 percent short on the second segment, 4.7 percent on the
//! approach climb, at the Frankfurt departure).

// A test asserts on values it built from a brief it controls, so a failed
// unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use serde_json::json;

/// 70 seats, 800 nmi, no preset: only the brief's own numbers and the engine.
fn brief() -> serde_json::Value {
    json!({
        "requirements": {
            "mtow_kg": 23_000.0, "num_passengers": 70,
            "cruise_mach": 0.44, "cruise_altitude_m": 5_180.0
        },
        "geometry": {"fuselage": {"diameter_m": 2.865}, "engine": {"engine_name": "PW127M"}},
        "optimizer": {"objective": {"design_range_nmi": 800.0}}
    })
}

#[test]
fn the_turboprop_class_start_is_within_a_few_percent_of_the_engine_out_climb_checks() {
    let config = AlasConfig::from_value(&brief()).unwrap();
    assert!(config.derives_clean_sheet_start());
    let design = config.configured_nominal_design();
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let row = |id: &str| {
        assessment
            .residuals
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    };
    // Required static T/W (limit) against the installed one (actual) with the
    // catalogue's two PW127M: the final segment is met outright, the second
    // segment and the approach climb are within 3 and 8 percent.
    for (id, allowed_shortfall) in [
        ("oei_final_segment", 0.0),
        ("oei_second_segment", 0.03),
        ("oei_approach_climb", 0.08),
    ] {
        let r = row(id);
        assert!(
            r.limit.is_finite() && r.limit <= (1.0 + allowed_shortfall) * r.actual,
            "{id}: required {} against installed {}",
            r.limit,
            r.actual
        );
    }
    // Installed T/W is the catalogue engine at the brief MTOW, not a free variable.
    let installed = row("oei_second_segment").actual;
    assert!((0.30..0.34).contains(&installed), "{installed}");
    // The class wing is inside the wing-loading floor of the class.
    assert!(!row("wing_loading").violated());
    assert!(config.performance.cl_max_to < config.performance.cl_max_land);
}
