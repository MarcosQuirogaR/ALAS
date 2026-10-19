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

/// [`elliptic_forces_by_y`] with the half-ellipse integrated exactly over each
/// cell and shared to the cell's two nodes by the linear shape functions (the
/// statically equivalent, consistent nodal load).
///
/// The reference's `np.gradient` gives the end nodes a full one-sided cell,
/// which moved about 5 percent of the lift to the root node (y = 0, no bending
/// arm). The trapezoid rule fixes that but samples a square-root ellipse at the
/// nodes: with the lift renormalised to the total, the outboard cells lose lift
/// and the bending moment at 55-85 percent span came out 9-40 percent below the
/// elliptic load the native beam and the sizing use (A320 and B747-400 decks).
/// The consistent load keeps the total and the first moment of every cell, so
/// the cumulative moment follows the analytic ellipse to the mesh resolution.
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
    if trapezoid && spans.len() > 1 {
        return consistent_elliptic(nid_y, total_force_n, semi_span);
    }
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

/// Antiderivatives of `sqrt(1 - u^2)` and `u sqrt(1 - u^2)` at `u = y / s`.
fn ellipse_integrals(y: f64, semi_span: f64) -> (f64, f64) {
    let u = (y / semi_span).clamp(0.0, 1.0);
    let root = (1.0 - u * u).max(0.0).sqrt();
    (
        semi_span * 0.5 * (u * root + u.asin()),
        -semi_span * semi_span * (1.0 - u * u).max(0.0).powf(1.5) / 3.0,
    )
}

/// Consistent nodal loads of the half-ellipse `q(y) ~ sqrt(1 - (y/s)^2)` on the
/// node line, normalised to `total_force_n`.
fn consistent_elliptic(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    semi_span: f64,
) -> Vec<(i64, f64)> {
    let s = semi_span.max(1e-9);
    let mut order: Vec<usize> = (0..nid_y.len()).collect();
    order.sort_by(|&a, &b| nid_y[a].1.total_cmp(&nid_y[b].1));
    let mut weights = vec![0.0; nid_y.len()];
    for pair in order.windows(2) {
        let (i, j) = (pair[0], pair[1]);
        let (a, b) = (nid_y[i].1, nid_y[j].1);
        let width = b - a;
        if width <= 0.0 || !width.is_finite() {
            continue;
        }
        let (f_a, g_a) = ellipse_integrals(a, s);
        let (f_b, g_b) = ellipse_integrals(b, s);
        let (i0, i1) = (f_b - f_a, g_b - g_a);
        weights[i] += (b * i0 - i1) / width;
        weights[j] += (i1 - a * i0) / width;
    }
    // Lift of the ellipse outboard of the last node (the mesh ends short of the
    // wing tip when the tip rib is inboard of it) acts on the last node.
    if let Some(&last) = order.last() {
        let y_last = nid_y[last].1;
        if semi_span > y_last {
            let (f_a, _) = ellipse_integrals(y_last, s);
            let (f_b, _) = ellipse_integrals(s, s);
            weights[last] += f_b - f_a;
        }
    }
    let total: f64 = weights.iter().sum();
    if total < 1e-15 {
        let equal = total_force_n / nid_y.len().max(1) as f64;
        return nid_y.iter().map(|&(nid, _)| (nid, equal)).collect();
    }
    nid_y
        .iter()
        .zip(&weights)
        .map(|(&(nid, _), &weight)| (nid, weight / total * total_force_n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consistent_load_keeps_the_total_and_the_analytic_root_moment() {
        // Coarse, uneven node line like a rib-spaced front spar.
        let ys = [0.0, 1.2, 3.1, 5.5, 8.0, 10.6, 13.0, 15.0, 16.5, 17.05];
        let nodes: Vec<(i64, f64)> = ys.iter().enumerate().map(|(i, &y)| (i as i64, y)).collect();
        let s = 17.05;
        let total = 1.0e6;
        let forces = elliptic_forces_by_y_trapezoid(&nodes, total, s);
        let sum: f64 = forces.iter().map(|f| f.1).sum();
        assert!((sum - total).abs() < 1e-6 * total);
        let moment: f64 = forces.iter().zip(&nodes).map(|(f, n)| f.1 * n.1).sum();
        // Analytic first moment of a half ellipse: 4 s / (3 pi) times the total.
        let exact = total * 4.0 * s / (3.0 * std::f64::consts::PI);
        assert!((moment / exact - 1.0).abs() < 0.01, "{}", moment / exact);
        // The trapezoid rule this replaces was several percent low.
        let old = distribute_elliptic_old(&nodes, total, s);
        assert!(old < 0.99 * exact);
    }

    fn distribute_elliptic_old(nodes: &[(i64, f64)], total: f64, s: f64) -> f64 {
        let weights: Vec<f64> = (0..nodes.len())
            .map(|i| {
                let below = if i > 0 {
                    nodes[i].1 - nodes[i - 1].1
                } else {
                    0.0
                };
                let above = if i + 1 < nodes.len() {
                    nodes[i + 1].1 - nodes[i].1
                } else {
                    0.0
                };
                (1.0 - (nodes[i].1 / s).powi(2)).max(0.0).sqrt() * 0.5 * (below + above)
            })
            .collect();
        let sum: f64 = weights.iter().sum();
        weights
            .iter()
            .zip(nodes)
            .map(|(w, n)| w / sum * total * n.1)
            .sum()
    }
}

#[cfg(test)]
mod tail_tests {
    use super::*;

    #[test]
    fn lift_outboard_of_the_last_node_is_kept_on_the_last_node() {
        // Mesh ends at 98 percent of the wing semi-span.
        let s = 30.0;
        let nodes: Vec<(i64, f64)> = (0..=20).map(|i| (i, 0.98 * s * i as f64 / 20.0)).collect();
        let total = 1.0e6;
        let forces = elliptic_forces_by_y_trapezoid(&nodes, total, s);
        let moment: f64 = forces.iter().zip(&nodes).map(|(f, n)| f.1 * n.1).sum();
        let exact = total * 4.0 * s / (3.0 * std::f64::consts::PI);
        assert!((moment / exact - 1.0).abs() < 0.005, "{}", moment / exact);
        let sum: f64 = forces.iter().map(|f| f.1).sum();
        assert!((sum - total).abs() < 1e-6 * total);
    }
}
