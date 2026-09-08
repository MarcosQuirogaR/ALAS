// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Normalized mesh geometry and deterministic poll directions for MADS.

use super::ScoredPoint;

pub(super) fn midpoint(bounds: &[(f64, f64)]) -> Vec<f64> {
    bounds
        .iter()
        .map(|&(lower, upper)| lower + (upper - lower) * 0.5)
        .collect()
}

pub(super) fn clamp_to_bounds(values: &mut [f64], bounds: &[(f64, f64)]) {
    for (value, &(lower, upper)) in values.iter_mut().zip(bounds) {
        *value = value.clamp(lower, upper);
    }
}

pub(super) fn to_normalized(values: &[f64], bounds: &[(f64, f64)]) -> Vec<f64> {
    values
        .iter()
        .zip(bounds)
        .map(|(&value, &(lower, upper))| {
            let width = upper - lower;
            if width == 0.0 {
                0.0
            } else {
                ((value - lower) / width).clamp(0.0, 1.0)
            }
        })
        .collect()
}

pub(super) fn from_normalized(values: &[f64], bounds: &[(f64, f64)]) -> Vec<f64> {
    values
        .iter()
        .zip(bounds)
        .map(|(&value, &(lower, upper))| {
            if upper == lower {
                lower
            } else {
                lower + value.clamp(0.0, 1.0) * (upper - lower)
            }
        })
        .collect()
}

pub(super) fn same_point(left: &[f64], right: &[f64], bounds: &[(f64, f64)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .zip(bounds)
            .all(|((&a, &b), &(lower, upper))| {
                let scale = (upper - lower).max(1.0);
                (a - b).abs() <= 1.0e-13 * scale
            })
}

pub(super) fn poll_centers(
    current: &[f64],
    infeasible: Option<&ScoredPoint>,
    bounds: &[(f64, f64)],
) -> Vec<Vec<f64>> {
    let mut centers = vec![current.to_vec()];
    if let Some(point) = infeasible {
        if !same_point(current, &point.values, bounds) {
            centers.push(point.values.clone());
        }
    }
    centers
}

pub(super) fn poll_point(
    center: &[f64],
    direction: &[i64],
    frame_size: f64,
    bounds: &[(f64, f64)],
) -> Vec<f64> {
    let center_normalized = to_normalized(center, bounds);
    let candidate = center_normalized
        .iter()
        .zip(direction)
        .map(|(&value, &component)| value + frame_size * component as f64)
        .collect::<Vec<_>>();
    from_normalized(&candidate, bounds)
}

pub(super) fn initial_search_points(
    bounds: &[(f64, f64)],
    origin: &[f64],
    mesh_size: f64,
    seed: u64,
) -> Vec<Vec<f64>> {
    let dimension = bounds.len();
    let count = dimension.saturating_mul(2).clamp(4, 16);
    let permutations = (0..dimension)
        .map(|index| permutation(count, mix_seed(seed, index as u64 + 1)))
        .collect::<Vec<_>>();
    (0..count)
        .map(|sample| {
            let normalized = (0..dimension)
                .map(|index| {
                    let target = (permutations[index][sample] as f64 + 0.5) / count as f64;
                    let mesh_index = ((target - origin[index]) / mesh_size).round();
                    (origin[index] + mesh_index * mesh_size).clamp(0.0, 1.0)
                })
                .collect::<Vec<_>>();
            from_normalized(&normalized, bounds)
        })
        .collect()
}

pub(super) fn permutation(length: usize, mut state: u64) -> Vec<usize> {
    let mut values = (0..length).collect::<Vec<_>>();
    for index in (1..length).rev() {
        let swap = (next_u64(&mut state) % (index as u64 + 1)) as usize;
        values.swap(index, swap);
    }
    values
}

/// Construct a MADS positive poll basis.  Integer offsets keep every point
/// on the translated mesh; `+/-` columns of a full-rank matrix are a maximal
/// positive spanning set.  The extra coordinate and pair-diagonal directions
/// preserve local resolution and make oblique valleys visible at coarse mesh
/// sizes while the Halton-generated matrix supplies changing rational slopes.
pub(super) fn poll_directions(dimension: usize, iteration: usize, seed: u64) -> Vec<Vec<i64>> {
    if dimension == 0 {
        return Vec::new();
    }
    // q grows without a finite cap: rational integer directions become
    // progressively finer on the unit sphere, which is the directional
    // density mechanism required by MADS.  Practical runs stop long before
    // usize-to-f64 precision or i64 capacity could become relevant.
    let q = ((iteration as f64 + 1.0).sqrt() + 1.0).ceil() as i64;
    let matrix = direction_matrix(dimension, q, iteration, seed);
    let mut directions = Vec::with_capacity(8 * dimension);
    for vector in
        (0..dimension).map(|column| matrix.iter().map(|row| row[column]).collect::<Vec<_>>())
    {
        add_signed_direction(&mut directions, vector);
    }
    for index in 0..dimension {
        let mut vector = vec![0_i64; dimension];
        vector[index] = 1;
        add_signed_direction(&mut directions, vector);
        if index + 1 < dimension {
            let mut plus = vec![0_i64; dimension];
            plus[index] = 1;
            plus[index + 1] = 1;
            add_signed_direction(&mut directions, plus);
            let mut minus = vec![0_i64; dimension];
            minus[index] = 1;
            minus[index + 1] = -1;
            add_signed_direction(&mut directions, minus);
        }
    }
    directions
}

pub(super) fn add_signed_direction(directions: &mut Vec<Vec<i64>>, direction: Vec<i64>) {
    if direction.iter().all(|&value| value == 0) {
        return;
    }
    if !directions.iter().any(|existing| existing == &direction) {
        directions.push(direction.clone());
    }
    let negative = direction
        .into_iter()
        .map(|value| -value)
        .collect::<Vec<_>>();
    if !directions.iter().any(|existing| existing == &negative) {
        directions.push(negative);
    }
}

pub(super) fn direction_matrix(
    dimension: usize,
    q: i64,
    iteration: usize,
    seed: u64,
) -> Vec<Vec<i64>> {
    let span = (2 * q + 1) as f64;
    let stride = dimension.saturating_mul(dimension).max(1) as u64;
    // Consecutive blocks of one low-discrepancy sequence are used for
    // successive polls.  A seed shifts the sequence without replacing it by
    // an opaque platform RNG, so the density and repeatability statements are
    // both explicit.
    let start = (seed % 1_000_000_000)
        .saturating_add((iteration as u64).saturating_mul(stride.saturating_mul(32)));
    for attempt in 0_u64..32 {
        let mut matrix = vec![vec![0_i64; dimension]; dimension];
        for (row, row_values) in matrix.iter_mut().enumerate() {
            for (column, value_slot) in row_values.iter_mut().enumerate() {
                let index = start
                    .saturating_add(attempt.saturating_mul(stride))
                    .saturating_add((row * dimension + column) as u64)
                    .saturating_add(1);
                let base = nth_prime((row + column) % 32);
                let value = (halton(index, base) * span).floor() as i64 - q;
                *value_slot = if row == column && value == 0 {
                    1
                } else {
                    value
                };
            }
        }
        if dimension > 1 && !has_oblique_column(&matrix) {
            matrix[1][0] = 1;
        }
        if full_rank(&matrix) {
            return matrix;
        }
    }
    let mut identity = vec![vec![0_i64; dimension]; dimension];
    for (index, row) in identity.iter_mut().enumerate() {
        row[index] = 1;
    }
    identity
}

pub(super) fn has_oblique_column(matrix: &[Vec<i64>]) -> bool {
    (0..matrix.len()).any(|column| {
        matrix
            .iter()
            .map(|row| row[column])
            .filter(|&value| value != 0)
            .count()
            > 1
    })
}

pub(super) fn full_rank(matrix: &[Vec<i64>]) -> bool {
    let dimension = matrix.len();
    if dimension == 0 || matrix.iter().any(|row| row.len() != dimension) {
        return false;
    }
    let mut work = matrix
        .iter()
        .map(|row| row.iter().map(|&value| value as f64).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for column in 0..dimension {
        let pivot = (column..dimension).max_by(|&left, &right| {
            work[left][column]
                .abs()
                .total_cmp(&work[right][column].abs())
        });
        let Some(pivot) = pivot else {
            return false;
        };
        if work[pivot][column].abs() <= f64::EPSILON {
            return false;
        }
        work.swap(column, pivot);
        let pivot_value = work[column][column];
        for row in (column + 1)..dimension {
            let factor = work[row][column] / pivot_value;
            let (above, below) = work.split_at_mut(row);
            let pivot_tail = &above[column][column..];
            let row_tail = &mut below[0][column..];
            for (value, pivot) in row_tail.iter_mut().zip(pivot_tail) {
                *value -= factor * *pivot;
            }
        }
    }
    true
}

pub(super) fn nth_prime(index: usize) -> u64 {
    let mut found = 0usize;
    let mut candidate = 2_u64;
    loop {
        if is_prime(candidate) {
            if found == index {
                return candidate;
            }
            found += 1;
        }
        candidate += 1;
    }
}

pub(super) fn is_prime(value: u64) -> bool {
    if value < 2 {
        return false;
    }
    let mut divisor = 2_u64;
    while divisor * divisor <= value {
        if value % divisor == 0 {
            return false;
        }
        divisor += 1;
    }
    true
}

pub(super) fn halton(mut index: u64, base: u64) -> f64 {
    let mut factor = 1.0 / base as f64;
    let mut value = 0.0;
    while index > 0 {
        value += factor * (index % base) as f64;
        index /= base;
        factor /= base as f64;
    }
    value
}

pub(super) fn mix_seed(seed: u64, stream: u64) -> u64 {
    let mut state = seed.wrapping_add(stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    next_u64(&mut state)
}

pub(super) fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
