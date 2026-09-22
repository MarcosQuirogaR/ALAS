// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Feasibility-first Differential Evolution for constrained product searches.
//!
//! Candidate replacement follows J. Lampinen, "A Constraint Handling
//! Approach for the Differential Evolution Algorithm," CEC 2002,
//! DOI 10.1109/CEC.2002.1004459: feasibility precedes objective quality, and
//! aggregate violation orders two infeasible candidates.

use crate::cancellation::{CancelPhase, CancelScope};
use crate::python_rng::RandomState;

use super::{product_de, EvaluatePoint, MethodOutcome, ScoredPoint};

/// Run feasibility-first Differential Evolution.
///
/// # Cancellation granularity
///
/// The flag is read **immediately before every candidate evaluation**, in the
/// initial population and in every generation alike, so the bound on stopping
/// is one coupled analysis rather than one generation. That distinction is the
/// whole difference between a responsive cancel and an unresponsive one here:
/// the sixteen-variable product space at the `quick_draft` multiplier runs 64
/// candidates per generation and evaluates them one at a time, so a
/// per-generation check made the bound sixty-four analyses - measured at
/// 28.76 s on the 2026-09-22 A320-200 smoke - where a per-evaluation check
/// makes it one.
///
/// Stopping mid-generation is safe because a candidate is replaced atomically:
/// `population[i]` and `scored[i]` are written together or not at all, so the
/// population is a set of fully scored points at every observation point, and
/// a half-built trial vector can never be returned. The best is re-promoted
/// before returning, so the winner is the best point actually analysed. Work
/// already produced is kept: a run cancelled in generation five returns
/// generation five's population, not nothing.
///
/// The initial population may also be cut short. Its scored prefix is kept and
/// ranked; if not even one candidate was analysed the winner is the explicit
/// unevaluated sentinel, which is worst on every ordering key and can never be
/// mistaken for a design.
///
/// The returned `bool` reports whether the run stopped on the flag rather than
/// by exhausting `generations`. For an uncancelled run every check is a
/// relaxed load that changes nothing, so a seeded replay is bit-identical to
/// the uninstrumented search.
pub(crate) fn run_feasibility_first_de(
    bounds: &[(f64, f64)],
    population_size: usize,
    generations: usize,
    seed: u64,
    initial_design: Option<&[f64]>,
    scope: &CancelScope<'_>,
    evaluate: &mut EvaluatePoint<'_>,
) -> (MethodOutcome, bool) {
    let population_size = population_size.max(6);
    let mut rng = RandomState::seed(seed);
    let mut population = latin_hypercube(bounds, population_size, &mut rng);
    if let Some(initial) = initial_design.filter(|values| values.len() == bounds.len()) {
        let mut seeded = initial.to_vec();
        clamp_into_bounds(&mut seeded, bounds);
        population[0] = seeded;
    }
    let nothing_analysed = population.first().cloned().unwrap_or_default();

    scope.enter(CancelPhase::DeInitialPopulation, 0);
    let mut cancelled = false;
    let mut scored: Vec<ScoredPoint> = Vec::with_capacity(population_size);
    for candidate in &population {
        if scope.requested() {
            cancelled = true;
            scope.work_skipped(format!(
                "initial population stopped after {} of {population_size} candidates",
                scored.len()
            ));
            break;
        }
        scored.push(scope.evaluation(|| evaluate(candidate)));
    }
    if cancelled {
        // Only the scored prefix is a population; the rest was never
        // analysed and must not be ranked as if it had been.
        population.truncate(scored.len());
        promote_best(&mut population, &mut scored);
        return (
            MethodOutcome {
                winner: scored
                    .first()
                    .cloned()
                    .unwrap_or_else(|| product_de::unevaluated(&nothing_analysed)),
                pareto_front: Vec::new(),
            },
            true,
        );
    }
    promote_best(&mut population, &mut scored);

    let mut indices: Vec<usize> = (0..population_size).collect();
    'generations: for generation in 0..generations {
        scope.enter(CancelPhase::DeGeneration, generation as u64);
        let mutation = rng.uniform(0.5, 1.0);
        for candidate in 0..population_size {
            if scope.requested() {
                cancelled = true;
                scope.work_skipped(format!(
                    "generation {generation} stopped after {candidate} of {population_size} trials"
                ));
                break 'generations;
            }
            shuffle(&mut indices, &mut rng);
            let samples: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&index| index != candidate)
                .take(2)
                .collect();
            let forced = rng.randint(bounds.len());
            let mut trial = population[candidate].clone();
            for dimension in 0..bounds.len() {
                if dimension == forced || rng.uniform(0.0, 1.0) < 0.7 {
                    let mutant = population[0][dimension]
                        + mutation
                            * (population[samples[0]][dimension]
                                - population[samples[1]][dimension]);
                    let (lower, upper) = bounds[dimension];
                    trial[dimension] = reflect_into_bounds(mutant, lower, upper);
                }
            }

            let trial_score = scope.evaluation(|| evaluate(&trial));
            if preferred(&trial_score, &scored[candidate]) {
                population[candidate] = trial;
                scored[candidate] = trial_score;
            }
        }
        promote_best(&mut population, &mut scored);
        if scope.requested() {
            cancelled = true;
            break;
        }
    }
    // Idempotent when the loop completed a generation; it is what makes a
    // mid-generation stop still return the best point analysed.
    promote_best(&mut population, &mut scored);

    (
        MethodOutcome {
            winner: scored
                .first()
                .cloned()
                .unwrap_or_else(|| product_de::unevaluated(&nothing_analysed)),
            pareto_front: Vec::new(),
        },
        cancelled,
    )
}

fn preferred(left: &ScoredPoint, right: &ScoredPoint) -> bool {
    left.feasibility_key() < right.feasibility_key()
}

fn promote_best(population: &mut [Vec<f64>], scored: &mut [ScoredPoint]) {
    let Some((best, _)) = scored
        .iter()
        .enumerate()
        .min_by_key(|(_, point)| point.feasibility_key())
    else {
        return;
    };
    population.swap(0, best);
    scored.swap(0, best);
}

/// Fold `value` back inside `[lower, upper]` by reflecting it at whichever
/// bound it crossed.
///
/// Repair keeps the search inside the declared envelope without discarding
/// the direction the difference vector proposed: a component that overshoots
/// by a little lands a little inside the bound it passed, rather than being
/// scattered uniformly across the envelope. The fold is a pure function of
/// the mutant, so a seeded run replays exactly; resampling would have made
/// repair consume the generator and couple the result to how often a mutant
/// happened to leave the box.
///
/// A degenerate bound, where the width is not positive, admits only `lower`.
/// A non-finite mutant cannot come from finite parents and a finite mutation
/// factor; it fails closed to `lower` rather than propagating.
fn reflect_into_bounds(value: f64, lower: f64, upper: f64) -> f64 {
    let width = upper - lower;
    if !value.is_finite() || !width.is_finite() || width <= 0.0 {
        return lower;
    }
    // A reflection at both bounds is periodic over twice the width: the
    // first half of each period runs up from `lower`, the second half runs
    // back down to it.
    let span = 2.0 * width;
    let offset = (value - lower).rem_euclid(span);
    let folded = if offset <= width {
        offset
    } else {
        span - offset
    };
    (lower + folded).clamp(lower, upper)
}

/// Bring every component of `values` inside `bounds` by clamping.
///
/// Used for a caller-supplied starting point, where the nearest admissible
/// design is the honest repair: the point was chosen deliberately, so a
/// reflection that moved it away from the bound it sits against would
/// misrepresent the request.
fn clamp_into_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = if value.is_finite() {
            value.clamp(lower, upper)
        } else {
            lower
        };
    }
}

fn latin_hypercube(
    bounds: &[(f64, f64)],
    population_size: usize,
    rng: &mut RandomState,
) -> Vec<Vec<f64>> {
    let mut population = vec![vec![0.0; bounds.len()]; population_size];
    for dimension in 0..bounds.len() {
        let mut order: Vec<usize> = (0..population_size).collect();
        shuffle(&mut order, rng);
        for row in 0..population_size {
            let normalized = (order[row] as f64 + rng.uniform(0.0, 1.0)) / population_size as f64;
            let (lower, upper) = bounds[dimension];
            population[row][dimension] = lower + normalized * (upper - lower);
        }
    }
    population
}

fn shuffle<T>(values: &mut [T], rng: &mut RandomState) {
    for position in (1..values.len()).rev() {
        let swap_with = rng.randint(position + 1);
        values.swap(position, swap_with);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancellation::CancelWatch;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn point(values: &[f64]) -> ScoredPoint {
        let x = values[0];
        let valid = x >= 0.4;
        ScoredPoint {
            values: values.to_vec(),
            cost: x * x,
            valid,
            constraint_violation: (0.4 - x).max(0.0),
            objectives: [x * x, x, x],
        }
    }

    #[test]
    fn a_feasible_candidate_beats_a_cheaper_infeasible_candidate() {
        let feasible = point(&[0.5]);
        let infeasible = point(&[0.0]);
        assert!(preferred(&feasible, &infeasible));
    }

    #[test]
    fn the_seeded_search_is_bounded_and_repeatable() {
        let run = || {
            run_feasibility_first_de(
                &[(0.0, 1.0)],
                8,
                5,
                42,
                Some(&[0.7]),
                &CancelScope::attach(None),
                &mut point,
            )
        };
        let (first, first_cancelled) = run();
        let (second, second_cancelled) = run();
        assert_eq!(first, second);
        assert!(!first_cancelled && !second_cancelled);
        assert!(first.winner.valid);
        assert!((0.0..=1.0).contains(&first.winner.values[0]));
    }

    #[test]
    fn a_cancellation_observed_after_one_generation_stops_before_the_generation_budget() {
        let bounds = [(0.0, 1.0)];
        let population = 8usize;
        let generations = 40usize;
        let cancel = AtomicBool::new(false);
        let mut calls = 0usize;
        let mut evaluate = |values: &[f64]| {
            calls += 1;
            // The initial population (`population` calls) plus the first
            // generation's trials (`population` more) have all been scored
            // by this point; request cancellation right at that boundary.
            if calls == population * 2 {
                cancel.store(true, Ordering::Relaxed);
            }
            point(values)
        };

        let (outcome, cancelled) = run_feasibility_first_de(
            &bounds,
            population,
            generations,
            7,
            None,
            &CancelScope::attach(Some(&cancel)),
            &mut evaluate,
        );

        assert!(
            cancelled,
            "the search must report that it stopped on the cancellation signal"
        );
        assert!(outcome.winner.valid);
        assert!((0.0..=1.0).contains(&outcome.winner.values[0]));
        assert!(
            calls < population * (1 + generations),
            "cancellation must stop well short of the {generations}-generation budget: \
             {calls} evaluations"
        );
    }

    /// The bound this task exists to shrink: a flag set in the middle of a
    /// generation must cost at most one more evaluation, not the rest of the
    /// generation.
    #[test]
    fn a_cancellation_mid_generation_costs_exactly_one_more_evaluation() {
        let bounds = [(0.0, 1.0)];
        let population = 16usize;
        let cancel = AtomicBool::new(false);
        // Three trials into the second generation: not a boundary under any
        // per-generation reading of the flag.
        let request_at = population * 2 + 3;
        let mut calls = 0usize;
        let mut evaluate = |values: &[f64]| {
            calls += 1;
            if calls == request_at {
                cancel.store(true, Ordering::Relaxed);
            }
            point(values)
        };

        let (outcome, cancelled) = run_feasibility_first_de(
            &bounds,
            population,
            40,
            11,
            None,
            &CancelScope::attach(Some(&cancel)),
            &mut evaluate,
        );

        assert!(cancelled);
        assert_eq!(
            calls, request_at,
            "the flag must be read before the next evaluation starts, so the evaluation that \
             set it is the last one"
        );
        assert!(
            outcome.winner.valid,
            "a mid-generation stop still returns the best fully scored candidate: {:?}",
            outcome.winner
        );
    }

    #[test]
    fn a_cancellation_during_the_initial_population_keeps_the_scored_prefix() {
        let bounds = [(0.0, 1.0)];
        let population = 12usize;
        let cancel = AtomicBool::new(false);
        let mut calls = 0usize;
        let mut evaluate = |values: &[f64]| {
            calls += 1;
            if calls == 4 {
                cancel.store(true, Ordering::Relaxed);
            }
            point(values)
        };

        let (outcome, cancelled) = run_feasibility_first_de(
            &bounds,
            population,
            8,
            3,
            None,
            &CancelScope::attach(Some(&cancel)),
            &mut evaluate,
        );

        assert!(cancelled);
        assert_eq!(calls, 4, "the initial population must stop on the flag too");
        assert!(
            outcome.winner.cost.is_finite(),
            "the four candidates already analysed are kept and ranked: {:?}",
            outcome.winner
        );
    }

    #[test]
    fn a_flag_already_set_analyses_nothing_and_returns_the_unevaluated_sentinel() {
        let bounds = [(0.0, 1.0)];
        let cancel = AtomicBool::new(true);
        let mut calls = 0usize;
        let mut evaluate = |values: &[f64]| {
            calls += 1;
            point(values)
        };

        let (outcome, cancelled) = run_feasibility_first_de(
            &bounds,
            8,
            5,
            1,
            Some(&[0.7]),
            &CancelScope::attach(Some(&cancel)),
            &mut evaluate,
        );

        assert!(cancelled);
        assert_eq!(calls, 0, "no analysis may start under an already-set flag");
        assert!(
            !outcome.winner.valid && outcome.winner.cost.is_infinite(),
            "with nothing analysed the winner must be the sentinel: {:?}",
            outcome.winner
        );
    }

    #[test]
    fn telemetry_records_the_de_phase_the_request_landed_in() {
        let bounds = [(0.0, 1.0)];
        let population = 8usize;
        let watch = CancelWatch::new();
        let mut calls = 0usize;
        let mut evaluate = |values: &[f64]| {
            calls += 1;
            if calls == population * 2 + 2 {
                watch.request_cancellation();
            }
            point(values)
        };

        let scope = CancelScope::attach(Some(watch.flag()));
        let (_, cancelled) =
            run_feasibility_first_de(&bounds, population, 40, 5, None, &scope, &mut evaluate);
        scope.search_finished("cancelled");

        assert!(cancelled);
        let snapshot = watch.snapshot();
        assert_eq!(snapshot.requested_during, CancelPhase::DeGeneration);
        assert_eq!(
            snapshot.requested_during_index, 1,
            "the request landed in the second generation, counted from zero"
        );
        assert_eq!(snapshot.evaluations_completed as usize, calls);
        assert_eq!(
            snapshot.evaluations_after_request, 1,
            "exactly the evaluation in flight when the request arrived finishes after it; \
             no further evaluation may start"
        );
        assert_eq!(
            snapshot.stop_reason,
            Some(crate::cancellation::StopReason::Cancelled)
        );
    }

    /// An uncancelled run must be unchanged by the instrumentation: same
    /// winner, same evaluation count, same order.
    #[test]
    fn instrumentation_does_not_disturb_an_uncancelled_seeded_replay() {
        let bounds = [(0.0, 1.0), (-1.0, 2.0)];
        let run = |scope: &CancelScope<'_>| {
            let mut seen = Vec::new();
            let mut evaluate = |values: &[f64]| {
                seen.push(values.to_vec());
                point(values)
            };
            let (outcome, cancelled) = run_feasibility_first_de(
                &bounds,
                10,
                4,
                99,
                Some(&[0.5, 0.5]),
                scope,
                &mut evaluate,
            );
            assert!(!cancelled);
            (outcome, seen)
        };
        let watch = CancelWatch::new();
        let (instrumented, instrumented_points) = run(&CancelScope::attach(Some(watch.flag())));
        let (bare, bare_points) = run(&CancelScope::attach(None));
        assert_eq!(instrumented, bare);
        assert_eq!(instrumented_points, bare_points);
        assert_eq!(
            watch.snapshot().evaluations_completed as usize,
            bare_points.len()
        );
    }

    #[test]
    fn repair_reflects_an_overshoot_back_inside_and_leaves_an_interior_value_alone() {
        // Inside the box, repair is the identity.
        for value in [0.0, 0.25, 1.0] {
            assert_eq!(reflect_into_bounds(value, 0.0, 1.0), value);
        }
        // Just past a bound, the value lands just inside it, on the side it
        // crossed, rather than anywhere in the box.
        assert!((reflect_into_bounds(1.1, 0.0, 1.0) - 0.9).abs() < 1e-12);
        assert!((reflect_into_bounds(-0.1, 0.0, 1.0) - 0.1).abs() < 1e-12);
        // An overshoot of more than the whole width still terminates inside.
        for value in [12.7, -12.7, 1.0e12, -1.0e12] {
            let folded = reflect_into_bounds(value, 0.0, 1.0);
            assert!((0.0..=1.0).contains(&folded), "{value} -> {folded}");
        }
        // Degenerate and non-finite inputs fail closed to the lower bound
        // instead of propagating.
        assert_eq!(reflect_into_bounds(f64::NAN, 0.0, 1.0), 0.0);
        assert_eq!(reflect_into_bounds(f64::INFINITY, 0.0, 1.0), 0.0);
        assert_eq!(reflect_into_bounds(0.5, 1.0, 1.0), 1.0);
    }

    #[test]
    fn an_out_of_bounds_starting_point_is_repaired_into_the_population() {
        let bounds = [(2.0, 5.0)];
        let mut seeded = vec![900.0];
        clamp_into_bounds(&mut seeded, &bounds);
        assert_eq!(seeded, vec![5.0]);

        let mut nonfinite = vec![f64::NAN];
        clamp_into_bounds(&mut nonfinite, &bounds);
        assert_eq!(nonfinite, vec![2.0]);

        // The search accepts the same starting point rather than dropping it.
        let mut evaluate = |values: &[f64]| ScoredPoint {
            values: values.to_vec(),
            cost: values[0],
            valid: true,
            constraint_violation: 0.0,
            objectives: [values[0], 0.0, 0.0],
        };
        let (outcome, cancelled) = run_feasibility_first_de(
            &bounds,
            6,
            0,
            3,
            Some(&[900.0]),
            &CancelScope::attach(None),
            &mut evaluate,
        );
        assert!(!cancelled);
        assert!((2.0..=5.0).contains(&outcome.winner.values[0]));
    }
}
