// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The SQP step: the elastic quadratic subproblem and the damped BFGS
//! update of its Hessian.

use super::qp::solve_qp;

/// Penalty on the elastic slacks in the subproblem.
const ELASTIC_PENALTY: f64 = 1.0e4;
/// Curvature added to the elastic slacks so the subproblem Hessian is
/// positive definite.
const ELASTIC_CURVATURE: f64 = 1.0e-6;

/// Assemble and solve the elastic subproblem
/// `min g'd + 0.5 d'Bd + rho sum(t)` subject to `c + Jd <= t`, `t >= 0`
/// and the unit-box bounds on `z + d`. Returns the step over the active
/// variables and the multipliers of the linearised constraints.
pub(super) fn subproblem(
    hessian: &[Vec<f64>],
    grad: &[f64],
    jac: &[Vec<f64>],
    constraints: &[f64],
    z_active: &[f64],
) -> Option<(Vec<f64>, Vec<f64>)> {
    let k = grad.len();
    let m = constraints.len();
    let dim = k + m;
    let mut h = vec![vec![0.0; dim]; dim];
    for (row, source) in h.iter_mut().zip(hessian) {
        row[..k].copy_from_slice(&source[..k]);
    }
    for j in 0..m {
        h[k + j][k + j] = ELASTIC_CURVATURE;
    }
    let mut g = vec![0.0; dim];
    g[..k].copy_from_slice(grad);
    for value in g.iter_mut().skip(k) {
        *value = ELASTIC_PENALTY;
    }
    let mut rows = Vec::with_capacity(2 * m + 2 * k);
    let mut rhs = Vec::with_capacity(2 * m + 2 * k);
    for (j, (jac_row, &c)) in jac.iter().zip(constraints).enumerate() {
        let mut row = vec![0.0; dim];
        row[..k].copy_from_slice(&jac_row[..k]);
        row[k + j] = -1.0;
        rows.push(row);
        rhs.push(-c);
        let mut slack_row = vec![0.0; dim];
        slack_row[k + j] = -1.0;
        rows.push(slack_row);
        rhs.push(0.0);
    }
    for (i, &z) in z_active.iter().enumerate() {
        let mut upper = vec![0.0; dim];
        upper[i] = 1.0;
        rows.push(upper);
        rhs.push(1.0 - z);
        let mut lower = vec![0.0; dim];
        lower[i] = -1.0;
        rows.push(lower);
        rhs.push(z);
    }
    let solution = solve_qp(&h, &g, &rows, &rhs, 1e-9, 200)?;
    let d = solution.x[..k].to_vec();
    let multipliers = (0..m).map(|j| solution.multipliers[2 * j]).collect();
    Some((d, multipliers))
}

/// Damped BFGS update of `b` with step `s` and Lagrangian-gradient change
/// `y` (Nocedal and Wright, Procedure 18.2), which keeps `b` positive
/// definite when the curvature along `s` is not.
pub(super) fn damped_bfgs(b: &mut [Vec<f64>], s: &[f64], y: &[f64]) {
    let bs: Vec<f64> = b
        .iter()
        .map(|row| row.iter().zip(s).map(|(a, x)| a * x).sum())
        .collect();
    let s_bs: f64 = s.iter().zip(&bs).map(|(a, c)| a * c).sum();
    if s_bs <= 0.0 {
        return;
    }
    let s_y: f64 = s.iter().zip(y).map(|(a, c)| a * c).sum();
    let theta = if s_y >= 0.2 * s_bs {
        1.0
    } else {
        0.8 * s_bs / (s_bs - s_y)
    };
    let r: Vec<f64> = y
        .iter()
        .zip(&bs)
        .map(|(yi, bsi)| theta * yi + (1.0 - theta) * bsi)
        .collect();
    let s_r: f64 = s.iter().zip(&r).map(|(a, c)| a * c).sum();
    if s_r <= 1e-14 {
        return;
    }
    for (row, (&bs_i, &r_i)) in b.iter_mut().zip(bs.iter().zip(&r)) {
        for (value, (&bs_j, &r_j)) in row.iter_mut().zip(bs.iter().zip(&r)) {
            *value += -bs_i * bs_j / s_bs + r_i * r_j / s_r;
        }
    }
}

/// The identity matrix, the initial and reset Hessian approximation.
pub(super) fn identity(k: usize) -> Vec<Vec<f64>> {
    (0..k)
        .map(|i| (0..k).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_damped_update_keeps_the_hessian_positive_definite_on_negative_curvature() {
        let mut b = identity(2);
        let s = vec![1.0, 0.0];
        let y = vec![-1.0, 0.0];
        damped_bfgs(&mut b, &s, &y);
        assert!(b[0][0] > 0.0);
        assert!(b[0][0] * b[1][1] - b[0][1] * b[1][0] > 0.0);
    }

    #[test]
    fn an_exact_secant_on_a_quadratic_is_absorbed_by_the_update() {
        // For f = 0.5 x' diag(4, 1) x the secant pair (s, y = A s) makes
        // B s = A s after one update along s.
        let mut b = identity(2);
        let s = vec![1.0, 0.0];
        let y = vec![4.0, 0.0];
        damped_bfgs(&mut b, &s, &y);
        let bs: Vec<f64> = b.iter().map(|row| row[0]).collect();
        assert!((bs[0] - 4.0).abs() < 1e-12 && bs[1].abs() < 1e-12, "{b:?}");
    }

    #[test]
    fn the_subproblem_returns_a_feasible_step_and_positive_multiplier_on_an_active_row() {
        // Minimise -d with d in [0, 1] and constraint -0.4 + d <= 0.
        let hessian = identity(1);
        let (d, lambda) = subproblem(&hessian, &[-1.0], &[vec![1.0]], &[-0.4], &[0.0])
            .unwrap_or_else(|| panic!("solves"));
        assert!((d[0] - 0.4).abs() < 1e-5, "{d:?}");
        assert!(lambda[0] > 0.5, "{lambda:?}");
    }
}
