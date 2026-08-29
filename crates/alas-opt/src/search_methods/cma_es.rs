// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded, deterministic covariance-adapting evolution strategy.
//!
//! The implementation works in normalized coordinates, updates a full
//! rank-mu covariance matrix, and samples through a regularized Cholesky
//! factor. Candidates are clipped only at the physical boundary; the
//! covariance still adapts from the bounded samples, so this remains a CMA-ES
//! search rather than an independent random sampler with a moving center.

use std::cmp::Ordering;

use crate::python_rng::RandomState;

use super::{EvaluatePoint, MethodOutcome, ScoredPoint};

const MIN_COVARIANCE: f64 = 1.0e-12;
const INITIAL_SIGMA: f64 = 0.30;
const MIN_SIGMA: f64 = 1.0e-3;
const MAX_SIGMA: f64 = 1.0;

/// Run a bounded covariance-adapting evolution strategy.
pub(crate) fn run_cma_es(
    bounds: &[(f64, f64)],
    population_size: usize,
    generations: usize,
    seed: u64,
    initial_design: Option<&[f64]>,
    evaluate: &mut EvaluatePoint<'_>,
) -> MethodOutcome {
    let dimension = bounds.len();
    if dimension == 0 {
        let candidate = initial_design.unwrap_or(&[]);
        let point = evaluate(candidate);
        return MethodOutcome {
            winner: point.clone(),
            pareto_front: vec![point],
        };
    }

    let lambda = population_size.max(4);
    let mu = (lambda / 2).max(1);
    let weights = recombination_weights(mu);
    let covariance_learning_rate = (0.25 / dimension as f64).min(0.25);
    let mut rng = RandomState::seed(seed);
    let mut mean = normalized_initial_mean(bounds, initial_design);
    let mut covariance = identity_matrix(dimension);
    let mut sigma = INITIAL_SIGMA;
    let mut all_points = Vec::with_capacity(lambda * generations.max(1) + 1);

    let initial_values = to_physical(&mean, bounds);
    let initial_point = evaluate(&initial_values);
    all_points.push(initial_point.clone());
    let mut previous_best = initial_point;

    for _ in 0..generations {
        let cholesky =
            regularized_cholesky(&covariance).unwrap_or_else(|| identity_matrix(dimension));
        let old_mean = mean.clone();
        let mut generation = Vec::with_capacity(lambda);
        for _ in 0..lambda {
            let normal = standard_normal_vector(dimension, &mut rng);
            let correlated = lower_triangular_product(&cholesky, &normal);
            let unconstrained: Vec<f64> = old_mean
                .iter()
                .zip(correlated)
                .map(|(&center, step)| center + sigma * step)
                .collect();
            let bounded = unconstrained
                .iter()
                .map(|&value| value.clamp(0.0, 1.0))
                .collect::<Vec<_>>();
            let physical = to_physical(&bounded, bounds);
            let point = evaluate(&physical);
            generation.push((bounded, point));
        }

        generation.sort_by(|left, right| feasibility_order(&left.1, &right.1));
        let generation_best = generation
            .first()
            .map(|(_, point)| point.clone())
            .unwrap_or_else(|| previous_best.clone());
        let improved = feasibility_order(&generation_best, &previous_best) == Ordering::Less;

        let selected = generation.iter().take(mu).collect::<Vec<_>>();
        let next_mean = weighted_mean(&selected, &weights, dimension);
        let rank_mu = covariance_from_steps(&selected, &weights, &old_mean, sigma, dimension);
        covariance = blend_covariance(
            &covariance,
            &rank_mu,
            covariance_learning_rate,
            MIN_COVARIANCE,
        );
        mean = next_mean;
        sigma = if improved {
            (sigma * 1.20).min(MAX_SIGMA)
        } else {
            (sigma * 0.85).max(MIN_SIGMA)
        };

        all_points.extend(generation.into_iter().map(|(_, point)| point));
        previous_best = if improved {
            generation_best
        } else {
            previous_best
        };
    }

    let winner = all_points
        .iter()
        .min_by(|left, right| feasibility_order(left, right))
        .cloned()
        .unwrap_or_else(|| fallback_point(bounds));
    MethodOutcome {
        winner: winner.clone(),
        pareto_front: vec![winner],
    }
}

fn normalized_initial_mean(bounds: &[(f64, f64)], initial_design: Option<&[f64]>) -> Vec<f64> {
    bounds
        .iter()
        .enumerate()
        .map(|(index, &(lower, upper))| {
            let width = upper - lower;
            let fallback = 0.5;
            let value = initial_design
                .and_then(|design| design.get(index).copied())
                .filter(|value| value.is_finite())
                .map(|value| {
                    if width.is_finite() && width > 0.0 {
                        (value - lower) / width
                    } else {
                        0.0
                    }
                })
                .unwrap_or(fallback);
            value.clamp(0.0, 1.0)
        })
        .collect()
}

fn to_physical(normalized: &[f64], bounds: &[(f64, f64)]) -> Vec<f64> {
    normalized
        .iter()
        .zip(bounds)
        .map(|(&value, &(lower, upper))| {
            if upper.is_finite() && lower.is_finite() && upper > lower {
                lower + value.clamp(0.0, 1.0) * (upper - lower)
            } else {
                lower
            }
        })
        .collect()
}

fn recombination_weights(mu: usize) -> Vec<f64> {
    let mut weights: Vec<f64> = (0..mu)
        .map(|index| (mu as f64 + 0.5).ln() - ((index + 1) as f64).ln())
        .collect();
    let sum = weights.iter().sum::<f64>();
    if sum.is_finite() && sum > 0.0 {
        for weight in &mut weights {
            *weight /= sum;
        }
    } else {
        weights.fill(1.0 / mu as f64);
    }
    weights
}

fn weighted_mean(
    selected: &[&(Vec<f64>, ScoredPoint)],
    weights: &[f64],
    dimension: usize,
) -> Vec<f64> {
    let mut mean = vec![0.0; dimension];
    for (weight, (candidate, _)) in weights.iter().zip(selected) {
        for index in 0..dimension {
            mean[index] += weight * candidate[index];
        }
    }
    mean.into_iter()
        .map(|value| value.clamp(0.0, 1.0))
        .collect()
}

fn covariance_from_steps(
    selected: &[&(Vec<f64>, ScoredPoint)],
    weights: &[f64],
    old_mean: &[f64],
    sigma: f64,
    dimension: usize,
) -> Vec<Vec<f64>> {
    let denominator = sigma.max(MIN_SIGMA);
    let mut covariance = vec![vec![0.0; dimension]; dimension];
    for (weight, (candidate, _)) in weights.iter().zip(selected) {
        let step: Vec<f64> = candidate
            .iter()
            .zip(old_mean)
            .map(|(&value, &center)| (value - center) / denominator)
            .collect();
        for row in 0..dimension {
            for column in 0..=row {
                covariance[row][column] += weight * step[row] * step[column];
            }
        }
    }
    let lower_triangle = covariance.clone();
    for (row, row_values) in covariance.iter_mut().enumerate() {
        for (column, value) in row_values.iter_mut().enumerate().skip(row + 1) {
            *value = lower_triangle[column][row];
        }
    }
    covariance
}

fn blend_covariance(
    previous: &[Vec<f64>],
    sample: &[Vec<f64>],
    learning_rate: f64,
    minimum_diagonal: f64,
) -> Vec<Vec<f64>> {
    let dimension = previous.len();
    let mut covariance = vec![vec![0.0; dimension]; dimension];
    for (row, row_values) in covariance.iter_mut().enumerate() {
        for (column, value) in row_values.iter_mut().enumerate() {
            *value =
                (1.0 - learning_rate) * previous[row][column] + learning_rate * sample[row][column];
        }
    }
    let mut symmetric_covariance = vec![vec![0.0; dimension]; dimension];
    for (row, row_values) in covariance.iter().enumerate() {
        for (column, value) in row_values.iter().enumerate() {
            symmetric_covariance[row][column] = 0.5 * (*value + covariance[column][row]);
        }
    }
    for (row, row_values) in covariance.iter_mut().enumerate() {
        for (column, value) in row_values.iter_mut().enumerate() {
            *value = symmetric_covariance[row][column];
        }
        row_values[row] = row_values[row].max(minimum_diagonal);
    }
    covariance
}

fn identity_matrix(dimension: usize) -> Vec<Vec<f64>> {
    (0..dimension)
        .map(|row| {
            (0..dimension)
                .map(|column| if row == column { 1.0 } else { 0.0 })
                .collect()
        })
        .collect()
}

fn standard_normal_vector(dimension: usize, rng: &mut RandomState) -> Vec<f64> {
    let mut normal = Vec::with_capacity(dimension);
    while normal.len() < dimension {
        let first = 2.0 * rng.uniform(0.0, 1.0) - 1.0;
        let second = 2.0 * rng.uniform(0.0, 1.0) - 1.0;
        let radius = first * first + second * second;
        if radius <= 0.0 || radius >= 1.0 {
            continue;
        }
        let scale = (-2.0 * radius.ln() / radius).sqrt();
        normal.push(first * scale);
        if normal.len() < dimension {
            normal.push(second * scale);
        }
    }
    normal
}

fn lower_triangular_product(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    (0..matrix.len())
        .map(|row| {
            matrix[row]
                .iter()
                .take(row + 1)
                .zip(vector)
                .map(|(&coefficient, &value)| coefficient * value)
                .sum()
        })
        .collect()
}

fn regularized_cholesky(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let dimension = matrix.len();
    let mut jitter = 0.0;
    for _ in 0..12 {
        let mut factor = vec![vec![0.0; dimension]; dimension];
        let mut valid = true;
        for row in 0..dimension {
            for column in 0..=row {
                let mut value = matrix[row][column];
                if row == column {
                    value += jitter;
                }
                value -= (0..column)
                    .map(|index| factor[row][index] * factor[column][index])
                    .sum::<f64>();
                if row == column {
                    if !value.is_finite() || value <= MIN_COVARIANCE {
                        valid = false;
                        break;
                    }
                    factor[row][column] = value.sqrt();
                } else {
                    let diagonal = factor[column][column];
                    if !diagonal.is_finite() || diagonal <= 0.0 {
                        valid = false;
                        break;
                    }
                    factor[row][column] = value / diagonal;
                }
            }
            if !valid {
                break;
            }
        }
        if valid {
            return Some(factor);
        }
        jitter = if jitter == 0.0 {
            MIN_COVARIANCE
        } else {
            jitter * 10.0
        };
    }
    None
}

fn feasibility_order(left: &ScoredPoint, right: &ScoredPoint) -> Ordering {
    left.feasibility_key().cmp(&right.feasibility_key())
}

fn fallback_point(bounds: &[(f64, f64)]) -> ScoredPoint {
    let values = bounds.iter().map(|&(lower, _)| lower).collect();
    ScoredPoint {
        values,
        cost: f64::INFINITY,
        valid: false,
        constraint_violation: f64::INFINITY,
        objectives: [f64::INFINITY; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(values: &[f64], valid: bool, violation: f64) -> ScoredPoint {
        let cost = values.iter().map(|value| value * value).sum::<f64>();
        ScoredPoint {
            values: values.to_vec(),
            cost,
            valid,
            constraint_violation: violation,
            objectives: [cost, cost, 0.0],
        }
    }

    fn sphere<'a>(threshold: f64) -> impl FnMut(&[f64]) -> ScoredPoint + 'a {
        move |values| {
            let valid = values.first().copied().unwrap_or(0.0) >= threshold;
            let violation = if valid { 0.0 } else { threshold - values[0] };
            point(values, valid, violation)
        }
    }

    #[test]
    fn cma_es_is_deterministic_for_a_seeded_objective() {
        let bounds = [(0.0, 1.0), (0.0, 1.0)];
        let mut first_objective = sphere(-1.0);
        let first = run_cma_es(&bounds, 8, 5, 17, Some(&[0.8, 0.2]), &mut first_objective);
        let mut second_objective = sphere(-1.0);
        let second = run_cma_es(&bounds, 8, 5, 17, Some(&[0.8, 0.2]), &mut second_objective);
        assert_eq!(first, second);
    }

    #[test]
    fn cma_es_respects_physical_bounds() {
        let bounds = [(-3.0, 2.0), (10.0, 12.0), (0.0, 1.0)];
        let mut objective = sphere(-1.0);
        let outcome = run_cma_es(&bounds, 7, 4, 3, None, &mut objective);
        for (value, (lower, upper)) in outcome.winner.values.iter().zip(bounds) {
            assert!((*value >= lower) && (*value <= upper));
        }
    }

    #[test]
    fn feasibility_takes_priority_over_a_lower_scalar_cost() {
        let bounds = [(0.0, 1.0)];
        let mut objective = |values: &[f64]| {
            let valid = values[0] >= 0.8;
            let violation = if valid { 0.0 } else { 0.8 - values[0] };
            ScoredPoint {
                values: values.to_vec(),
                cost: if valid { 100.0 } else { -100.0 },
                valid,
                constraint_violation: violation,
                objectives: [if valid { 100.0 } else { -100.0 }, 0.0, 0.0],
            }
        };
        let outcome = run_cma_es(&bounds, 10, 3, 5, Some(&[0.9]), &mut objective);
        assert!(outcome.winner.valid);
    }

    #[test]
    fn regularized_cholesky_recovers_a_nearly_singular_covariance() {
        let matrix = vec![vec![1.0, 1.0], vec![1.0, 1.0 + 1.0e-16]];
        let factor = regularized_cholesky(&matrix).expect("regularization should recover matrix");
        assert!(factor.iter().flatten().all(|value| value.is_finite()));
        assert!(factor[0][0] > 0.0);
        assert!(factor[1][1] > 0.0);
    }

    #[test]
    fn rank_mu_update_can_adapt_an_off_diagonal_covariance() {
        let selected = [
            (vec![0.8, 0.8], point(&[0.8, 0.8], true, 0.0)),
            (vec![0.7, 0.7], point(&[0.7, 0.7], true, 0.0)),
        ];
        let references = selected.iter().collect::<Vec<_>>();
        let covariance = covariance_from_steps(&references, &[0.6, 0.4], &[0.5, 0.5], 0.1, 2);
        assert!(covariance[0][1] > 0.0);
        assert_eq!(covariance[0][1], covariance[1][0]);
    }
}
