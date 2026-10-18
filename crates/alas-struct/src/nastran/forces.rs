// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lift discretized onto the front-spar node line, in the reference's form and
//! with the trapezoid rule for the product solve.

use crate::sizing::gradient_unit;

/// Distribute `total_force_n` over `nid_y` with the same half-ellipse
/// [`crate::loads::elliptic_distributed_load`] uses, discretized onto the
/// mesh's own front-spar node line: `_elliptic_forces_by_y`.
///
/// The node line is not evenly spaced, so each node's share is weighted by the
/// span it stands for as well as by the ellipse. A distribution that summed to
/// nothing (every node at the tip, where the ellipse is zero) falls back to
/// an equal split rather than dividing by it.
pub fn elliptic_forces_by_y(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    semi_span: f64,
) -> Vec<(i64, f64)> {
    distribute_elliptic(nid_y, total_force_n, semi_span, false)
}

/// [`elliptic_forces_by_y`] with each node's tributary span by the trapezoid
/// rule: half a cell at the two end nodes. The reference's `np.gradient` gives
/// the end nodes a full one-sided cell, which moved about 5 percent of the lift
/// to the root node (y = 0, no bending arm) and left the lift moment 2-6
/// percent low against the elliptic load the native beam and the sizing use.
pub fn elliptic_forces_by_y_trapezoid(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    semi_span: f64,
) -> Vec<(i64, f64)> {
    distribute_elliptic(nid_y, total_force_n, semi_span, true)
}

pub(super) fn distribute_elliptic(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    semi_span: f64,
    trapezoid: bool,
) -> Vec<(i64, f64)> {
    let spans: Vec<f64> = nid_y.iter().map(|&(_, y)| y).collect();
    let ellipse: Vec<f64> = spans
        .iter()
        .map(|&y| {
            (1.0 - (y / semi_span.max(1e-9)).powi(2))
                .clamp(0.0, 1.0)
                .sqrt()
        })
        .collect();
    let widths: Vec<f64> = if spans.len() > 1 && !trapezoid {
        gradient_unit(&spans)
            .iter()
            .map(|value| value.abs())
            .collect()
    } else if spans.len() > 1 {
        (0..spans.len())
            .map(|i| {
                let below = if i > 0 { spans[i] - spans[i - 1] } else { 0.0 };
                let above = if i + 1 < spans.len() {
                    spans[i + 1] - spans[i]
                } else {
                    0.0
                };
                0.5 * (below.abs() + above.abs())
            })
            .collect()
    } else {
        vec![1.0]
    };

    let mut weights: Vec<f64> = ellipse
        .iter()
        .zip(&widths)
        .map(|(&q, &width)| q * width)
        .collect();
    let mut total: f64 = weights.iter().sum();
    if total < 1e-15 {
        weights = vec![1.0; ellipse.len()];
        total = weights.iter().sum();
    }
    nid_y
        .iter()
        .zip(&weights)
        .map(|(&(nid, _), &weight)| (nid, weight / total * total_force_n))
        .collect()
}
