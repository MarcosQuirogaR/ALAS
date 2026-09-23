// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sampling, repair, selection and memory-update helpers for
//! [`super::run`]. Split out of `lshade_de.rs` to keep that module's
//! generation loop readable on its own; see its module documentation for the
//! algorithm and citations.

use crate::python_rng::RandomState;

use super::{ARCHIVE_RATE, EPSILON_DECAY_EXPONENT, MEMORY_SIZE, MIN_POPULATION};
use crate::search_methods::{OrderedF64, ScoredPoint};

pub(super) use crate::search_methods::product_de::unevaluated;

pub(super) fn min_by_feasibility(left: ScoredPoint, right: ScoredPoint) -> ScoredPoint {
    if right.feasibility_key() < left.feasibility_key() {
        right
    } else {
        left
    }
}

/// The epsilon-level comparison key (Takahama & Sakai): a candidate within
/// `epsilon` of feasible is ranked by objective alone; otherwise by
/// violation, objective as the tie-break.
pub(super) fn epsilon_key(point: &ScoredPoint, epsilon: f64) -> (u8, OrderedF64, OrderedF64) {
    if point.constraint_violation <= epsilon {
        (
            0,
            OrderedF64(point.cost),
            OrderedF64(point.constraint_violation),
        )
    } else {
        (
            1,
            OrderedF64(point.constraint_violation),
            OrderedF64(point.cost),
        )
    }
}

/// `epsilon(0) * (1 - generation / control_generations) ^ cp` for
/// `generation < control_generations`, exactly zero from that point on so
/// the comparison becomes Deb's feasibility rule for the remainder of the
/// budget.
pub(super) fn epsilon_schedule(
    epsilon0: f64,
    generation: usize,
    control_generations: usize,
) -> f64 {
    if control_generations == 0 || generation >= control_generations {
        return 0.0;
    }
    let fraction = 1.0 - generation as f64 / control_generations as f64;
    epsilon0 * fraction.powf(EPSILON_DECAY_EXPONENT)
}

pub(super) fn quantile(sorted_ascending: &[f64], fraction: f64) -> f64 {
    if sorted_ascending.is_empty() {
        return 0.0;
    }
    let index = ((sorted_ascending.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted_ascending[index.min(sorted_ascending.len() - 1)]
}

/// Truncated Cauchy(mu, 0.1) draw for the mutation factor, resampled while
/// non-positive and clipped to 1.0, following SHADE's own `F` sampling rule.
pub(super) fn sample_f(mu: f64, rng: &mut RandomState) -> f64 {
    for _ in 0..25 {
        let u = rng.uniform(0.0, 1.0);
        let value = mu + 0.1 * (std::f64::consts::PI * (u - 0.5)).tan();
        if value > 0.0 {
            return value.min(1.0);
        }
    }
    0.5
}

/// Normal(mu, 0.1) draw for the crossover rate, clipped to `[0, 1]`.
pub(super) fn sample_cr(mu: f64, rng: &mut RandomState) -> f64 {
    let u1 = rng.uniform(f64::EPSILON, 1.0);
    let u2 = rng.uniform(0.0, 1.0);
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    (mu + 0.1 * z).clamp(0.0, 1.0)
}

/// Midpoint-to-parent bound repair: a component that leaves `[lower, upper]`
/// is placed halfway between the bound it crossed and the parent's own
/// value at that component, rather than reflected or clamped to the bound
/// itself.
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

pub(super) fn choose_pbest(
    scored: &[ScoredPoint],
    epsilon: f64,
    p_fraction: f64,
    rng: &mut RandomState,
) -> usize {
    let count = ((p_fraction * scored.len() as f64).ceil() as usize).clamp(1, scored.len());
    let mut ranked: Vec<usize> = (0..scored.len()).collect();
    ranked.sort_by_key(|&index| epsilon_key(&scored[index], epsilon));
    ranked[rng.randint(count)]
}

pub(super) fn choose_distinct(
    population_size: usize,
    exclude: &[usize],
    rng: &mut RandomState,
) -> usize {
    loop {
        let candidate = rng.randint(population_size);
        if !exclude.contains(&candidate) {
            return candidate;
        }
    }
}

/// A distinct index over `population` indices `0..population_size` followed
/// by archive indices `population_size..population_size + archive_len`.
pub(super) fn choose_from_union(
    population_size: usize,
    archive_len: usize,
    exclude: &[usize],
    rng: &mut RandomState,
) -> usize {
    let total = population_size + archive_len;
    if total <= exclude.len() {
        return exclude[0];
    }
    loop {
        let candidate = rng.randint(total);
        if !exclude.contains(&candidate) {
            return candidate;
        }
    }
}

pub(super) fn push_archive(
    archive: &mut Vec<Vec<f64>>,
    replaced_parent: Vec<f64>,
    population_size: usize,
    rng: &mut RandomState,
) {
    let capacity = ((ARCHIVE_RATE * population_size as f64).round() as usize).max(1);
    if archive.len() < capacity {
        archive.push(replaced_parent);
    } else {
        // The archive is enrichment for the mutation pool, not a ranked
        // structure, so which old entry is evicted has no effect on
        // reproducibility beyond which mutation directions later trials draw
        // from; the eviction slot still comes from the seeded stream so the
        // whole run stays a pure function of the seed.
        let index = rng.randint(archive.len());
        archive[index] = replaced_parent;
    }
}

pub(super) fn trim_archive(
    archive: &mut Vec<Vec<f64>>,
    population_size: usize,
    rng: &mut RandomState,
) {
    let capacity = ((ARCHIVE_RATE * population_size as f64).round() as usize).max(1);
    while archive.len() > capacity {
        let index = rng.randint(archive.len());
        archive.swap_remove(index);
    }
}

pub(super) fn update_memory(
    memory_f: &mut [f64; MEMORY_SIZE],
    memory_cr: &mut [f64; MEMORY_SIZE],
    memory_index: &mut usize,
    successes: &[(f64, f64, f64)],
) {
    if successes.is_empty() {
        return;
    }
    let weight_sum: f64 = successes.iter().map(|(_, _, weight)| weight).sum();
    if weight_sum <= 0.0 {
        return;
    }
    let mut lehmer_num = 0.0;
    let mut lehmer_den = 0.0;
    let mut cr_num = 0.0;
    for &(f, cr, weight) in successes {
        let w = weight / weight_sum;
        lehmer_num += w * f * f;
        lehmer_den += w * f;
        cr_num += w * cr;
    }
    if lehmer_den > 0.0 {
        let slot = *memory_index;
        memory_f[slot] = (lehmer_num / lehmer_den).clamp(0.0, 1.0);
        memory_cr[slot] = cr_num.clamp(0.0, 1.0);
        *memory_index = (slot + 1) % MEMORY_SIZE;
    }
}

/// L-SHADE's linear population size reduction: shrinks affinely from the
/// initial size to [`super::MIN_POPULATION`] as `fraction` (evaluated
/// generations over the generation budget) runs from 0 to 1.
pub(super) fn linear_reduced_size(initial: usize, fraction: f64) -> usize {
    let target = initial as f64 - (initial as f64 - MIN_POPULATION as f64) * fraction;
    (target.round() as usize).clamp(MIN_POPULATION, initial)
}

pub(super) fn reduce_population(
    population: &mut Vec<Vec<f64>>,
    scored: &mut Vec<ScoredPoint>,
    keep: usize,
    epsilon: f64,
) {
    let mut order: Vec<usize> = (0..population.len()).collect();
    order.sort_by_key(|&index| epsilon_key(&scored[index], epsilon));
    let survivors: Vec<usize> = order.into_iter().take(keep).collect();
    let mut next_population = Vec::with_capacity(keep);
    let mut next_scored = Vec::with_capacity(keep);
    for index in survivors {
        next_population.push(population[index].clone());
        next_scored.push(scored[index].clone());
    }
    *population = next_population;
    *scored = next_scored;
}

/// Mean, over design dimensions, of each dimension's population range
/// normalized by its bound width: `0` when every candidate sits on the same
/// point, `1` when a dimension still spans its whole envelope.
pub(super) fn normalized_spread(population: &[Vec<f64>], bounds: &[(f64, f64)]) -> f64 {
    if population.len() < 2 || bounds.is_empty() {
        return 0.0;
    }
    let mut total = 0.0;
    for (dimension, &(lower, upper)) in bounds.iter().enumerate() {
        let width = (upper - lower).max(f64::EPSILON);
        let mut min_value = f64::INFINITY;
        let mut max_value = f64::NEG_INFINITY;
        for candidate in population {
            let value = candidate[dimension];
            min_value = min_value.min(value);
            max_value = max_value.max(value);
        }
        total += (max_value - min_value) / width;
    }
    total / bounds.len() as f64
}

pub(super) fn relative_change(previous: f64, current: f64) -> f64 {
    (previous - current).abs() / previous.abs().max(1.0e-9)
}

/// Signed relative change from `start` to `current`: positive when `current`
/// is smaller (an improvement, under this crate's minimisation convention),
/// negative when it worsened. Scaled by `|start|`, with a zero-scale start
/// falling back to the absolute difference so a zero-cost baseline still
/// reports a meaningful sign.
pub(super) fn relative_change_signed(start: f64, current: f64) -> f64 {
    if !start.is_finite() || !current.is_finite() {
        return 0.0;
    }
    let scale = start.abs();
    if scale <= f64::MIN_POSITIVE {
        start - current
    } else {
        (start - current) / scale
    }
}

pub(super) fn clamp_into_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = if value.is_finite() {
            value.clamp(lower, upper)
        } else {
            lower
        };
    }
}

pub(super) fn latin_hypercube(
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
    for candidate in &mut population {
        crate::search_methods::clamp_to_bounds(candidate, bounds);
    }
    population
}

fn shuffle(values: &mut [usize], rng: &mut RandomState) {
    for position in (1..values.len()).rev() {
        let swap_with = rng.randint(position + 1);
        values.swap(position, swap_with);
    }
}
