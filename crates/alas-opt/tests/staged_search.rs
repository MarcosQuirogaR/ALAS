// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the two-stage product search on real aircraft: what
//! a converged run must have earned, what a registered aircraft's envelope
//! means when the caller supplies no bounds, what must not change when the
//! same search is spread over worker threads, and how a time-limited run is
//! replayed.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::{AlasConfig, DesignMode};
use alas_opt::{DesignOptimizer, OptimizationError};

mod support;
use support::nominal;

/// A configuration for `preset` in reference-adaptation mode, with a small
/// but honest search budget so the test measures behaviour rather than
/// hardware.
fn reference_config(preset: &str, workers: i64) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.solver.seed = Some(7);
    config.optimizer.solver.workers = workers;
    // One screening batch and the 24-member initial population plus about
    // two generations: the smallest budget that runs every stage.
    config.optimizer.solver.screening.max_evaluations = 64;
    config.optimizer.solver.refinement.max_evaluations = 72;
    config
}

/// The configuration the registered-aircraft envelope tests run: the shared
/// reference search with the draft analysis resolution and the worker pool the
/// machine offers. Neither changes what is asserted: the results are
/// independent of the worker count (see the determinism test below), and the
/// assertions concern the envelope and the locked coordinates, not the
/// aerodynamic figures the mesh resolves. Only the five resolution fields of
/// the draft preset are applied, so no tuned assumption is overwritten.
fn registered_aircraft_config(preset: &str) -> AlasConfig {
    let mut config = reference_config(preset, 0);
    let draft = &alas_config::fidelity_presets::get("draft")
        .expect("draft fidelity preset")
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    config
}

/// Search `preset` with no restated bounds and hold the search's own
/// properties.
///
/// The global design-variable bounds describe AVE's family. Every other
/// registered type sits outside at least one of them, so intersecting the
/// configured envelope with them would empty it and reject the run before a
/// single candidate was evaluated. An unbounded call must mean "the design
/// space I configured".
///
/// Whether a given registered aircraft then reaches a *feasible* candidate
/// under its default route and requirements is a separate question owned by
/// the mass, propulsion and mission models: an A320 loaded without its own
/// operational route is rejected on `mission_profile_range` and
/// `structural_inventory_unverified`, which is a real property of that
/// configuration and not of the search. So this holds the two properties that
/// are the search's own: the request must not be rejected as invalid bounds,
/// and any design it does return must lie inside the envelope with the locked
/// coordinates untouched.
fn assert_searched_inside_own_envelope(preset: &str) {
    let config = registered_aircraft_config(preset);
    let start = nominal(preset);
    let envelope = config.optimizer.design_space.envelope(&start);

    let mut optimizer = DesignOptimizer::new(config);
    let design = match optimizer.run(None, Some(&start), None) {
        Ok(result) => result.best_design,
        Err(OptimizationError::NoFeasibleDesign(evidence)) => {
            // The search ran: candidates were built, sized and rejected on
            // physical grounds, which is the outcome this fix makes
            // reachable at all.
            assert!(
                evidence.evaluated_candidates > 0,
                "{preset}: reported no feasible design without evaluating one"
            );
            return;
        }
        Err(error) => panic!("{preset}: {error}"),
    };

    for ((value, variable), spec) in design.to_array().into_iter().zip(&envelope).zip(SPECS) {
        let tolerance = 1.0e-9 * variable.lower.abs().max(variable.upper.abs()).max(1.0);
        assert!(
            value >= variable.lower - tolerance && value <= variable.upper + tolerance,
            "{preset} {}: {value} outside [{}, {}]",
            spec.name,
            variable.lower,
            variable.upper
        );
        if variable.fixed {
            // A locked preset variable is protected geometry: the search
            // may not move it at all, in either direction.
            assert_eq!(value, variable.nominal, "{preset} {} is locked", spec.name);
        }
    }
}

// One test per preset so the runner searches the three aircraft in parallel.
#[test]
fn a_registered_aircraft_is_searched_inside_its_own_envelope_a320_200() {
    assert_searched_inside_own_envelope("A320-200");
}

#[test]
fn a_registered_aircraft_is_searched_inside_its_own_envelope_atr72_600() {
    assert_searched_inside_own_envelope("ATR72-600");
}

#[test]
fn a_registered_aircraft_is_searched_inside_its_own_envelope_a380_800() {
    assert_searched_inside_own_envelope("A380-800");
}

#[test]
fn the_reference_envelope_is_the_ten_percent_window_around_the_loaded_preset() {
    // The reference envelope: `x_ref +/- 0.10 |x_ref|` on lengths and scale
    // factors, anchored at the aircraft that was loaded, not at the latest
    // candidate, so repeated runs cannot compound the allowance.
    let preset = "A320-200";
    let config = reference_config(preset, 1);
    let start = nominal(preset);
    let envelope = config.optimizer.design_space.envelope(&start);

    let span = envelope.iter().find(|v| v.name == "span_m").unwrap();
    assert!((span.lower - 0.9 * start.span_m).abs() < 1.0e-9);
    assert!((span.upper - 1.1 * start.span_m).abs() < 1.0e-9);

    // Re-anchoring on an optimized candidate would widen the window; the
    // envelope is a function of the reference it is handed, so the test is
    // that the caller anchors it on the preset.
    let moved = DesignVector {
        span_m: start.span_m * 1.1,
        ..start
    };
    let compounded = config.optimizer.design_space.envelope(&moved);
    let compounded_span = compounded.iter().find(|v| v.name == "span_m").unwrap();
    assert!(compounded_span.upper > span.upper);
}

#[test]
fn the_same_seed_reaches_the_same_winner_at_1_4_and_16_workers() {
    // Every batch is built from the seed before it is evaluated and every
    // score is stored at its own index, so the worker count may change the
    // wall time and nothing else: same winner bits, same history, same
    // per-stage accounting.
    let preset = "AVE";
    let start = nominal(preset);
    // Time limits are machine- and load-dependent by design, so the run stops
    // on its evaluation budgets only: the documented reproducible mode.
    let run_with = |workers: i64| {
        let mut config = reference_config(preset, workers);
        config.optimizer.solver.stop_on_evaluations_only = true;
        let mut optimizer = DesignOptimizer::new(config);
        optimizer
            .run(None, Some(&start), None)
            .unwrap_or_else(|error| panic!("{workers} workers: {error}"))
    };
    let serial = run_with(1);
    for workers in [4, 16] {
        let parallel = run_with(workers);
        assert_eq!(serial.best_design, parallel.best_design, "{workers}");
        assert_eq!(serial.best_cost.to_bits(), parallel.best_cost.to_bits());
        assert_eq!(serial.termination, parallel.termination);
        assert_eq!(
            serial.history.design_vectors,
            parallel.history.design_vectors
        );
        let (a, b) = (
            serial.search_diagnostics.clone().expect("diagnostics"),
            parallel.search_diagnostics.expect("diagnostics"),
        );
        assert_eq!(a.analysis_evaluations, b.analysis_evaluations);
        assert_eq!(a.poll_iterations, b.poll_iterations);
        assert_eq!(
            a.poll_block_size, b.poll_block_size,
            "N_init follows the budget"
        );
        let counts = |d: &alas_opt::SearchDiagnostics| {
            d.stages
                .iter()
                .map(|s| {
                    (
                        s.evaluations,
                        s.elite_size,
                        s.termination.clone(),
                        s.sizing_work,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&a), counts(&b));
        assert_eq!(b.workers, workers as usize);
    }
}

#[test]
fn every_stage_respects_its_budget_and_a_time_limited_run_replays_exactly() {
    // The test is about stage budgets and replay, so it needs a preset whose
    // nominal passes the hard gate: the AVE nominal now fails the rotation
    // forward-CG limit (a physical finding, see `mission_sized`), leaving a
    // four-evaluation search nothing feasible to return.
    let preset = "A320-200";
    let start = nominal(preset);
    let mut limited = reference_config(preset, 4);
    limited.optimizer.solver.screening.time_limit_s = 3.0;
    limited.optimizer.solver.refinement.time_limit_s = 6.0;
    let first = DesignOptimizer::new(limited.clone())
        .run(None, Some(&start), None)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let stages = first
        .search_diagnostics
        .clone()
        .expect("diagnostics")
        .stages;
    assert_eq!(stages.len(), 2);
    for stage in &stages {
        assert!(stage.evaluations <= stage.max_evaluations, "{stage:?}");
        assert!(
            ["evaluation_budget", "time_budget", "converged", "stagnated"]
                .contains(&stage.termination.as_str()),
            "{stage:?}"
        );
        // Every sized candidate ran under the cap measured on the nominal,
        // and none flew more trips than it.
        let work = stage.sizing_work.expect("candidates sized");
        let flights = work.cap_trip_flights.expect("capped");
        assert!(work.cap_deck_evals.is_some(), "{work:?}");
        assert!(work.trip_flights.max <= u64::from(flights), "{work:?}");
        assert!(work.trip_flights.p50 <= work.trip_flights.max);
    }

    // Replaying with each stage stopped at its recorded count and no time
    // pressure evaluates the same candidates and returns the same winner.
    let mut replay = limited;
    replay.optimizer.solver.screening.time_limit_s = 300.0;
    replay.optimizer.solver.refinement.time_limit_s = 300.0;
    replay.optimizer.solver.screening.replay_evaluations = Some(stages[0].evaluations as i64);
    replay.optimizer.solver.refinement.replay_evaluations = Some(stages[1].evaluations as i64);
    replay
        .optimizer
        .solver
        .refinement
        .replay_planned_evaluations = Some(stages[1].planned_evaluations as i64);
    let second = DesignOptimizer::new(replay)
        .run(None, Some(&start), None)
        .unwrap_or_else(|error| panic!("{preset} replay: {error}"));
    assert_eq!(first.best_design, second.best_design);
    assert_eq!(first.best_cost.to_bits(), second.best_cost.to_bits());
    assert_eq!(first.history.design_vectors, second.history.design_vectors);
}

#[test]
fn a_run_that_reports_convergence_has_a_feasible_candidate_the_replay_accepts() {
    // Convergence is a claim about the aircraft, not about the loop: it may
    // only be reported for a winner that satisfies every hard residual.
    let preset = "AVE";
    let mut config = reference_config(preset, 16);
    config.optimizer.solver.refinement.max_evaluations = 400;
    let start = nominal(preset);

    let result = DesignOptimizer::new(config.clone())
        .run(None, Some(&start), None)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let diagnostics = result
        .search_diagnostics
        .clone()
        .expect("product search reports its lifecycle");
    assert_eq!(diagnostics.converged, result.termination == "converged");
    if diagnostics.converged {
        assert!(result.best_valid);
        let replay = alas_opt::assess_product_candidate(&config, &result.best_design)
            .unwrap_or_else(|reason| panic!("finalist replay: {reason}"));
        assert!(
            replay.hard_feasible,
            "converged finalist violates {:?}",
            replay.violated_hard_ids()
        );
    }
    assert!(diagnostics.analysis_evaluations > 0);
    assert!(diagnostics.screening_evaluations > 0);
    let baseline = diagnostics
        .baseline
        .expect("the baseline is always evaluated");
    if result.best_valid && baseline.baseline_feasible {
        assert!(baseline.winner_objective_value <= baseline.baseline_objective_value);
    }
}
