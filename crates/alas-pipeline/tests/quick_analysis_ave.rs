// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The timing line is the benchmark evidence this test exists to report.
#![allow(clippy::print_stderr)]

//! The reduced Quick Analysis on the AVE reference: every metric terminates,
//! values are finite, stale revisions stay identifiable, and cancellation
//! stops the run early.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::quick_analysis::{
    reduced_config, run_quick_analysis, QuickAnalysisRequest, QuickMetric, QuickOutcome, QuickStage,
};

fn ave_request(revision: u64) -> QuickAnalysisRequest {
    QuickAnalysisRequest {
        config: AlasConfig::default(),
        design: DesignVector::default(),
        revision,
    }
}

#[test]
fn every_metric_terminates_exactly_once_with_finite_values_on_ave() {
    let request = ave_request(7);
    let mut events = Vec::new();
    let started = Instant::now();
    let summary = run_quick_analysis(
        &request,
        &mut |event| events.push(event),
        &AtomicBool::new(false),
    );
    let wall_ms = started.elapsed().as_millis();
    eprintln!(
        "quick analysis (debug build): initial stage {} ms, final {} ms, wall {} ms",
        summary.initial_stage_ms, summary.final_ms, wall_ms
    );

    assert!(!summary.cancelled);
    assert_eq!(events.len(), QuickMetric::ALL.len());
    for metric in QuickMetric::ALL {
        let matching: Vec<_> = events.iter().filter(|e| e.metric == metric).collect();
        assert_eq!(matching.len(), 1, "{metric:?} must terminate once");
        assert_eq!(matching[0].revision, 7);
        match &matching[0].outcome {
            QuickOutcome::Value(value) => {
                assert!(value.achieved.is_finite(), "{metric:?} achieved is finite");
                assert!(
                    value.requested.is_none_or(f64::is_finite),
                    "{metric:?} requested is finite"
                );
                assert!(!value.note.is_empty());
            }
            QuickOutcome::PayloadRange(corners) => {
                assert_eq!(corners.points.len(), 4);
                assert!(corners
                    .points
                    .iter()
                    .all(|(r, p)| r.is_finite() && p.is_finite()));
                assert!(corners.points[1].0 > 0.0, "max-payload range is positive");
            }
            QuickOutcome::Feasibility(flags) => {
                assert_eq!(flags.feasible, flags.flags.iter().all(|f| !f.blocking));
            }
            QuickOutcome::Failed(message) | QuickOutcome::Unsupported(message) => {
                panic!("{metric:?} did not produce a value on AVE: {message}");
            }
        }
    }
    let takeoff = events
        .iter()
        .find(|e| e.metric == QuickMetric::TakeoffMass)
        .and_then(|e| match &e.outcome {
            QuickOutcome::Value(v) => Some(v.clone()),
            _ => None,
        })
        .expect("takeoff mass value");
    assert!(takeoff.achieved > 100_000.0 && takeoff.achieved < 500_000.0);
    assert_eq!(takeoff.requested, Some(request.config.requirements.mtow_kg));
    let ceiling = events
        .iter()
        .find(|e| e.metric == QuickMetric::ServiceCeiling)
        .and_then(|e| match &e.outcome {
            QuickOutcome::Value(v) => Some(v.achieved),
            _ => None,
        })
        .expect("ceiling value");
    assert!(
        ceiling > 8_000.0 && ceiling <= 20_000.0,
        "ceiling {ceiling} m"
    );
    // Initial-stage metrics are published before every extended one.
    let last_initial = events
        .iter()
        .rposition(|e| e.metric.stage() == QuickStage::Initial)
        .expect("initial metrics");
    let first_extended = events
        .iter()
        .position(|e| e.metric.stage() == QuickStage::Extended)
        .expect("extended metrics");
    assert!(last_initial < first_extended);
}

#[test]
fn the_reduced_configuration_keeps_geometry_and_requirements_untouched() {
    let config = AlasConfig::default();
    let reduced = reduced_config(&config);
    assert_eq!(reduced.geometry, config.geometry);
    assert_eq!(reduced.requirements, config.requirements);
    assert!(reduced.analysis.sweep_n_points <= 9);
    assert!(reduced.analysis.fine_spanwise_resolution <= config.analysis.fine_spanwise_resolution);
    assert_eq!(
        reduced.optimizer.design_space.mode,
        alas_config::optimizer::DesignMode::BaselineSandbox
    );
}

#[test]
fn a_cancelled_run_stops_before_the_extended_stage() {
    let request = ave_request(3);
    let cancel = AtomicBool::new(true);
    let mut events = Vec::new();
    let summary = run_quick_analysis(&request, &mut |event| events.push(event), &cancel);
    assert!(summary.cancelled);
    assert!(events
        .iter()
        .all(|e| e.metric.stage() == QuickStage::Initial));
    assert!(events.len() < QuickMetric::ALL.len());
}

#[test]
fn an_unbuildable_geometry_fails_every_metric_honestly() {
    let mut request = ave_request(1);
    request.design.tip_chord_m = -1.0;
    let mut events = Vec::new();
    run_quick_analysis(
        &request,
        &mut |event| events.push(event),
        &AtomicBool::new(false),
    );
    assert_eq!(events.len(), QuickMetric::ALL.len());
    assert!(events
        .iter()
        .all(|e| matches!(e.outcome, QuickOutcome::Failed(_))));
}
