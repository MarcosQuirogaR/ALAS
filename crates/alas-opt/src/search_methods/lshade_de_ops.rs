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

/// Feasible elites precede all infeasible candidates. Between infeasible
/// candidates, the epsilon-level comparison (Takahama & Sakai) ranks finite
/// positive violations inside epsilon by objective, other misses by violation.
pub(super) fn epsilon_key(point: &ScoredPoint, epsilon: f64) -> (u8, OrderedF64, OrderedF64) {
    if !point.cost.is_finite() || !point.constraint_violation.is_finite() {
        return (3, OrderedF64(f64::INFINITY), OrderedF64(f64::INFINITY));
    }
    // A completed feasible design survives every restoration phase. Epsilon
    // only helps order infeasible candidates; it cannot reward abandoning a
    // feasible basin for an attractive objective outside the physical limits.
    if point.valid {
        return point.feasibility_key();
    }
    if point.constraint_violation > 0.0 && point.constraint_violation <= epsilon {
        (
            1,
            OrderedF64(point.cost),
            OrderedF64(point.constraint_violation),
        )
    } else {
        (
            2,
            OrderedF64(point.constraint_violation),
            OrderedF64(point.cost),
        )
    }
}

/// SHADE weights successful feasibility restoration by residual reduction,
/// rather than by an unrelated (often constant) failure cost. This keeps F/CR
/// adaptation active when the entire initial population is infeasible.
pub(super) fn selection_improvement(parent: &ScoredPoint, trial: &ScoredPoint) -> f64 {
    let gain = if parent.valid != trial.valid {
        1.0
    } else if !trial.valid && trial.constraint_violation < parent.constraint_violation {
        (parent.constraint_violation - trial.constraint_violation)
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
    let mut active = 0usize;
    for (dimension, &(lower, upper)) in bounds.iter().enumerate() {
        if lower == upper {
            continue;
        }
        active += 1;
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
    total / active.max(1) as f64
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

/// Mix a reference-conditioned neighborhood with independent full-envelope
/// exploration. The 3:1 allocation is a search heuristic, not a geometry
/// constraint: subsequent mutation can visit every configured bound. Radius
/// is dimensionless; actual design vectors retain the interface's SI/degree
/// units, and locked coordinates are left untouched.
pub(super) fn conditioned_population(
    bounds: &[(f64, f64)],
    count: usize,
    initial: Option<&[f64]>,
    radius: Option<f64>,
    rng: &mut RandomState,
) -> Vec<Vec<f64>> {
    let Some(initial) = initial.filter(|point| point.len() == bounds.len()) else {
        return latin_hypercube(bounds, count, rng);
    };
    let mut center = initial.to_vec();
    clamp_into_bounds(&mut center, bounds);
    let Some(radius) = radius.filter(|value| value.is_finite() && *value >= 0.0) else {
        let mut points = latin_hypercube(bounds, count, rng);
        points[0] = center;
        return points;
    };
    let global_count = count.div_ceil(4);
    let local_bounds: Vec<(f64, f64)> = bounds
        .iter()
        .zip(&center)
        .map(|(&(lower, upper), &value)| {
            let delta = radius.min(1.0) * (upper - lower);
            ((value - delta).max(lower), (value + delta).min(upper))
        })
        .collect();
    let mut points = latin_hypercube(&local_bounds, count - global_count, rng);
    points[0] = center;
    points.extend(latin_hypercube(bounds, global_count, rng));
    points
}

fn shuffle(values: &mut [usize], rng: &mut RandomState) {
    for position in (1..values.len()).rev() {
        let swap_with = rng.randint(position + 1);
        values.swap(position, swap_with);
    }
}
