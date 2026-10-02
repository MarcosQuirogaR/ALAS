// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sampling, repair, selection and schedule helpers for [`super::run`]; see
//! its module documentation for the algorithm and citations.

use super::super::rng::SearchRng;
use super::super::{OrderedF64, ScoredPoint, Tier};

pub(super) use crate::search_methods::product_de::unevaluated;

/// The epsilon-level comparison key. Feasible candidates always come first,
/// ahead of any epsilon-feasible one (a deviation from Takahama and Sakai,
/// who rank the two together by objective; see the kernel documentation);
/// a closed candidate whose violation is within `epsilon` is ranked by
/// objective; everything else by tier, then violation, then objective.
pub(super) fn epsilon_key(point: &ScoredPoint, epsilon: f64) -> (u8, OrderedF64, OrderedF64) {
    let (tier, violation, cost) = point.feasibility_key();
    match tier {
        Tier::Feasible => (0, violation, cost),
        Tier::ClosedInfeasible if violation.0 <= epsilon => (1, cost, violation),
        Tier::ClosedInfeasible => (2, violation, cost),
        Tier::NotClosed => (3, violation, cost),
        Tier::PreGateFailed => (4, violation, cost),
    }
}

/// Success weight: the violation reduction while infeasible, the relative
/// objective gain otherwise, so adaptation still learns while the whole
/// population is infeasible.
pub(super) fn selection_improvement(parent: &ScoredPoint, trial: &ScoredPoint) -> f64 {
    let gain = if parent.tier != trial.tier {
        1.0
    } else if !trial.valid() {
        (parent.constraint_violation - trial.constraint_violation).abs()
            / parent.constraint_violation.abs().max(1.0e-12)
    } else {
        (parent.cost - trial.cost).abs() / parent.cost.abs().max(1.0e-12)
    };
    if gain.is_finite() {
        gain.max(f64::EPSILON)
    } else {
        1.0
    }
}

/// `epsilon(0) (1 - n / Tc)^5` for `n < Tc` evaluations, exactly zero after.
pub(super) fn epsilon_schedule(epsilon0: f64, evaluations: usize, control: usize) -> f64 {
    if control == 0 || evaluations >= control {
        return 0.0;
    }
    epsilon0 * (1.0 - evaluations as f64 / control as f64).powi(5)
}

pub(super) fn quantile(sorted_ascending: &[f64], fraction: f64) -> f64 {
    if sorted_ascending.is_empty() {
        return 0.0;
    }
    let index = ((sorted_ascending.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted_ascending[index.min(sorted_ascending.len() - 1)]
}

/// Truncated Cauchy(mu, 0.1) draw for `F`, resampled while non-positive and
/// clipped to 1 (SHADE's rule).
pub(super) fn sample_f(mu: f64, rng: &mut SearchRng) -> f64 {
    for _ in 0..25 {
        let value = mu + 0.1 * (std::f64::consts::PI * (rng.unit() - 0.5)).tan();
        if value > 0.0 {
            return value.min(1.0);
        }
    }
    mu.clamp(f64::EPSILON, 1.0)
}

/// Normal(mu, 0.1) draw for `CR`, clipped to `[0, 1]`; the terminal memory
/// value draws zero.
pub(super) fn sample_cr(mu: Option<f64>, rng: &mut SearchRng) -> f64 {
    let Some(mu) = mu else {
        return 0.0;
    };
    let u1 = rng.uniform(f64::EPSILON, 1.0);
    let u2 = rng.unit();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    (mu + 0.1 * z).clamp(0.0, 1.0)
}

/// Weighted Lehmer mean `sum(w s^2) / sum(w s)`, weights normalized; `None`
/// for an empty or degenerate set.
pub(super) fn lehmer_mean(samples: impl Iterator<Item = (f64, f64)>) -> Option<f64> {
    let (mut numerator, mut denominator) = (0.0, 0.0);
    for (value, weight) in samples {
        numerator += weight * value * value;
        denominator += weight * value;
    }
    (denominator > 0.0 && numerator.is_finite()).then(|| numerator / denominator)
}

/// Midpoint repair: a component outside its bound is placed halfway between
/// the crossed bound and the parent's value.
pub(super) fn repair_midpoint(value: f64, parent: f64, bound: (f64, f64)) -> f64 {
    let (lower, upper) = bound;
    if !value.is_finite() {
        return parent.clamp(lower, upper);
    }
    if value < lower {
        return ((parent + lower) * 0.5).clamp(lower, upper);
    }
    if value > upper {
        return ((parent + upper) * 0.5).clamp(lower, upper);
    }
    value
}

/// A member of the best `max(2, round(p N))` under the epsilon comparison.
pub(super) fn choose_pbest(
    scored: &[ScoredPoint],
    epsilon: f64,
    fraction: f64,
    rng: &mut SearchRng,
) -> usize {
    let count = ((fraction * scored.len() as f64).round() as usize)
        .clamp(2.min(scored.len()), scored.len());
    let mut ranked: Vec<usize> = (0..scored.len()).collect();
    ranked.sort_by_key(|&index| epsilon_key(&scored[index], epsilon));
    ranked[rng.below(count)]
}

pub(super) fn choose_distinct(size: usize, exclude: &[usize], rng: &mut SearchRng) -> usize {
    loop {
        let candidate = rng.below(size);
        if !exclude.contains(&candidate) {
            return candidate;
        }
    }
}

/// A distinct index over the population `0..size` followed by the archive.
pub(super) fn choose_from_union(
    size: usize,
    archive_len: usize,
    exclude: &[usize],
    rng: &mut SearchRng,
) -> usize {
    choose_distinct(size + archive_len, exclude, rng)
}

fn archive_capacity(size: usize, rate: f64) -> usize {
    ((rate * size as f64).round() as usize).max(1)
}

pub(super) fn push_archive(
    archive: &mut Vec<Vec<f64>>,
    parent: Vec<f64>,
    size: usize,
    rate: f64,
    rng: &mut SearchRng,
) {
    if archive.len() < archive_capacity(size, rate) {
        archive.push(parent);
    } else {
        let index = rng.below(archive.len());
        archive[index] = parent;
    }
}

pub(super) fn trim_archive(
    archive: &mut Vec<Vec<f64>>,
    size: usize,
    rate: f64,
    rng: &mut SearchRng,
) {
    while archive.len() > archive_capacity(size, rate) {
        let index = rng.below(archive.len());
        archive.swap_remove(index);
    }
}

/// Linear population size reduction in evaluations:
/// `round(N_init + (N_min - N_init) n / B)`.
pub(super) fn linear_reduced_size(
    initial: usize,
    minimum: usize,
    evaluations: usize,
    budget: usize,
) -> usize {
    let fraction = (evaluations as f64 / budget.max(1) as f64).clamp(0.0, 1.0);
    let target = initial as f64 + (minimum as f64 - initial as f64) * fraction;
    (target.round() as usize).clamp(minimum, initial)
}

/// Keep the best `keep` members under the epsilon comparison.
pub(super) fn reduce_population(
    population: &mut Vec<Vec<f64>>,
    scored: &mut Vec<ScoredPoint>,
    keep: usize,
    epsilon: f64,
) {
    let mut order: Vec<usize> = (0..population.len()).collect();
    order.sort_by_key(|&index| epsilon_key(&scored[index], epsilon));
    order.truncate(keep);
    *population = order
        .iter()
        .map(|&index| population[index].clone())
        .collect();
    *scored = order.iter().map(|&index| scored[index].clone()).collect();
}

/// Mean over free coordinates of the population range over the bound width:
/// `0` for a collapsed population, `1` for one spanning the whole envelope.
pub(super) fn normalized_spread(population: &[Vec<f64>], bounds: &[(f64, f64)]) -> f64 {
    let mut total = 0.0;
    let mut free = 0usize;
    for (dimension, &(lower, upper)) in bounds.iter().enumerate() {
        if upper <= lower {
            continue;
        }
        free += 1;
        let (low, high) = population
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |acc, x| {
                (acc.0.min(x[dimension]), acc.1.max(x[dimension]))
            });
        if high >= low {
            total += (high - low) / (upper - lower);
        }
    }
    total / free.max(1) as f64
}

pub(super) fn relative_change(previous: f64, current: f64) -> f64 {
    (previous - current).abs() / previous.abs().max(1.0e-9)
}

/// Share of the evaluation budget `B` a run must have spent before
/// stagnation may stop it. Engineering choice: by then the epsilon level has
/// been zero for `0.3 B` evaluations and the linear reduction has halved the
/// population, so a stall is one of the reduced search, not of the wide
/// early population. Measured without it: A320-200, seed 20260922, stopped as
/// stagnated after 215 of 590 evaluations, 1.6 % worse than the same seed
/// run to its budget.
pub(super) const STAGNATION_MINIMUM_BUDGET_FRACTION: f64 = 0.5;

/// Whether a run whose feasible best has stalled for `stalled` generations
/// stops, for `(configured window, N_init, N_min)`. The effective window is
/// the configured one or `ceil(2 N_init / N_min)` generations, whichever is
/// longer: at the reduced size `N_min` that is two initial populations of
/// trials without progress (engineering choice). It applies only once
/// [`STAGNATION_MINIMUM_BUDGET_FRACTION`] of `budget` is spent.
pub(super) fn stagnation_stops(
    stalled: usize,
    (configured, initial, minimum): (usize, usize, usize),
    evaluations: usize,
    budget: usize,
) -> bool {
    let window = configured
        .max(1)
        .max((2 * initial).div_ceil(minimum.max(1)));
    stalled >= window
        && evaluations as f64 >= STAGNATION_MINIMUM_BUDGET_FRACTION * budget.max(1) as f64
}
