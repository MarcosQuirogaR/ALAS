// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the staged product search: what a converged run
//! must have earned, what a registered aircraft's envelope means when the
//! caller supplies no bounds, and what must not change when the same search
//! is spread over worker threads.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::{AlasConfig, DesignMode};
use alas_opt::{DesignOptimizer, OptimizationError};

/// A configuration for `preset` in reference-adaptation mode, with a small
/// but honest search budget so the test measures behaviour rather than
/// hardware.
fn reference_config(preset: &str, workers: i64) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.solver.seed = Some(7);
    config.optimizer.solver.workers = workers;
    // Two generations of a six-per-variable population is the smallest
    // budget that still runs the scan, the verification and several polls.
    config.optimizer.solver.max_iterations = 2;
    config.optimizer.solver.population_size = 2;
    config
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

#[test]
fn a_registered_aircraft_is_searched_inside_its_own_envelope_without_restated_bounds() {
    // The global design-variable bounds describe AVE's family. Every other
    // registered type sits outside at least one of them, so intersecting the
    // configured envelope with them used to empty it and reject the run
    // before a single candidate was evaluated. An unbounded call must mean
    // "the design space I configured".
    //
    // Whether a given registered aircraft then reaches a *feasible* candidate
    // under its default route and requirements is a separate question owned by
    // the mass, propulsion and mission models: an A320 loaded without its own
    // operational route is rejected on `mission_profile_range` and
    // `structural_inventory_unverified`, which is a real finding about that
    // configuration and not about the search. So this holds the two properties
    // that are the search's own: the request must not be rejected as invalid
    // bounds, and any design it does return must lie inside the envelope with
    // the locked coordinates untouched.
    for preset in ["A320-200", "ATR72-600", "A380-800"] {
        let config = reference_config(preset, 1);
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
                continue;
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
}

#[test]
fn the_reference_envelope_is_the_ten_percent_window_around_the_loaded_preset() {
    // The D09 interpretation: `x_ref +/- 0.10 |x_ref|` on lengths and scale
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
fn the_same_seed_reaches_the_same_finalist_serially_and_across_workers() {
    // The poll block boundary is a search setting and the worker count only
    // decides how a block is distributed, so a user with more cores must get
    // the same aircraft, not merely a similar one.
    let preset = "AVE";
    let start = nominal(preset);

    let run_with = |workers: i64| {
        let mut optimizer = DesignOptimizer::new(reference_config(preset, workers));
        optimizer
            .run(None, Some(&start), None)
            .unwrap_or_else(|error| panic!("{workers} workers: {error}"))
    };

    let serial = run_with(1);
    let parallel = run_with(8);

    assert_eq!(serial.best_design, parallel.best_design);
    assert_eq!(serial.best_cost.to_bits(), parallel.best_cost.to_bits());
    assert_eq!(serial.termination, parallel.termination);
    assert_eq!(
        serial.history.n_evaluations(),
        parallel.history.n_evaluations()
    );
    let (serial_diagnostics, parallel_diagnostics) = (
        serial
            .search_diagnostics
            .expect("product search reports its lifecycle"),
        parallel
            .search_diagnostics
            .expect("product search reports its lifecycle"),
    );
    assert_eq!(
        serial_diagnostics.analysis_evaluations,
        parallel_diagnostics.analysis_evaluations
    );
    assert_eq!(
        serial_diagnostics.poll_iterations,
        parallel_diagnostics.poll_iterations
    );
    assert_eq!(serial_diagnostics.converged, parallel_diagnostics.converged);
    // The block size is what makes that true, and it must not have followed
    // the worker count.
    assert_eq!(
        serial_diagnostics.poll_block_size,
        parallel_diagnostics.poll_block_size
    );
    assert_eq!(serial_diagnostics.workers, 1);
    assert_eq!(parallel_diagnostics.workers, 8);
}

#[test]
fn a_run_that_reports_convergence_has_a_feasible_improved_candidate() {
    // Convergence is a claim about the aircraft, not about the loop: it may
    // only be reported when the winner satisfies every hard residual (which
    // includes the coupled sizing and dispatch closure) and improves on the
    // first feasible design the search found.
    let preset = "AVE";
    let mut config = reference_config(preset, 4);
    // Enough budget for the mesh to contract to the convergence spacing.
    config.optimizer.solver.max_iterations = 15;
    config.optimizer.solver.population_size = 6;
    let start = nominal(preset);

    let mut optimizer = DesignOptimizer::new(config.clone());
    let result = optimizer
        .run(None, Some(&start), None)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let diagnostics = result
        .search_diagnostics
        .clone()
        .expect("product search reports its lifecycle");

    if diagnostics.converged {
        assert_eq!(result.termination, "converged");
        assert!(result.best_valid);
        assert!(diagnostics.relative_improvement.unwrap_or(0.0) > 0.0);
        // The finalist is re-evaluated by the same objective, independently
        // of the search's own bookkeeping.
        let replay = alas_opt::assess_product_candidate(&config, &result.best_design)
            .unwrap_or_else(|reason| panic!("finalist replay: {reason}"));
        assert!(
            replay.hard_feasible,
            "converged finalist violates {:?}",
            replay.violated_hard_ids()
        );
    } else {
        // Any other outcome must name a budget or safety stop rather than
        // presenting itself as a converged optimization.
        assert_ne!(result.termination, "converged");
        assert!(
            [
                "evaluation_budget",
                "iteration_limit",
                "mesh_limit",
                "fixed_bounds",
                "watchdog",
            ]
            .contains(&result.termination.as_str()),
            "unexpected termination {}",
            result.termination
        );
    }
    assert!(diagnostics.analysis_evaluations > 0);
    assert!(diagnostics.screening_evaluations > 0);
    assert!(diagnostics.verification_evaluations > 0);
}

#[test]
fn a_watchdog_stop_is_never_presented_as_convergence() {
    // The watchdog exists so a pathological configuration cannot run
    // unbounded. It is a diagnostic limit, so if it ever fires the run must
    // report `watchdog`, never `converged`.
    //
    // This checks the contract on whatever the normal run produced rather
    // than forcing a timeout, because a wall-clock trip point is not a stable
    // thing to assert on across machines; the kernel's own
    // `a_watchdog_stop_is_reported_as_not_converged` forces the timeout
    // directly on a synthetic evaluator.
    let preset = "AVE";
    let mut optimizer = DesignOptimizer::new(reference_config(preset, 4));
    let result = optimizer
        .run(None, Some(&nominal(preset)), None)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    if result.termination == "watchdog" {
        assert!(
            !result
                .search_diagnostics
                .expect("product search reports its lifecycle")
                .converged
        );
    }
}
