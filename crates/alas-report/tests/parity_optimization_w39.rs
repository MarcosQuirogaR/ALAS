// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W3.9 optimization-history parity: the Rust scene preserves the reference
//! L/D series, span color encoding, running-best trace, and colorbar contract.

// Invalid checked-in JSON is itself the assertion this fixture-backed test
// needs to report, so decoding is intentionally fail-fast.
#![cfg_attr(test, allow(clippy::expect_used))]

use alas_config::design_variables::DesignVector;
use alas_opt::history::OptimizationHistory;
use alas_report::families::optimization::figure_optimization_history;
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
fn optimization_history_matches_the_reference_panel_and_series_contract() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w39.json"
    ))
    .expect("W3.9 fixture is valid JSON");
    let reference = &fixture["figures"]["optimization_history:light"];
    assert_eq!(reference["available"], true);
    assert_eq!(reference["panel_count"], 2);
    assert_eq!(reference["axes"][0]["series"][0], "best so far");
    assert_eq!(reference["axes"][1]["ylabel"], "span [m]");

    let scene = figure_optimization_history(&sample_history(), Some("light"));
    assert_eq!(
        scene.title.as_deref(),
        Some("Optimization convergence (3 valid evaluations)")
    );

    let labels: Vec<&str> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"valid evaluation #"));
    assert!(labels.contains(&"L/D"));
    assert!(labels.contains(&"Span [m]"));
    assert!(labels.contains(&"Evaluation"));
    assert!(labels.contains(&"Best so far"));

    let evaluation_points = scene
        .elements
        .iter()
        .filter(|element| {
            matches!(
                element,
                SceneElement::Circle {
                    radius,
                    fill: Some(_),
                    stroke: None,
                    ..
                } if (*radius - 3.5).abs() < f64::EPSILON
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
    assert!(
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Rect { fill: Some(_), .. }))
            .count()
            >= 64,
        "the span colorbar is present"
    );
}
