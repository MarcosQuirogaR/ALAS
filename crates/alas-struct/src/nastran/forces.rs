// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lift discretized onto the front-spar node line, in the reference's form and
//! with the consistent (statically equivalent) nodal load for the product solve.

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
pub fn elliptic_forces_by_y_consistent(
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
    consistent: bool,
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
    if consistent && spans.len() > 1 {
        return consistent_elliptic(nid_y, total_force_n, semi_span);
    }
    let widths: Vec<f64> = if spans.len() > 1 && !consistent {
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
    // Lift of the ellipse inboard of the first node (root plane at y = 0 when
    // the node line starts at the root, as the product spar line does) acts on
    // the first node: the same lumping rule as the outboard end below.
    if let Some(&first) = order.first() {
        let y_first = nid_y[first].1.min(s);
        if y_first > 0.0 {
            let (f_0, _) = ellipse_integrals(0.0, s);
            let (f_a, _) = ellipse_integrals(y_first, s);
            weights[first] += f_a - f_0;
        }
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
    // Normalise by the analytic total of the half-ellipse on [0, s], not by the
    // sum of the weights: the sum is then a check on the coverage, not a
    // renormalisation that would hide a missing segment. Any part of the span
    // the node line does not reach is the root-side or outboard lift already
    // lumped above.
    let total = ellipse_integrals(s, s).0 - ellipse_integrals(0.0, s).0;
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

/// Consistent nodal weights of a unit spanwise density `q` (per metre of span,
/// `q(y)` for `y in [0, semi_span]`) on the node line: the density integrated
/// exactly over each cell (fine trapezoid table) and shared to the two nodes by
/// the linear shape functions, as in [`elliptic_forces_by_y_consistent`]. Load
/// inboard of the first node and outboard of the last is lumped on them. The
/// weights sum to `integral q dy`.
fn consistent_weights(nid_y: &[(i64, f64)], semi_span: f64, q: &dyn Fn(f64) -> f64) -> Vec<f64> {
    const N: usize = 4000;
    let s = semi_span.max(1e-9);
    let dy = s / N as f64;
    let mut f = vec![0.0; N + 1];
    let mut g = vec![0.0; N + 1];
    for i in 1..=N {
        let (a, b) = ((i - 1) as f64 * dy, i as f64 * dy);
        f[i] = f[i - 1] + 0.5 * dy * (q(a) + q(b));
        g[i] = g[i - 1] + 0.5 * dy * (a * q(a) + b * q(b));
    }
    let at = |table: &[f64], y: f64| {
        let t = (y / dy).clamp(0.0, N as f64);
        let i = (t.floor() as usize).min(N - 1);
        table[i] + (table[i + 1] - table[i]) * (t - i as f64)
    };
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
        let (i0, i1) = (at(&f, b) - at(&f, a), at(&g, b) - at(&g, a));
        weights[i] += (b * i0 - i1) / width;
        weights[j] += (i1 - a * i0) / width;
    }
    if let Some(&first) = order.first() {
        weights[first] += at(&f, nid_y[first].1.min(s));
    }
    if let Some(&last) = order.last() {
        weights[last] += f[N] - at(&f, nid_y[last].1.min(s));
    }
    weights
}

/// Nodal forces of the basic-plus-additional lift of a case, N, signed:
/// `F * additional + F_level * basic`, each consistent on the node line. The
/// additional weights are renormalised to one so the nodal sum is exactly `F`.
pub fn shaped_forces_by_y_consistent(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    level_force_n: f64,
    semi_span: f64,
    shape: &alas_geom::wing_structure::SpanLoadShape,
) -> Vec<(i64, f64)> {
    let s = semi_span.max(1e-9);
    let add = consistent_weights(nid_y, semi_span, &|y| shape.density_at(y / s) / s);
    let basic = consistent_weights(nid_y, semi_span, &|y| shape.basic_at(y / s) / s);
    let sum: f64 = add.iter().sum();
    if sum.abs() < 1e-15 {
        let equal = total_force_n / nid_y.len().max(1) as f64;
        return nid_y.iter().map(|&(nid, _)| (nid, equal)).collect();
    }
    let basic_sum = basic.iter().sum::<f64>();
    nid_y
        .iter()
        .zip(add.iter().zip(&basic))
        .map(|(&(nid, _), (&a, &b))| {
            // The basic weights carry no net lift: remove the table residual.
            let b = b - basic_sum * a / sum;
            (nid, total_force_n * a / sum + level_force_n * b)
        })
        .collect()
}

/// Product-deck lift forces on the front-spar node line, signed and summing to
/// `total_force_n`: the deck's VLM lift shape when it carries one, else the
/// half-ellipse (consistent nodal loads in both cases). When the front spar is
/// not at the quarter chord, the share `deck.quarter_chord_rear_share` of each
/// force moves to the nearest rear-spar node so the resultant of each pair sits
/// on the section quarter chord, not on the spar web. Both the MSC and the
/// NASTRAN-95 static decks call this.
pub(crate) fn product_lift_forces(
    deck: &crate::mesh::Deck,
    node_index: &crate::mesh::MeshNodeIndex,
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    level_force_n: f64,
    semi_span: f64,
) -> Vec<(i64, f64)> {
    let base = match &deck.lift_shape {
        Some(shape) => {
            shaped_forces_by_y_consistent(nid_y, total_force_n, level_force_n, semi_span, shape)
        }
        None => elliptic_forces_by_y_consistent(nid_y, total_force_n, semi_span),
    };
    let share = deck.quarter_chord_rear_share;
    let Some(rear) = node_index.spar_upper_nids.last() else {
        return base;
    };
    if share == 0.0 || node_index.spar_upper_nids.len() < 2 {
        return base;
    }
    let mut out = Vec::with_capacity(2 * base.len());
    for (nid, force) in base {
        let y = deck.node_y(nid);
        let rear_nid = rear.iter().copied().min_by(|&a, &b| {
            (deck.node_y(a) - y)
                .abs()
                .total_cmp(&(deck.node_y(b) - y).abs())
        });
        match rear_nid {
            Some(r) => {
                out.push((nid, force * (1.0 - share)));
                out.push((r, force * share));
            }
            None => out.push((nid, force)),
        }
    }
    out
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
        let forces = elliptic_forces_by_y_consistent(&nodes, total, s);
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
        let forces = elliptic_forces_by_y_consistent(&nodes, total, s);
        let moment: f64 = forces.iter().zip(&nodes).map(|(f, n)| f.1 * n.1).sum();
        let exact = total * 4.0 * s / (3.0 * std::f64::consts::PI);
        assert!((moment / exact - 1.0).abs() < 0.005, "{}", moment / exact);
        let sum: f64 = forces.iter().map(|f| f.1).sum();
        assert!((sum - total).abs() < 1e-6 * total);
    }
}

#[cfg(test)]
mod inboard_tests {
    use super::*;

    /// A node line that starts inboard of the root (y = 0.6 m) keeps the
    /// lift of the inboard segment on its first node: the total is the
    /// analytic half-ellipse and the first moment matches the ellipse with
    /// the lumped inboard arm (E-N1G-LOAD-JUMP, renormalisation no longer hides
    /// the missing segment).
    #[test]
    fn lift_inboard_of_the_first_node_is_kept_on_the_first_node() {
        let s = 17.05;
        let nodes: Vec<(i64, f64)> = (0..=20)
            .map(|i| (i, 0.6 + (s - 0.6) * i as f64 / 20.0))
            .collect();
        let total = 1.0e6;
        let forces = elliptic_forces_by_y_consistent(&nodes, total, s);
        let sum: f64 = forces.iter().map(|f| f.1).sum();
        assert!((sum - total).abs() < 1e-9 * total, "{sum}");
        // Lift of the inboard strip 0..0.6 m sits on node 0 at its arm 0.6 m.
        let inboard = ellipse_integrals(0.6, s).0 - ellipse_integrals(0.0, s).0;
        let total_analytic = ellipse_integrals(s, s).0 - ellipse_integrals(0.0, s).0;
        let lumped = inboard / total_analytic * total;
        assert!(
            forces[0].1 >= lumped * (1.0 - 1e-9),
            "{} vs {lumped}",
            forces[0].1
        );
    }
}
