// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optimization-history contract against the reference panel.
//!
//! The reference figure is a valid-only objective scatter with a running
//! best, colored by span with a colorbar. The native figure deliberately
//! departs from it: it plots every requested candidate of every stage,
//! highlights only the valid ones and drops the span encoding. What carries
//! over, and is checked here, is the running-best series over the valid
//! evaluations and a physically labelled objective axis.

// Invalid checked-in JSON is itself the assertion this fixture-backed test
// needs to report, so decoding is intentionally fail-fast.
#![cfg_attr(test, allow(clippy::expect_used))]

use alas_config::design_variables::DesignVector;
use alas_opt::history::OptimizationHistory;
use alas_report::families::optimization::{
    figure_optimization_history, BEST_VALID_LABEL, HISTORY_TITLE, VALID_LABEL, VALID_RADIUS,
};
use alas_report::scene::SceneElement;
use serde_json::Value;

fn sample_history() -> OptimizationHistory {
    let mut history = OptimizationHistory::new();
    let dv = DesignVector::default();
    for (ld, span) in [(12.0, 28.0), (15.0, 30.0), (13.0, 29.0)] {
        history.record(dv, true, 0.0, ld, span, 0.0, 0.0, 0.0, "");
    }
    history
}

#[test]
fn optimization_history_keeps_the_reference_running_best_over_valid_evaluations() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w39.json"
    ))
    .expect("fixture is valid JSON");
    let reference = &fixture["figures"]["optimization_history:light"];
    assert_eq!(reference["available"], true);
    assert_eq!(reference["axes"][0]["series"][0], "best so far");

    let scene = figure_optimization_history(&sample_history(), Some("light"));
    assert_eq!(scene.title.as_deref(), Some(HISTORY_TITLE));

    let labels: Vec<&str> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"Normalized ranking cost [dimensionless]"));
    assert!(!labels.contains(&"L/D"));
    assert!(labels.contains(&format!("{VALID_LABEL} (3)").as_str()));
    assert!(labels.contains(&BEST_VALID_LABEL));

    let evaluation_points = scene
        .elements
        .iter()
        .filter(|element| {
            matches!(
                element,
                SceneElement::Circle {
                    radius,
                    fill: Some(_),
                    ..
                } if (*radius - VALID_RADIUS).abs() < f64::EPSILON
            )
        })
        .count();
    assert_eq!(evaluation_points, 3);
    assert_eq!(
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polyline { .. }))
            .count(),
        1,
        "the running-best series is present"
    );
}
