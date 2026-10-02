// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::cancellation::CancelWatch;

fn settings(seed: u64, budget: usize) -> Settings {
    Settings {
        max_evaluations: budget,
        stop_after: budget,
        population: 20,
        seed,
        adaptation: false,
        spread_tolerance: 0.0,
        stagnation_generations: usize::MAX,
        time_limit: None,
        infeasible_reserve: 0,
        max_rejects: 20 * budget,
    }
}

fn scored(values: &[f64], cost: f64, violation: f64) -> ScoredPoint {
    ScoredPoint {
        values: values.to_vec(),
        cost,
        tier: if violation > 0.0 {
            Tier::ClosedInfeasible
        } else {
            Tier::Feasible
        },
        constraint_violation: violation,
        objectives: [cost, 0.0, 0.0],
    }
}

/// A bowl whose unconstrained optimum `(1, 1)` violates `x + y <= 0.5`, so
/// the feasible optimum sits on that boundary at `(0.25, 0.25)`, cost 1.125.
fn bowl_behind_a_halfplane(values: &[f64]) -> ScoredPoint {
    let (x, y) = (values[0], values[1]);
    scored(
        values,
        (x - 1.0).powi(2) + (y - 1.0).powi(2),
        (x + y - 0.5).max(0.0),
    )
}

fn batch_of<'a>(
    evaluate: impl Fn(&[f64]) -> ScoredPoint + 'a,
    seen: &'a mut Vec<usize>,
) -> impl FnMut(&[Vec<f64>]) -> Vec<ScoredPoint> + 'a {
    move |points: &[Vec<f64>]| {
        seen.push(points.len());
        points.iter().map(|point| evaluate(point)).collect()
    }
}

fn run_bowl(settings: Settings, seen: &mut Vec<usize>) -> Outcome {
    run(
        &[(-2.0, 2.0), (-2.0, 2.0)],
        &[],
        settings,
        Instant::now(),
        &CancelScope::attach(None),
        &mut batch_of(bowl_behind_a_halfplane, seen),
    )
}

#[test]
fn the_budget_is_spent_exactly_and_the_population_shrinks_to_its_floor() {
    let mut batches = Vec::new();
    let outcome = run_bowl(settings(3, 403), &mut batches);
    assert_eq!(outcome.evaluations, 403);
    assert_eq!(batches.iter().sum::<usize>(), 403);
    assert_eq!(outcome.termination, Termination::EvaluationBudget);
    assert_eq!(batches[0], 20, "the initial population is N_init");
    // Linear reduction in evaluations: generations never grow and the last
    // full generation sits at the floor of eight.
    assert!(batches[1..].windows(2).all(|pair| pair[1] <= pair[0]));
    assert_eq!(outcome.population_final, MIN_POPULATION);
    // A feasible optimum on the constraint boundary is found.
    assert!(outcome.winner.valid());
    assert!(
        (outcome.winner.cost - 1.125).abs() < 0.05,
        "{:?}",
        outcome.winner
    );
}

#[test]
fn a_replay_stopped_at_the_recorded_count_is_the_same_run() {
    let mut long_batches = Vec::new();
    let long = run_bowl(settings(9, 300), &mut long_batches);
    // A run cut after the first 132 evaluations (as a time limit would cut
    // it, between generations) matches the full run's prefix exactly: the
    // population schedule depends on the budget, not on where it stopped.
    let cut = long_batches
        .iter()
        .scan(0, |sum, &len| {
            *sum += len;
            Some(*sum)
        })
        .find(|&sum| sum >= 132)
        .unwrap();
    let mut seen = Vec::new();
    let mut replay_points = Vec::new();
    let replay = run(
        &[(-2.0, 2.0), (-2.0, 2.0)],
        &[],
        Settings {
            stop_after: cut,
            ..settings(9, 300)
        },
        Instant::now(),
        &CancelScope::attach(None),
        &mut |points: &[Vec<f64>]| {
            seen.push(points.len());
            replay_points.extend_from_slice(points);
            points.iter().map(|p| bowl_behind_a_halfplane(p)).collect()
        },
    );
    assert_eq!(replay.evaluations, cut);
    assert_eq!(seen[..], long_batches[..seen.len()]);
    assert!(long.winner.feasibility_key() <= replay.winner.feasibility_key());
    let again = run_bowl(
        Settings {
            stop_after: cut,
            ..settings(9, 300)
        },
        &mut Vec::new(),
    );
    assert_eq!(again.winner, replay.winner);
}

/// The bowl behind a design-vector gate rejecting `x < threshold`.
fn gated_bowl(threshold: f64) -> impl Fn(&[f64]) -> ScoredPoint {
    move |values: &[f64]| {
        if values[0] < threshold {
            ScoredPoint {
                tier: Tier::PreGateFailed,
                cost: f64::INFINITY,
                constraint_violation: threshold - values[0],
                ..bowl_behind_a_halfplane(values)
            }
        } else {
            bowl_behind_a_halfplane(values)
        }
    }
}

fn run_gated(threshold: f64, settings: Settings, requested: &mut Vec<Vec<f64>>) -> Outcome {
    let gate = gated_bowl(threshold);
    run(
        &[(-2.0, 2.0), (-2.0, 2.0)],
        &[],
        settings,
        Instant::now(),
        &CancelScope::attach(None),
        &mut |points: &[Vec<f64>]| {
            requested.extend_from_slice(points);
            points.iter().map(|point| gate(point)).collect()
        },
    )
}

#[test]
fn rejections_never_count_against_the_budget_whatever_their_rate() {
    for reject_rate in [0.0, 0.5, 0.9] {
        let threshold = -2.0 + 4.0 * reject_rate;
        let mut requested = Vec::new();
        let outcome = run_gated(threshold, settings(3, 400), &mut requested);
        let analysed = requested.iter().filter(|p| p[0] >= threshold).count();
        assert_eq!((outcome.evaluations, analysed), (400, 400), "{reject_rate}");
        assert_eq!(outcome.rejected, requested.len() - analysed);
        assert_eq!(outcome.termination, Termination::EvaluationBudget);
    }
}

#[test]
fn the_rejection_cap_stops_the_run_within_one_generation() {
    let mut requested = Vec::new();
    let outcome = run_gated(
        1.6,
        Settings {
            max_rejects: 30,
            ..settings(3, 400)
        },
        &mut requested,
    );
    assert_eq!(outcome.termination, Termination::PregateExhausted);
    assert!(outcome.evaluations < 400);
    // Checked between generations: overrun by at most one generation.
    assert!(
        (30..30 + 20).contains(&outcome.rejected),
        "{}",
        outcome.rejected
    );
}

#[test]
fn the_time_limit_is_checked_between_generations_only() {
    // How many generations fit depends on machine load, so the test checks
    // the guard's rule, not a count: a generation only starts before the
    // limit, and every generation that starts is evaluated whole.
    let limit = std::time::Duration::from_millis(160);
    let started = Instant::now();
    let mut batches = Vec::new();
    let mut batch_starts = Vec::new();
    let outcome = run(
        &[(-2.0, 2.0), (-2.0, 2.0)],
        &[],
        Settings {
            time_limit: Some(limit),
            ..settings(1, 100_000)
        },
        started,
        &CancelScope::attach(None),
        &mut |points: &[Vec<f64>]| {
            batch_starts.push(started.elapsed());
            batches.push(points.len());
            std::thread::sleep(std::time::Duration::from_millis(50));
            points.iter().map(|p| bowl_behind_a_halfplane(p)).collect()
        },
    );
    assert_eq!(outcome.termination, Termination::TimeBudget);
    assert!(!batches.is_empty());
    assert_eq!(outcome.evaluations, batches.iter().sum::<usize>());
    assert!(
        batch_starts.iter().all(|start| *start < limit),
        "a generation started after the limit: {batch_starts:?}"
    );
}

#[test]
fn feasibility_first_ordering_follows_the_tiers() {
    // Cost falls towards x = 0, which is infeasible; the tiers rank a closed
    // infeasible design ahead of an unclosed or pre-gate-rejected one
    // whatever their objective.
    let tiered = |values: &[f64]| {
        let x = values[0];
        let tier = match x {
            x if x >= 0.6 => Tier::Feasible,
            x if x >= 0.4 => Tier::ClosedInfeasible,
            x if x >= 0.2 => Tier::NotClosed,
            _ => Tier::PreGateFailed,
        };
        ScoredPoint {
            values: values.to_vec(),
            cost: x,
            tier,
            constraint_violation: if tier == Tier::Feasible { 0.0 } else { 1.0 - x },
            objectives: [x; 3],
        }
    };
    for (upper, expected) in [
        (1.0, Tier::Feasible),
        (0.59, Tier::ClosedInfeasible),
        (0.39, Tier::NotClosed),
        (0.19, Tier::PreGateFailed),
    ] {
        let outcome = run(
            &[(0.0, upper)],
            &[],
            settings(4, 200),
            Instant::now(),
            &CancelScope::attach(None),
            &mut batch_of(tiered, &mut Vec::new()),
        );
        assert_eq!(outcome.winner.tier, expected, "box [0, {upper}]");
        if expected == Tier::Feasible {
            assert!(outcome.winner.values[0] >= 0.6);
        }
    }
    let mut keys = [
        tiered(&[0.1]),
        tiered(&[0.9]),
        tiered(&[0.3]),
        tiered(&[0.5]),
    ];
    keys.sort_by_key(ScoredPoint::feasibility_key);
    let order: Vec<Tier> = keys.iter().map(|p| p.tier).collect();
    assert_eq!(
        order,
        [
            Tier::Feasible,
            Tier::ClosedInfeasible,
            Tier::NotClosed,
            Tier::PreGateFailed
        ]
    );
}

#[test]
fn an_equal_trial_replaces_its_parent() {
    // On a flat plateau every trial ties its parent. With replacement on
    // `<=` the second generation's trials are built on the first
    // generation's trials: the coordinates crossover leaves untouched equal
    // the replaced parent, never the original initial member.
    let flat = |values: &[f64]| scored(values, 1.0, 0.0);
    let mut batches: Vec<Vec<Vec<f64>>> = Vec::new();
    run(
        &[(0.0, 1.0); 10],
        &[],
        Settings {
            population: 8,
            ..settings(5, 24)
        },
        Instant::now(),
        &CancelScope::attach(None),
        &mut |points: &[Vec<f64>]| {
            batches.push(points.to_vec());
            points.iter().map(|p| flat(p)).collect()
        },
    );
    assert_eq!(batches.len(), 3);
    // Only coordinates the first generation changed can tell the two
    // parents apart.
    let (mut from_trial, mut from_initial) = (0, 0);
    for ((initial, first), second) in batches[0].iter().zip(&batches[1]).zip(&batches[2]) {
        for j in 0..initial.len() {
            if first[j] != initial[j] {
                from_trial += usize::from(second[j] == first[j]);
                from_initial += usize::from(second[j] == initial[j]);
            }
        }
    }
    assert!(from_trial > 0);
    assert_eq!(from_initial, 0);
}

#[test]
fn with_nothing_feasible_the_reserve_is_left_for_restoration() {
    let infeasible = |values: &[f64]| scored(values, values[0], 1.0 + values[0]);
    let outcome = run(
        &[(0.0, 1.0), (0.0, 1.0)],
        &[],
        Settings {
            infeasible_reserve: 40,
            ..settings(2, 200)
        },
        Instant::now(),
        &CancelScope::attach(None),
        &mut batch_of(infeasible, &mut Vec::new()),
    );
    assert!(!outcome.winner.valid());
    assert!(outcome.evaluations <= 160, "{}", outcome.evaluations);
    assert!(outcome.evaluations > 140, "{}", outcome.evaluations);
    // The least-violating candidate wins.
    assert!(outcome.winner.constraint_violation < 1.05);
}

#[test]
fn seeds_enter_the_initial_population_first_and_repaired_into_the_box() {
    let mut first = Vec::new();
    run(
        &[(0.0, 1.0), (5.0, 5.0)],
        &[vec![0.25, 5.0], vec![7.0, 9.0]],
        settings(8, 30),
        Instant::now(),
        &CancelScope::attach(None),
        &mut |points: &[Vec<f64>]| {
            if first.is_empty() {
                first = points.to_vec();
            }
            points.iter().map(|p| scored(p, p[0], 0.0)).collect()
        },
    );
    assert_eq!(first.len(), 20);
    assert_eq!(first[0], vec![0.25, 5.0]);
    assert_eq!(first[1], vec![1.0, 5.0]);
    assert!(first
        .iter()
        .all(|p| (0.0..=1.0).contains(&p[0]) && p[1] == 5.0));
}

#[test]
fn a_cancellation_during_a_generation_keeps_its_scores_and_stops() {
    let watch = CancelWatch::new();
    let mut calls = 0;
    let outcome = run(
        &[(-2.0, 2.0), (-2.0, 2.0)],
        &[],
        settings(6, 1_000),
        Instant::now(),
        &CancelScope::attach(Some(watch.flag())),
        &mut |points: &[Vec<f64>]| {
            calls += 1;
            if calls == 3 {
                watch.request_cancellation();
            }
            points.iter().map(|p| bowl_behind_a_halfplane(p)).collect()
        },
    );
    assert_eq!(outcome.termination, Termination::Cancelled);
    assert_eq!(calls, 3);
    assert_eq!(outcome.generations_completed, 1);
}

#[test]
fn stagnation_stops_as_converged_only_when_the_population_has_collapsed() {
    let bowl = |values: &[f64]| scored(values, values[0].powi(2) + values[1].powi(2), 0.0);
    let run_with = |tolerance| {
        run(
            &[(-1.0, 1.0), (-1.0, 1.0)],
            &[],
            Settings {
                spread_tolerance: tolerance,
                stagnation_generations: 5,
                ..settings(4, 20_000)
            },
            Instant::now(),
            &CancelScope::attach(None),
            &mut batch_of(bowl, &mut Vec::new()),
        )
    };
    let converged = run_with(0.5);
    assert_eq!(converged.termination, Termination::Converged);
    assert!(converged.evaluations < 20_000);
    let stagnated = run_with(0.0);
    assert_eq!(stagnated.termination, Termination::Stagnated);
}

#[test]
fn stagnation_never_stops_a_run_before_its_minimum_budget_share() {
    // A flat objective stalls from the first generation; a one-generation
    // window would stop it at once without the budget-share rule.
    let flat = |values: &[f64]| scored(values, 1.0, 0.0);
    for budget in [300, 1_000, 4_000] {
        let outcome = run(
            &[(-1.0, 1.0), (-1.0, 1.0)],
            &[],
            Settings {
                stagnation_generations: 1,
                ..settings(9, budget)
            },
            Instant::now(),
            &CancelScope::attach(None),
            &mut batch_of(flat, &mut Vec::new()),
        );
        assert_eq!(outcome.termination, Termination::Stagnated, "{budget}");
        let spent = outcome.evaluations as f64 / budget as f64;
        assert!(
            spent >= ops::STAGNATION_MINIMUM_BUDGET_FRACTION,
            "{budget}: {spent}"
        );
        assert!(outcome.evaluations < budget, "{budget}");
    }
    // The window grows with the initial population against the floor.
    for (initial, minimum) in [(24_usize, 8_usize), (78, 8), (200, 8)] {
        let window = (2 * initial).div_ceil(minimum);
        let stops = |stalled| ops::stagnation_stops(stalled, (5, initial, minimum), 900, 1_000);
        assert!(!stops(window - 1) && stops(window), "{initial}");
    }
}

#[test]
fn memory_updates_use_lehmer_means_and_skip_generations_without_success() {
    let mut memory = Memory {
        f: [INITIAL_MEMORY; MEMORY_SIZE],
        cr: [Some(INITIAL_MEMORY); MEMORY_SIZE],
        next: 0,
    };
    memory.update(&[]);
    assert_eq!(memory.next, 0, "no success leaves the memory unchanged");
    memory.update(&[(0.4, 0.2, 1.0), (0.8, 0.6, 3.0)]);
    // Weighted Lehmer: (0.25*0.16 + 0.75*0.64) / (0.25*0.4 + 0.75*0.8).
    assert!((memory.f[0] - 0.52 / 0.7).abs() < 1e-12);
    assert!(
        (memory.cr[0].unwrap() - (0.25 * 0.04 + 0.75 * 0.36) / (0.25 * 0.2 + 0.75 * 0.6)).abs()
            < 1e-12
    );
    assert_eq!(memory.next, 1);
    // All-zero successful CR fixes that slot at the terminal value for good.
    memory.next = 0;
    memory.update(&[(0.5, 0.0, 1.0)]);
    assert_eq!(memory.cr[0], None);
    memory.next = 0;
    memory.update(&[(0.5, 0.7, 1.0)]);
    assert_eq!(memory.cr[0], None);
    assert_eq!(ops::sample_cr(None, &mut SearchRng::seed(1)), 0.0);
}

#[test]
fn the_epsilon_level_decays_to_zero_at_the_control_budget() {
    assert_eq!(ops::epsilon_schedule(1.0, 0, 100), 1.0);
    assert_eq!(ops::epsilon_schedule(1.0, 100, 100), 0.0);
    assert!((ops::epsilon_schedule(1.0, 50, 100) - 0.5_f64.powi(5)).abs() < 1e-15);
    let inside = scored(&[0.0], 5.0, 0.1);
    let outside = scored(&[0.0], 1.0, 0.3);
    assert!(ops::epsilon_key(&inside, 0.2) < ops::epsilon_key(&outside, 0.2));
    let cheaper_inside = scored(&[0.0], 1.0, 0.15);
    assert!(ops::epsilon_key(&cheaper_inside, 0.2) < ops::epsilon_key(&inside, 0.2));
    assert!(ops::epsilon_key(&cheaper_inside, 0.0) > ops::epsilon_key(&inside, 0.0));
}

#[test]
fn midpoint_repair_lands_between_the_crossed_bound_and_the_parent() {
    assert_eq!(ops::repair_midpoint(1.5, 0.6, (0.0, 1.0)), 0.8);
    assert_eq!(ops::repair_midpoint(-0.5, 0.2, (0.0, 1.0)), 0.1);
    assert_eq!(ops::repair_midpoint(0.5, 0.2, (0.0, 1.0)), 0.5);
    assert_eq!(ops::repair_midpoint(f64::NAN, 0.3, (0.0, 1.0)), 0.3);
}
