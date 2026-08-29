// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded local-surrogate trust-region optimization for expensive analyses.
//!
//! This dependency-free product method follows the local-region allocation
//! principle of D. Eriksson et al., "Scalable Global Optimization via Local
//! Bayesian Optimization," NeurIPS 2019. It uses a fixed radial-basis local
//! surrogate rather than claiming equivalence to the paper's trained Gaussian
//! processes; the GUI therefore identifies the method as a TuRBO-1 variant.

use crate::python_rng::RandomState;

use super::{normalized_distance_squared, EvaluatePoint, MethodOutcome, ScoredPoint};

pub(crate) fn run_turbo_1(
    bounds: &[(f64, f64)],
    population_size: usize,
    generations: usize,
    seed: u64,
    initial_design: Option<&[f64]>,
    evaluate: &mut EvaluatePoint<'_>,
) -> MethodOutcome {
    let batch_size = population_size.max(2);
    let mut rng = RandomState::seed(seed);
    let mut observations = Vec::new();
    if let Some(initial) = initial_design.filter(|values| in_bounds(values, bounds)) {
        observations.push(evaluate(initial));
    }
    while observations.len() < batch_size {
        let sample = random_point(bounds, &mut rng);
        observations.push(evaluate(&sample));
    }

    let mut best = best_point(&observations).clone();
    let mut trust_length = 0.8_f64;
    let mut successes = 0_usize;
    let mut failures = 0_usize;

    for _ in 0..generations {
        let pool_size = (batch_size * 8).max(64);
        let mut pool = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let candidate = trust_region_point(&best.values, bounds, trust_length, &mut rng);
            let acquisition = local_acquisition(&candidate, &observations, bounds);
            pool.push((acquisition, candidate));
        }
        pool.sort_by(|left, right| left.0.total_cmp(&right.0));

        let before = best.feasibility_key();
        for (_, candidate) in pool.into_iter().take(batch_size) {
            let score = evaluate(&candidate);
            if score.feasibility_key() < best.feasibility_key() {
                best = score.clone();
            }
            observations.push(score);
        }

        if best.feasibility_key() < before {
            successes += 1;
            failures = 0;
            if successes >= 3 {
                trust_length = (trust_length * 2.0).min(1.0);
                successes = 0;
            }
        } else {
            failures += 1;
            successes = 0;
            if failures >= 3 {
                trust_length = (trust_length * 0.5).max(0.02);
                failures = 0;
            }
        }
    }

    MethodOutcome {
        winner: best,
        pareto_front: Vec::new(),
    }
}

fn best_point(points: &[ScoredPoint]) -> &ScoredPoint {
    points
        .iter()
        .min_by_key(|point| point.feasibility_key())
        .unwrap_or(&points[0])
}

fn local_acquisition(
    candidate: &[f64],
    observations: &[ScoredPoint],
    bounds: &[(f64, f64)],
) -> f64 {
    let mut weighted_cost = 0.0;
    let mut total_weight = 0.0;
    let mut nearest = f64::INFINITY;
    for observation in observations.iter().rev().take(128) {
        let distance = normalized_distance_squared(candidate, &observation.values, bounds);
        nearest = nearest.min(distance);
        let weight = (-8.0 * distance).exp().max(1.0e-12);
        let feasibility_offset = if observation.valid {
            0.0
        } else {
            1.0e4 + observation.constraint_violation.max(0.0)
        };
        weighted_cost += weight * (observation.cost + feasibility_offset);
        total_weight += weight;
    }
    let mean = weighted_cost / total_weight.max(1.0e-12);
    mean - 0.25 * nearest.sqrt()
}

fn random_point(bounds: &[(f64, f64)], rng: &mut RandomState) -> Vec<f64> {
    bounds
        .iter()
        .map(|&(lower, upper)| rng.uniform(lower, upper))
        .collect()
}

fn trust_region_point(
    center: &[f64],
    bounds: &[(f64, f64)],
    length: f64,
    rng: &mut RandomState,
) -> Vec<f64> {
    center
        .iter()
        .zip(bounds)
        .map(|(&value, &(lower, upper))| {
            let radius = 0.5 * length * (upper - lower);
            rng.uniform((value - radius).max(lower), (value + radius).min(upper))
        })
        .collect()
}

fn in_bounds(values: &[f64], bounds: &[(f64, f64)]) -> bool {
    values.len() == bounds.len()
        && values
            .iter()
            .zip(bounds)
            .all(|(&value, &(lower, upper))| (lower..=upper).contains(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere(values: &[f64]) -> ScoredPoint {
        let cost = values.iter().map(|value| value * value).sum::<f64>();
        ScoredPoint {
            values: values.to_vec(),
            cost,
            valid: true,
            constraint_violation: 0.0,
            objectives: [cost, values[0].abs(), values[1].abs()],
        }
    }

    #[test]
    fn trust_region_proposals_stay_inside_every_bound() {
        let mut rng = RandomState::seed(42);
        for _ in 0..100 {
            let point = trust_region_point(&[0.0, 1.0], &[(-0.1, 0.1), (0.9, 1.1)], 1.0, &mut rng);
            assert!((-0.1..=0.1).contains(&point[0]));
            assert!((0.9..=1.1).contains(&point[1]));
        }
    }

    #[test]
    fn a_seeded_surrogate_search_is_deterministic() {
        let run = || {
            run_turbo_1(
                &[(-2.0, 2.0), (-2.0, 2.0)],
                4,
                4,
                42,
                Some(&[1.0, 1.0]),
                &mut sphere,
            )
        };
        assert_eq!(run(), run());
    }
}
