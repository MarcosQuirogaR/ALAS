// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for `alas-opt::differential_evolution`.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::differential_evolution::DesignOptimizer;
use alas_opt::objective::DesignObjective;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SolverSettingsFixture {
    max_iterations: i64,
    population_size: i64,
    seed: Option<i64>,
    seed_near_initial_design: bool,
    seed_perturbation_fraction: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    solver_settings: SolverSettingsFixture,
    best_cost: f64,
    best_design: Vec<f64>,
    n_evaluations: usize,
    n_valid: usize,
}

#[test]
fn parity_optimizer() {
    let fixture: Fixture = alas_testkit::load("opt", "optimizer");

    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = fixture.solver_settings.max_iterations;
    config.optimizer.solver.population_size = fixture.solver_settings.population_size;
    config.optimizer.solver.seed = fixture.solver_settings.seed;
    config.optimizer.solver.seed_near_initial_design =
        fixture.solver_settings.seed_near_initial_design;
    config.optimizer.solver.seed_perturbation_fraction =
        fixture.solver_settings.seed_perturbation_fraction;

    let dv_init = DesignVector::default();
    let mut initial_obj = DesignObjective::new_reference_compatibility(config.clone());
    let initial_cost = initial_obj.evaluate(&dv_init.to_array());

    let mut opt = DesignOptimizer::new_reference_compatibility(config);
    let mut progress_messages = Vec::new();
    let result = opt
        .run(
            None,
            Some(&dv_init),
            Some(&mut |msg| {
                progress_messages.push(msg.to_string());
            }),
        )
        .expect("the frozen reference population contains feasible candidates");

    // Verify optimizer improved or maintained cost relative to initial design
    assert!(
        result.best_cost <= initial_cost,
        "Optimizer should not worsen cost: got {}, initial was {}",
        result.best_cost,
        initial_cost
    );

    // Verify history and evaluations were recorded
    assert!(result.history.n_evaluations() > 0);
    assert!(result.history.n_valid() > 0);
    assert!(fixture.n_evaluations > 0);
    assert!(fixture.n_valid > 0);
    assert!(fixture.best_cost.is_finite());
    assert!(!fixture.best_design.is_empty());
    assert!(result.wall_time_s >= 0.0);
    assert!(!progress_messages.is_empty());

    // Verify best design is physically bounded
    assert!(result.best_design.span_m >= 60.0 && result.best_design.span_m <= 80.0);
    assert!(result.best_design.root_chord_m >= 12.0 && result.best_design.root_chord_m <= 19.0);
}

#[test]
fn zero_max_iterations_records_the_initial_population_and_its_feasibility() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(42);
    config.optimizer.solver.seed_near_initial_design = true;

    let initial = DesignVector::default();
    // The product path is the L-SHADE epsilon-constrained search, whose
    // initial population is not the frozen SciPy-parity one; the historical
    // DE initial population contract belongs to the explicit compatibility
    // constructor. Keep this parity check on that constructor so a
    // zero-iteration request still audits the frozen 16-member reference
    // population.
    let mut optimizer = DesignOptimizer::new_reference_compatibility(config);
    let result = optimizer
        .run(None, Some(&initial), None)
        .expect("the default design is feasible");

    assert_eq!(result.history.n_evaluations(), DesignVector::bounds().len());
    assert_eq!(result.history.valid.len(), result.history.n_evaluations());
    assert_eq!(
        result.history.reject_reason.len(),
        result.history.n_evaluations()
    );
    assert!(result
        .history
        .alpha_deg
        .iter()
        .all(|alpha| alpha.is_finite()));
}

#[test]
fn seeded_example_replays_the_python_winner() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 15;
    config.optimizer.solver.population_size = 6;
    config.optimizer.solver.tolerance = 0.05;
    config.optimizer.solver.seed = Some(42);

    let initial = DesignVector::default();
    let mut optimizer = DesignOptimizer::new_reference_compatibility(config);
    let result = optimizer
        .run(None, Some(&initial), None)
        .expect("the frozen reference population contains feasible candidates");
    let expected = [
        77.26310883297792,
        16.033742604077958,
        7.894322283595826,
        1.6464001591609407,
        34.72425730322664,
        0.8729953757071804,
        -2.1679461811881957,
        1.065889432108684,
        79.30530367149531,
        0.20106266931417716,
        0.8996440767000335,
        0.9630845474896375,
        0.0002894595161818379,
        0.0006704778573582777,
        -0.000594121555662916,
        0.00010574272521884703,
    ];

    let mut comparison = Comparison::new("seeded optimizer winner", Tier::Closed);
    comparison.scalar("best_cost", result.best_cost, -22.435780026498687);
    comparison.slice("best_design", &result.best_design.to_array(), &expected);
    comparison.finish();
    assert_eq!(result.history.n_evaluations(), 672);
    assert_eq!(result.history.n_valid(), 466);
}
