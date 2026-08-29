// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Feasibility-first Differential Evolution for constrained product searches.
//!
//! Candidate replacement follows J. Lampinen, "A Constraint Handling
//! Approach for the Differential Evolution Algorithm," CEC 2002,
//! DOI 10.1109/CEC.2002.1004459: feasibility precedes objective quality, and
//! aggregate violation orders two infeasible candidates.

use crate::python_rng::RandomState;

use super::{EvaluatePoint, MethodOutcome, ScoredPoint};

pub(crate) fn run_feasibility_first_de(
    bounds: &[(f64, f64)],
    population_size: usize,
    generations: usize,
    seed: u64,
    initial_design: Option<&[f64]>,
    evaluate: &mut EvaluatePoint<'_>,
) -> MethodOutcome {
    let population_size = population_size.max(6);
    let mut rng = RandomState::seed(seed);
    let mut population = latin_hypercube(bounds, population_size, &mut rng);
    if let Some(initial) = initial_design.filter(|values| in_bounds(values, bounds)) {
        population[0] = initial.to_vec();
    }
    let mut scored: Vec<ScoredPoint> = population.iter().map(|x| evaluate(x)).collect();
    promote_best(&mut population, &mut scored);

    let mut indices: Vec<usize> = (0..population_size).collect();
    for _ in 0..generations {
        let mutation = rng.uniform(0.5, 1.0);
        for candidate in 0..population_size {
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
                    trial[dimension] = if (lower..=upper).contains(&mutant) {
                        mutant
                    } else {
                        rng.uniform(lower, upper)
                    };
                }
            }

            let trial_score = evaluate(&trial);
            if preferred(&trial_score, &scored[candidate]) {
                population[candidate] = trial;
                scored[candidate] = trial_score;
            }
        }
        promote_best(&mut population, &mut scored);
    }

    MethodOutcome {
        winner: scored[0].clone(),
        pareto_front: Vec::new(),
    }
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

fn in_bounds(values: &[f64], bounds: &[(f64, f64)]) -> bool {
    values.len() == bounds.len()
        && values
            .iter()
            .zip(bounds)
            .all(|(&value, &(lower, upper))| (lower..=upper).contains(&value))
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
        let run = || run_feasibility_first_de(&[(0.0, 1.0)], 8, 5, 42, Some(&[0.7]), &mut point);
        let first = run();
        let second = run();
        assert_eq!(first, second);
        assert!(first.winner.valid);
        assert!((0.0..=1.0).contains(&first.winner.values[0]));
    }
}
