// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded, deterministic NSGA-II search for the native product path.
//!
//! Objective values are minimized. Feasibility is handled before the objective
//! vector: every feasible point dominates every infeasible point, and an
//! infeasible point with a smaller aggregate violation dominates one with a
//! larger violation. This keeps a Pareto search from retaining attractive but
//! physically unusable aircraft merely because they have a favorable L/D.

use std::cmp::Ordering;

use crate::python_rng::RandomState;

use super::{clamp_to_bounds, EvaluatePoint, MethodOutcome, ScoredPoint};

const CROSSOVER_PROBABILITY: f64 = 0.9;
const CROSSOVER_INDEX: f64 = 15.0;
const MUTATION_INDEX: f64 = 20.0;

/// Run a bounded NSGA-II search and select a deterministic scalar winner.
pub(crate) fn run_nsga2(
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

    let size = population_size.max(2);
    let mut rng = RandomState::seed(seed);
    let mut population = initial_population(bounds, size, initial_design, &mut rng);
    let mut scored = population
        .iter()
        .map(|candidate| evaluate(candidate))
        .collect::<Vec<_>>();

    for _ in 0..generations {
        let fronts = nondominated_fronts(&scored);
        let crowding = crowding_distances(&scored, &fronts);
        let mut offspring = Vec::with_capacity(size);
        while offspring.len() < size {
            let first = tournament_index(&scored, &fronts, &crowding, &mut rng);
            let second = tournament_index(&scored, &fronts, &crowding, &mut rng);
            let child = make_child(&population[first], &population[second], bounds, &mut rng);
            offspring.push(child);
        }

        let offspring_scores = offspring
            .iter()
            .map(|candidate| evaluate(candidate))
            .collect::<Vec<_>>();
        population.extend(offspring);
        scored.extend(offspring_scores);

        let selected = select_next_generation(&scored, size);
        population = selected
            .iter()
            .map(|&index| scored[index].values.clone())
            .collect();
        scored = selected
            .into_iter()
            .map(|index| scored[index].clone())
            .collect();
    }

    let fronts = nondominated_fronts(&scored);
    let first_front = fronts.first().cloned().unwrap_or_default();
    let mut pareto_front = first_front
        .iter()
        .map(|&index| scored[index].clone())
        .collect::<Vec<_>>();
    pareto_front.sort_by(point_order);

    let winner = scored
        .iter()
        .min_by(|left, right| feasibility_order(left, right))
        .cloned()
        .or_else(|| scored.first().cloned())
        .unwrap_or_else(|| fallback_point(bounds));

    MethodOutcome {
        winner,
        pareto_front,
    }
}

fn initial_population(
    bounds: &[(f64, f64)],
    population_size: usize,
    initial_design: Option<&[f64]>,
    rng: &mut RandomState,
) -> Vec<Vec<f64>> {
    let mut population = Vec::with_capacity(population_size);
    if let Some(initial) = initial_design {
        if initial.len() == bounds.len() {
            let mut candidate = initial.to_vec();
            clamp_to_bounds(&mut candidate, bounds);
            population.push(candidate);
        }
    }
    while population.len() < population_size {
        population.push(
            bounds
                .iter()
                .map(|&(lower, upper)| rng.uniform(lower, upper))
                .collect(),
        );
    }
    population
}

fn make_child(
    first: &[f64],
    second: &[f64],
    bounds: &[(f64, f64)],
    rng: &mut RandomState,
) -> Vec<f64> {
    let dimension = bounds.len();
    let mut child = vec![0.0; dimension];
    for index in 0..dimension {
        let (lower, upper) = bounds[index];
        let width = upper - lower;
        let first_value = first[index];
        let second_value = second[index];
        let mut value = if rng.uniform(0.0, 1.0) <= CROSSOVER_PROBABILITY {
            simulated_binary_child(first_value, second_value, lower, upper, rng)
        } else {
            first_value
        };

        if rng.uniform(0.0, 1.0) < 1.0 / dimension as f64 {
            value = polynomial_mutation(value, lower, upper, MUTATION_INDEX, rng);
        }
        child[index] = if width.is_finite() && width > 0.0 {
            value.clamp(lower, upper)
        } else {
            lower
        };
    }
    child
}

fn simulated_binary_child(
    first: f64,
    second: f64,
    lower: f64,
    upper: f64,
    rng: &mut RandomState,
) -> f64 {
    if !first.is_finite() || !second.is_finite() || (first - second).abs() <= f64::EPSILON {
        return first.clamp(lower, upper);
    }
    let (low_value, high_value) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let width = upper - lower;
    if !width.is_finite() || width <= 0.0 {
        return lower;
    }
    let random = rng.uniform(0.0, 1.0).max(f64::MIN_POSITIVE);
    let beta = 1.0 + 2.0 * (low_value - lower) / (high_value - low_value);
    let alpha = 2.0 - beta.powf(-(CROSSOVER_INDEX + 1.0));
    let beta_q = if random <= 1.0 / alpha {
        (random * alpha).powf(1.0 / (CROSSOVER_INDEX + 1.0))
    } else {
        (1.0 / (2.0 - random * alpha)).powf(1.0 / (CROSSOVER_INDEX + 1.0))
    };
    let lower_child = 0.5 * ((low_value + high_value) - beta_q * (high_value - low_value));

    let beta = 1.0 + 2.0 * (upper - high_value) / (high_value - low_value);
    let alpha = 2.0 - beta.powf(-(CROSSOVER_INDEX + 1.0));
    let beta_q = if random <= 1.0 / alpha {
        (random * alpha).powf(1.0 / (CROSSOVER_INDEX + 1.0))
    } else {
        (1.0 / (2.0 - random * alpha)).powf(1.0 / (CROSSOVER_INDEX + 1.0))
    };
    let upper_child = 0.5 * ((low_value + high_value) + beta_q * (high_value - low_value));

    if rng.uniform(0.0, 1.0) <= 0.5 {
        lower_child.clamp(lower, upper)
    } else {
        upper_child.clamp(lower, upper)
    }
}

fn polynomial_mutation(
    value: f64,
    lower: f64,
    upper: f64,
    index: f64,
    rng: &mut RandomState,
) -> f64 {
    let width = upper - lower;
    if !width.is_finite() || width <= 0.0 {
        return lower;
    }
    let normalized = ((value - lower) / width).clamp(0.0, 1.0);
    let random = rng.uniform(0.0, 1.0);
    let delta = if random <= 0.5 {
        let term = 2.0 * random + (1.0 - 2.0 * random) * (1.0 - normalized).powf(index + 1.0);
        term.powf(1.0 / (index + 1.0)) - 1.0
    } else {
        let term = 2.0 * (1.0 - random) + 2.0 * (random - 0.5) * normalized.powf(index + 1.0);
        1.0 - term.powf(1.0 / (index + 1.0))
    };
    (value + delta * width).clamp(lower, upper)
}

fn tournament_index(
    points: &[ScoredPoint],
    fronts: &[Vec<usize>],
    crowding: &[f64],
    rng: &mut RandomState,
) -> usize {
    let first = rng.randint(points.len());
    let second = rng.randint(points.len());
    let first_rank = rank_of(first, fronts);
    let second_rank = rank_of(second, fronts);
    match first_rank.cmp(&second_rank) {
        Ordering::Less => first,
        Ordering::Greater => second,
        Ordering::Equal => match crowding[first].total_cmp(&crowding[second]).reverse() {
            Ordering::Less => first,
            Ordering::Greater => second,
            Ordering::Equal => {
                if point_order(&points[first], &points[second]) == Ordering::Greater {
                    second
                } else {
                    first
                }
            }
        },
    }
}

fn rank_of(index: usize, fronts: &[Vec<usize>]) -> usize {
    fronts
        .iter()
        .position(|front| front.contains(&index))
        .unwrap_or(usize::MAX)
}

fn select_next_generation(points: &[ScoredPoint], population_size: usize) -> Vec<usize> {
    let fronts = nondominated_fronts(points);
    let mut selected = Vec::with_capacity(population_size);
    for front in fronts {
        if selected.len() + front.len() <= population_size {
            selected.extend(front);
            continue;
        }
        let crowding = crowding_distances(points, std::slice::from_ref(&front));
        let mut partial = front;
        partial.sort_by(|&left, &right| {
            crowding[right]
                .total_cmp(&crowding[left])
                .then_with(|| point_order(&points[left], &points[right]))
        });
        selected.extend(partial.into_iter().take(population_size - selected.len()));
        break;
    }
    selected
}

fn nondominated_fronts(points: &[ScoredPoint]) -> Vec<Vec<usize>> {
    let count = points.len();
    if count == 0 {
        return Vec::new();
    }
    let mut dominates = vec![Vec::new(); count];
    let mut dominated_by = vec![0_usize; count];
    let mut first = Vec::new();
    for left in 0..count {
        for right in (left + 1)..count {
            if dominates_point(&points[left], &points[right]) {
                dominates[left].push(right);
                dominated_by[right] += 1;
            } else if dominates_point(&points[right], &points[left]) {
                dominates[right].push(left);
                dominated_by[left] += 1;
            }
        }
        if dominated_by[left] == 0 {
            first.push(left);
        }
    }
    first.sort_by(|&left, &right| point_order(&points[left], &points[right]));

    let mut fronts = vec![first];
    let mut current = 0;
    while current < fronts.len() {
        let mut next = Vec::new();
        for &point in &fronts[current] {
            for &dominated in &dominates[point] {
                dominated_by[dominated] -= 1;
                if dominated_by[dominated] == 0 {
                    next.push(dominated);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_by(|&left, &right| point_order(&points[left], &points[right]));
        fronts.push(next);
        current += 1;
    }
    fronts
}

fn dominates_point(left: &ScoredPoint, right: &ScoredPoint) -> bool {
    if left.valid != right.valid {
        return left.valid;
    }
    if !left.valid {
        let violation_order = left
            .constraint_violation
            .total_cmp(&right.constraint_violation);
        if violation_order != Ordering::Equal {
            return violation_order == Ordering::Less;
        }
    }
    let no_worse = left
        .objectives
        .iter()
        .zip(right.objectives)
        .all(|(&a, b)| objective_value(a) <= objective_value(b));
    let strictly_better = left
        .objectives
        .iter()
        .zip(right.objectives)
        .any(|(&a, b)| objective_value(a) < objective_value(b));
    no_worse && strictly_better
}

fn crowding_distances(points: &[ScoredPoint], front_set: &[Vec<usize>]) -> Vec<f64> {
    let mut distances = vec![0.0; points.len()];
    for front in front_set {
        if front.len() <= 2 {
            for &index in front {
                distances[index] = f64::INFINITY;
            }
            continue;
        }
        for objective in 0..3 {
            let mut order = front.clone();
            order.sort_by(|&left, &right| {
                objective_value(points[left].objectives[objective])
                    .total_cmp(&objective_value(points[right].objectives[objective]))
                    .then_with(|| point_order(&points[left], &points[right]))
            });
            let lower = objective_value(points[order[0]].objectives[objective]);
            let upper =
                objective_value(points[*order.last().unwrap_or(&order[0])].objectives[objective]);
            distances[order[0]] = f64::INFINITY;
            distances[*order.last().unwrap_or(&order[0])] = f64::INFINITY;
            if !lower.is_finite() || !upper.is_finite() || upper <= lower {
                continue;
            }
            for window in order.windows(3) {
                let index = window[1];
                if distances[index].is_infinite() {
                    continue;
                }
                let previous = objective_value(points[window[0]].objectives[objective]);
                let next = objective_value(points[window[2]].objectives[objective]);
                distances[index] += (next - previous) / (upper - lower);
            }
        }
    }
    distances
}

fn feasibility_order(left: &ScoredPoint, right: &ScoredPoint) -> Ordering {
    left.feasibility_key().cmp(&right.feasibility_key())
}

fn point_order(left: &ScoredPoint, right: &ScoredPoint) -> Ordering {
    feasibility_order(left, right)
        .then_with(|| {
            left.objectives
                .iter()
                .zip(right.objectives)
                .map(|(&a, b)| objective_value(a).total_cmp(&objective_value(b)))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            left.values
                .iter()
                .zip(&right.values)
                .map(|(&a, &b)| a.total_cmp(&b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| left.values.len().cmp(&right.values.len()))
        })
}

fn objective_value(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        f64::INFINITY
    }
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
    use super::super::OrderedF64;
    use super::*;

    fn point(values: &[f64], valid: bool, violation: f64, objectives: [f64; 3]) -> ScoredPoint {
        ScoredPoint {
            values: values.to_vec(),
            cost: objectives[0],
            valid,
            constraint_violation: violation,
            objectives,
        }
    }

    fn sphere<'a>(threshold: f64) -> impl FnMut(&[f64]) -> ScoredPoint + 'a {
        move |values| {
            let cost = values.iter().map(|value| value * value).sum::<f64>();
            let valid = values.first().copied().unwrap_or(0.0) >= threshold;
            let violation = if valid { 0.0 } else { threshold - values[0] };
            point(values, valid, violation, [cost, cost, values[0]])
        }
    }

    #[test]
    fn nsga2_is_deterministic_for_a_seeded_objective() {
        let bounds = [(0.0, 1.0), (0.0, 1.0)];
        let mut first_objective = sphere(-1.0);
        let first = run_nsga2(&bounds, 12, 4, 7, Some(&[0.4, 0.6]), &mut first_objective);
        let mut second_objective = sphere(-1.0);
        let second = run_nsga2(&bounds, 12, 4, 7, Some(&[0.4, 0.6]), &mut second_objective);
        assert_eq!(first, second);
    }

    #[test]
    fn nsga2_keeps_the_winner_and_front_inside_bounds() {
        let bounds = [(-2.0, 2.0), (10.0, 12.0)];
        let mut objective = sphere(-1.0);
        let outcome = run_nsga2(&bounds, 8, 2, 12, None, &mut objective);
        let candidates = std::iter::once(&outcome.winner).chain(outcome.pareto_front.iter());
        for candidate in candidates {
            for (value, (lower, upper)) in candidate.values.iter().zip(bounds) {
                assert!((*value >= lower) && (*value <= upper));
            }
        }
    }

    #[test]
    fn feasibility_dominates_a_lower_cost_infeasible_point() {
        let feasible = point(&[0.0], true, 0.0, [10.0, 10.0, 10.0]);
        let infeasible = point(&[1.0], false, 1.0, [-100.0, -100.0, -100.0]);
        assert!(dominates_point(&feasible, &infeasible));
        assert!(!dominates_point(&infeasible, &feasible));
    }

    #[test]
    fn nondominated_sort_retains_a_tradeoff_front() {
        let points = vec![
            point(&[0.0], true, 0.0, [0.0, 1.0, 0.0]),
            point(&[0.5], true, 0.0, [0.5, 0.5, 0.0]),
            point(&[1.0], true, 0.0, [1.0, 0.0, 0.0]),
            point(&[0.2], true, 0.0, [0.8, 0.8, 0.8]),
        ];
        let fronts = nondominated_fronts(&points);
        assert_eq!(fronts[0].len(), 3);
        assert!(fronts[1].contains(&3));
    }

    #[test]
    fn winner_uses_feasibility_key_deterministically() {
        let valid = point(&[1.0], true, 0.0, [100.0, 100.0, 100.0]);
        let invalid = point(&[0.0], false, 0.1, [-100.0, -100.0, -100.0]);
        assert_eq!(feasibility_order(&valid, &invalid), Ordering::Less);
        assert_eq!(OrderedF64(1.0).cmp(&OrderedF64(2.0)), Ordering::Less);
    }
}
