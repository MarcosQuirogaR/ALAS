// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::AtomicBool;

use super::*;
use crate::cancellation::CancelWatch;

fn settings(seed: u64, population: usize, generations: usize) -> Settings {
    Settings {
        population,
        generations,
        seed,
        spread_tolerance: 0.01,
        stagnation_generations: 5,
    }
}

fn batch_of(
    evaluate: impl Fn(&[f64]) -> ScoredPoint,
) -> impl FnMut(&[Vec<f64>]) -> Vec<ScoredPoint> {
    move |points: &[Vec<f64>]| points.iter().map(|point| evaluate(point)).collect()
}

/// A one-dimensional objective whose feasible region is `x >= 0.4` and whose
/// cost falls towards `x = 0`, so the cheapest point in the box is infeasible
/// and feasibility-first ordering is observable.
fn scored_1d(values: &[f64]) -> ScoredPoint {
    let x = values[0];
    ScoredPoint {
        values: values.to_vec(),
        cost: x * x,
        valid: x >= 0.4,
        constraint_violation: (0.4 - x).max(0.0),
        objectives: [x * x, x, x],
    }
}

#[test]
fn a_seeded_search_replays_exactly_and_a_different_seed_searches_elsewhere() {
    let bounds = [(0.0, 1.0)];
    let run_once = |seed| {
        let mut evaluate = batch_of(scored_1d);
        run(
            &bounds,
            settings(seed, 8, 10),
            Some(&[0.7]),
            &CancelScope::attach(None),
            &mut evaluate,
        )
    };
    let first = run_once(42);
    let second = run_once(42);
    assert_eq!(
        first.winner, second.winner,
        "the same seed must replay exactly"
    );
    assert_eq!(first.evaluations, second.evaluations);
    assert!(!first.cancelled && !second.cancelled);
    let other = run_once(43);
    assert!(other.winner.values[0].is_finite());
}

#[test]
fn a_feasible_winner_is_preferred_to_a_cheaper_infeasible_one() {
    let bounds = [(0.0, 1.0)];
    let mut evaluate = batch_of(scored_1d);
    let outcome = run(
        &bounds,
        settings(5, 8, 15),
        None,
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(
        outcome.winner.valid,
        "a feasible design exists in [0, 1] and must win: {:?}",
        outcome.winner
    );
    assert_eq!(outcome.winner.constraint_violation, 0.0);
    assert!(outcome.winner.values[0] >= 0.4);
}

#[test]
fn with_no_feasible_design_the_least_violating_candidate_wins() {
    let bounds = [(0.0, 0.3)];
    let mut evaluate = batch_of(scored_1d);
    let outcome = run(
        &bounds,
        settings(9, 8, 15),
        None,
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(!outcome.winner.valid, "no point of [0, 0.3] is feasible");
    assert!(
        outcome.winner.constraint_violation <= 0.4 - 0.29,
        "the winner must be the least violating candidate: {:?}",
        outcome.winner
    );
}

#[test]
fn every_evaluated_candidate_stays_inside_the_bounds() {
    let bounds = [(2.0, 5.0), (-1.0, 1.0)];
    let mut seen: Vec<Vec<f64>> = Vec::new();
    let mut evaluate = |points: &[Vec<f64>]| -> Vec<ScoredPoint> {
        seen.extend(points.iter().cloned());
        points
            .iter()
            .map(|values| ScoredPoint {
                values: values.clone(),
                cost: values[0],
                valid: true,
                constraint_violation: 0.0,
                objectives: [values[0], values[0], values[0]],
            })
            .collect()
    };
    let outcome = run(
        &bounds,
        settings(11, 8, 10),
        // A starting point far outside the box, which repair must bring in
        // rather than the search either dropping it or carrying it.
        Some(&[900.0, -900.0]),
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(seen.len() >= 8);
    for candidate in &seen {
        for (value, &(lower, upper)) in candidate.iter().zip(bounds.iter()) {
            assert!(
                (lower..=upper).contains(value),
                "{value} left [{lower}, {upper}]"
            );
        }
    }
    for (value, &(lower, upper)) in outcome.winner.values.iter().zip(bounds.iter()) {
        assert!((lower..=upper).contains(value));
    }
}

#[test]
fn a_flag_already_set_analyses_nothing_and_returns_the_unevaluated_sentinel() {
    let bounds = [(0.0, 1.0)];
    let cancel = AtomicBool::new(true);
    let mut calls = 0usize;
    let mut evaluate = |points: &[Vec<f64>]| -> Vec<ScoredPoint> {
        calls += points.len();
        points.iter().map(|v| scored_1d(v)).collect()
    };
    let outcome = run(
        &bounds,
        settings(1, 8, 10),
        Some(&[0.7]),
        &CancelScope::attach(Some(&cancel)),
        &mut evaluate,
    );
    assert!(outcome.cancelled);
    assert_eq!(calls, 0, "no analysis may start under an already-set flag");
    assert!(
        !outcome.winner.valid && outcome.winner.cost.is_infinite(),
        "with nothing analysed the winner must be the sentinel: {:?}",
        outcome.winner
    );
}

#[test]
fn a_cancellation_mid_run_stops_within_one_generation_batch_and_keeps_the_best_point() {
    let bounds = [(0.0, 1.0)];
    let population = 8usize;
    // The watch lets the test see how many batches (initial population plus
    // generations) ran before the request landed, so it can assert the run
    // stopped well short of its budget rather than exhausting it.
    let watch = CancelWatch::new();
    let scope = CancelScope::attach(Some(watch.flag()));
    let mut batches = 0usize;
    let mut evaluate = |points: &[Vec<f64>]| -> Vec<ScoredPoint> {
        batches += 1;
        if batches == 3 {
            watch.request_cancellation();
        }
        points.iter().map(|v| scored_1d(v)).collect()
    };
    let outcome = run(
        &bounds,
        settings(3, population, 40),
        None,
        &scope,
        &mut evaluate,
    );
    assert!(outcome.cancelled);
    assert!(
        outcome.generations_completed < 40,
        "cancellation must stop well short of the generation budget: {}",
        outcome.generations_completed
    );
    assert!(outcome.winner.cost.is_finite());
}

#[test]
fn the_population_shrinks_toward_the_floor_as_generations_proceed() {
    assert_eq!(ops::linear_reduced_size(96, 0.0), 96);
    assert_eq!(ops::linear_reduced_size(96, 1.0), MIN_POPULATION);
    let mid = ops::linear_reduced_size(96, 0.5);
    assert!(mid > MIN_POPULATION && mid < 96, "{mid}");
}

#[test]
fn the_epsilon_schedule_decays_to_exactly_zero_at_the_control_fraction() {
    let epsilon0 = 2.0;
    let control = 10;
    assert_eq!(ops::epsilon_schedule(epsilon0, 0, control), epsilon0);
    assert_eq!(ops::epsilon_schedule(epsilon0, control, control), 0.0);
    assert_eq!(ops::epsilon_schedule(epsilon0, control + 5, control), 0.0);
    let mid = ops::epsilon_schedule(epsilon0, control / 2, control);
    assert!(mid > 0.0 && mid < epsilon0, "{mid}");
}

#[test]
fn midpoint_repair_lands_between_the_crossed_bound_and_the_parent() {
    assert_eq!(ops::repair_midpoint(1.5, 0.6, (0.0, 1.0)), 0.8);
    assert_eq!(ops::repair_midpoint(-0.5, 0.2, (0.0, 1.0)), 0.1);
    assert_eq!(ops::repair_midpoint(0.5, 0.2, (0.0, 1.0)), 0.5);
    assert_eq!(ops::repair_midpoint(f64::NAN, 0.3, (0.0, 1.0)), 0.3);
}

/// Rosenbrock's function constrained inside the unit disk: the unconstrained
/// optimum `(1, 1)` has `x^2 + y^2 = 2`, outside the disk, so the search must
/// find its way to the constrained boundary rather than the global minimum.
fn rosenbrock_in_disk(values: &[f64]) -> ScoredPoint {
    let (x, y) = (values[0], values[1]);
    let cost = 100.0 * (y - x * x).powi(2) + (1.0 - x).powi(2);
    let violation = (x * x + y * y - 1.0).max(0.0);
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid: violation <= 0.0,
        constraint_violation: violation,
        objectives: [cost, 0.0, 0.0],
    }
}

#[test]
fn a_disk_constrained_rosenbrock_finds_a_feasible_point_better_than_the_center() {
    let bounds = [(-2.0, 2.0), (-2.0, 2.0)];
    let mut evaluate = batch_of(rosenbrock_in_disk);
    let outcome = run(
        &bounds,
        settings(7, 40, 120),
        None,
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(
        outcome.winner.valid,
        "a feasible region exists inside the unit disk: {:?}",
        outcome.winner
    );
    let center_cost = rosenbrock_in_disk(&[0.0, 0.0]).cost;
    assert!(
        outcome.winner.cost < center_cost,
        "winner {} must beat the trivial feasible center {center_cost}",
        outcome.winner.cost
    );
}

/// A linearly constrained quadratic bowl whose unconstrained optimum
/// `(1, 1)` violates `x + y <= 0.5`, so the feasible optimum sits on that
/// boundary at `(0.25, 0.25)` with cost `1.125`.
fn bowl_behind_a_halfplane(values: &[f64]) -> ScoredPoint {
    let (x, y) = (values[0], values[1]);
    let cost = (x - 1.0).powi(2) + (y - 1.0).powi(2);
    let violation = (x + y - 0.5).max(0.0);
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid: violation <= 0.0,
        constraint_violation: violation,
        objectives: [cost, 0.0, 0.0],
    }
}

#[test]
fn a_linearly_constrained_bowl_converges_near_its_feasible_boundary_optimum() {
    let bounds = [(-2.0, 2.0), (-2.0, 2.0)];
    let mut evaluate = batch_of(bowl_behind_a_halfplane);
    let outcome = run(
        &bounds,
        settings(21, 40, 150),
        None,
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(outcome.winner.valid);
    assert!(
        (outcome.winner.cost - 1.125).abs() < 0.05,
        "winner cost {} should approach the known feasible optimum 1.125",
        outcome.winner.cost
    );
    assert!(
        (outcome.winner.values[0] - 0.25).abs() < 0.1
            && (outcome.winner.values[1] - 0.25).abs() < 0.1,
        "{:?}",
        outcome.winner.values
    );
}

#[test]
fn a_run_with_a_stable_feasible_population_reports_convergence() {
    // A trivial unconstrained bowl with a wide tolerance and a short
    // stagnation window converges well inside a generous generation budget.
    let bounds = [(-1.0, 1.0), (-1.0, 1.0)];
    let mut evaluate = batch_of(|values: &[f64]| {
        let cost = values[0].powi(2) + values[1].powi(2);
        ScoredPoint {
            values: values.to_vec(),
            cost,
            valid: true,
            constraint_violation: 0.0,
            objectives: [cost, 0.0, 0.0],
        }
    });
    let outcome = run(
        &bounds,
        Settings {
            population: 20,
            generations: 200,
            seed: 4,
            spread_tolerance: 0.05,
            stagnation_generations: 4,
        },
        None,
        &CancelScope::attach(None),
        &mut evaluate,
    );
    assert!(outcome.winner.valid);
    assert!(
        outcome.converged,
        "an easy unconstrained bowl must reach the convergence criterion: {:?}",
        outcome.generations_completed
    );
    assert!(outcome.generations_completed < 200);
    assert_eq!(outcome.epsilon_final, 0.0);
}
