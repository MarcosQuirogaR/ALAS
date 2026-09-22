// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Largest-remainder integer allocation shared by the cabin's count and
//! percent resolution paths.

/// Split `target` whole units across three weights using the largest-remainder
/// method, so the parts sum to exactly `target` instead of drifting from
/// independently rounded shares.
pub(super) fn proportional_integer_allocation(target: i64, weights: [f64; 3]) -> [i64; 3] {
    if target <= 0 {
        return [0; 3];
    }
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return [0, 0, target];
    }

    let mut allocation = [0_i64; 3];
    let mut fractional = [0.0_f64; 3];
    let mut assigned = 0_i64;
    for (index, weight) in weights.into_iter().enumerate() {
        let raw = target as f64 * weight / total;
        let whole = raw.floor() as i64;
        allocation[index] = whole;
        fractional[index] = raw - whole as f64;
        assigned += whole;
    }

    // At most two seats remain after flooring three class allocations. The
    // stable index tie-break keeps saved runs reproducible.
    let mut remaining = (target - assigned).max(0);
    while remaining > 0 {
        let index = fractional
            .iter()
            .enumerate()
            .max_by(|(left_index, left), (right_index, right)| {
                left.total_cmp(right)
                    .then_with(|| right_index.cmp(left_index))
            })
            .map_or(2, |(index, _)| index);
        allocation[index] += 1;
        fractional[index] = f64::NEG_INFINITY;
        remaining -= 1;
    }
    allocation
}
