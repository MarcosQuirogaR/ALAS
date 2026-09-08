// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A dense convex quadratic program with linear inequality constraints,
//! `min 0.5 x'Hx + g'x  s.t.  Ax <= b`, solved by Mehrotra's
//! predictor-corrector primal-dual interior-point method (J. Nocedal and
//! S. J. Wright, *Numerical Optimization*, 2nd ed., Algorithm 16.4).
//!
//! Each iteration eliminates the slack and dual steps and solves one
//! `n x n` symmetric positive-definite system `(H + A' D A) dx = rhs` with
//! `D = diag(lambda / s)` by the dense elimination in `alas-math`. The
//! subproblems an SQP driver poses here have a few tens of variables and
//! constraints, for which this is exact and cheap; there is no sparsity to
//! exploit.

/// The solution of one quadratic program.
#[derive(Debug, Clone, PartialEq)]
pub struct QpSolution {
    /// Primal minimiser.
    pub x: Vec<f64>,
    /// Lagrange multipliers of the inequality rows, nonnegative.
    pub multipliers: Vec<f64>,
    /// Whether the residuals fell below the tolerance.
    pub converged: bool,
    /// Interior-point iterations taken.
    pub iterations: usize,
}

/// Fraction-to-the-boundary rule: the step keeps every slack and dual
/// strictly positive with this margin.
const BOUNDARY_FRACTION: f64 = 0.995;

fn matvec(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    matrix
        .iter()
        .map(|row| row.iter().zip(vector).map(|(a, x)| a * x).sum())
        .collect()
}

fn transpose_matvec(matrix: &[Vec<f64>], columns: usize, vector: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; columns];
    for (row, &weight) in matrix.iter().zip(vector) {
        for (target, &a) in out.iter_mut().zip(row) {
            *target += a * weight;
        }
    }
    out
}

fn infinity_norm(values: &[f64]) -> f64 {
    values.iter().fold(0.0, |acc, v| acc.max(v.abs()))
}

/// Largest `alpha` in `[0, 1]` with `values + alpha * steps >= 0`.
fn max_step(values: &[f64], steps: &[f64]) -> f64 {
    values
        .iter()
        .zip(steps)
        .filter(|(_, &step)| step < 0.0)
        .fold(1.0, |alpha, (&value, &step)| alpha.min(-value / step))
}

/// Solve the reduced Newton system for the primal step given the
/// complementarity right-hand side `r_c`.
fn primal_step(
    hessian: &[Vec<f64>],
    constraints: &[Vec<f64>],
    dual_over_slack: &[f64],
    r_d: &[f64],
    r_p: &[f64],
    r_c: &[f64],
    slack: &[f64],
) -> Option<Vec<f64>> {
    let n = hessian.len();
    let mut system: Vec<Vec<f64>> = hessian.to_vec();
    for (row, &d) in constraints.iter().zip(dual_over_slack) {
        for i in 0..n {
            for j in 0..n {
                system[i][j] += row[i] * d * row[j];
            }
        }
    }
    let inner: Vec<f64> = dual_over_slack
        .iter()
        .zip(r_p)
        .zip(r_c)
        .zip(slack)
        .map(|(((&d, &rp), &rc), &s)| d * rp - rc / s)
        .collect();
    let projected = transpose_matvec(constraints, n, &inner);
    let rhs: Vec<Vec<f64>> = r_d
        .iter()
        .zip(&projected)
        .map(|(&rd, &p)| vec![-rd - p])
        .collect();
    let solved = match alas_math::linalg::solve(&system, &rhs) {
        Ok(solution) => solution,
        Err(_) => {
            // A pivot vanished: regularise the diagonal once and retry.
            let scale = 1e-10
                * (1.0
                    + system
                        .iter()
                        .enumerate()
                        .map(|(i, r)| r[i].abs())
                        .fold(0.0, f64::max));
            for (i, row) in system.iter_mut().enumerate() {
                row[i] += scale;
            }
            alas_math::linalg::solve(&system, &rhs).ok()?
        }
    };
    Some(solved.into_iter().map(|row| row[0]).collect())
}

/// Solve `min 0.5 x'Hx + g'x` subject to `A x <= b`.
///
/// `hessian` must be symmetric positive definite (a damped BFGS matrix
/// is). Returns `None` only when the linear algebra breaks down; an
/// unconverged but finite iterate is returned with `converged == false`.
pub fn solve_qp(
    hessian: &[Vec<f64>],
    gradient: &[f64],
    constraints: &[Vec<f64>],
    rhs: &[f64],
    tolerance: f64,
    max_iterations: usize,
) -> Option<QpSolution> {
    let n = gradient.len();
    let m = rhs.len();
    if m == 0 {
        let b: Vec<Vec<f64>> = gradient.iter().map(|&g| vec![-g]).collect();
        let x = alas_math::linalg::solve(hessian, &b).ok()?;
        return Some(QpSolution {
            x: x.into_iter().map(|row| row[0]).collect(),
            multipliers: Vec::new(),
            converged: true,
            iterations: 0,
        });
    }

    let mut x = vec![0.0; n];
    let ax = matvec(constraints, &x);
    let mut slack: Vec<f64> = rhs
        .iter()
        .zip(&ax)
        .map(|(&b, &a)| (b - a).max(1.0))
        .collect();
    let mut dual = vec![1.0; m];
    let scale_d = 1.0 + infinity_norm(gradient);
    let scale_p = 1.0 + infinity_norm(rhs);
    let mut converged = false;
    let mut iterations = 0;

    for iteration in 0..max_iterations {
        iterations = iteration;
        let hx = matvec(hessian, &x);
        let at_lambda = transpose_matvec(constraints, n, &dual);
        let r_d: Vec<f64> = (0..n).map(|i| hx[i] + gradient[i] + at_lambda[i]).collect();
        let ax = matvec(constraints, &x);
        let r_p: Vec<f64> = (0..m).map(|j| ax[j] + slack[j] - rhs[j]).collect();
        let mu = slack.iter().zip(&dual).map(|(s, l)| s * l).sum::<f64>() / m as f64;
        if infinity_norm(&r_d) <= tolerance * scale_d
            && infinity_norm(&r_p) <= tolerance * scale_p
            && mu <= tolerance
        {
            converged = true;
            break;
        }
        let dual_over_slack: Vec<f64> = dual.iter().zip(&slack).map(|(l, s)| l / s).collect();

        // Predictor (affine) step.
        let r_c_affine: Vec<f64> = slack.iter().zip(&dual).map(|(s, l)| s * l).collect();
        let dx_affine = primal_step(
            hessian,
            constraints,
            &dual_over_slack,
            &r_d,
            &r_p,
            &r_c_affine,
            &slack,
        )?;
        let a_dx = matvec(constraints, &dx_affine);
        let dl_affine: Vec<f64> = (0..m)
            .map(|j| dual_over_slack[j] * (a_dx[j] + r_p[j]) - r_c_affine[j] / slack[j])
            .collect();
        let ds_affine: Vec<f64> = (0..m)
            .map(|j| -(r_c_affine[j] + slack[j] * dl_affine[j]) / dual[j])
            .collect();
        let alpha_p = max_step(&slack, &ds_affine);
        let alpha_d = max_step(&dual, &dl_affine);
        let mu_affine = (0..m)
            .map(|j| (slack[j] + alpha_p * ds_affine[j]) * (dual[j] + alpha_d * dl_affine[j]))
            .sum::<f64>()
            / m as f64;
        let sigma = (mu_affine / mu).max(0.0).powi(3).min(1.0);

        // Corrector step.
        let r_c: Vec<f64> = (0..m)
            .map(|j| slack[j] * dual[j] + ds_affine[j] * dl_affine[j] - sigma * mu)
            .collect();
        let dx = primal_step(
            hessian,
            constraints,
            &dual_over_slack,
            &r_d,
            &r_p,
            &r_c,
            &slack,
        )?;
        let a_dx = matvec(constraints, &dx);
        let dl: Vec<f64> = (0..m)
            .map(|j| dual_over_slack[j] * (a_dx[j] + r_p[j]) - r_c[j] / slack[j])
            .collect();
        let ds: Vec<f64> = (0..m)
            .map(|j| -(r_c[j] + slack[j] * dl[j]) / dual[j])
            .collect();
        let alpha_p = (BOUNDARY_FRACTION * max_step(&slack, &ds)).min(1.0);
        let alpha_d = (BOUNDARY_FRACTION * max_step(&dual, &dl)).min(1.0);
        for i in 0..n {
            x[i] += alpha_p * dx[i];
        }
        for j in 0..m {
            slack[j] = (slack[j] + alpha_p * ds[j]).max(1e-300);
            dual[j] = (dual[j] + alpha_d * dl[j]).max(1e-300);
        }
        if x.iter().any(|v| !v.is_finite()) {
            return None;
        }
    }

    Some(QpSolution {
        x,
        multipliers: dual,
        converged,
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: usize) -> Vec<Vec<f64>> {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
            .collect()
    }

    #[test]
    fn an_unconstrained_qp_is_the_newton_step() {
        let h = vec![vec![2.0, 0.0], vec![0.0, 4.0]];
        let g = vec![-2.0, -8.0];
        let solution = solve_qp(&h, &g, &[], &[], 1e-10, 50).unwrap_or_else(|| panic!("solves"));
        assert!((solution.x[0] - 1.0).abs() < 1e-12);
        assert!((solution.x[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn an_active_constraint_moves_the_minimiser_onto_it_with_a_positive_multiplier() {
        // min (x-2)^2 + (y-2)^2 subject to x + y <= 2: minimiser (1, 1),
        // multiplier 2 (gradient of f is (-2, -2), so lambda (1, 1) = (2, 2)).
        let h = vec![vec![2.0, 0.0], vec![0.0, 2.0]];
        let g = vec![-4.0, -4.0];
        let a = vec![vec![1.0, 1.0]];
        let b = vec![2.0];
        let solution = solve_qp(&h, &g, &a, &b, 1e-10, 100).unwrap_or_else(|| panic!("solves"));
        assert!(solution.converged, "{solution:?}");
        assert!((solution.x[0] - 1.0).abs() < 1e-6, "{:?}", solution.x);
        assert!((solution.x[1] - 1.0).abs() < 1e-6);
        assert!((solution.multipliers[0] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn inactive_constraints_leave_the_unconstrained_minimiser_and_carry_zero_multipliers() {
        let h = identity(3);
        let g = vec![-1.0, -2.0, -3.0];
        // x_i <= 10 for every i: all slack.
        let a = identity(3);
        let b = vec![10.0; 3];
        let solution = solve_qp(&h, &g, &a, &b, 1e-10, 100).unwrap_or_else(|| panic!("solves"));
        assert!(solution.converged);
        for (i, expected) in [1.0, 2.0, 3.0].iter().enumerate() {
            assert!((solution.x[i] - expected).abs() < 1e-6);
            assert!(solution.multipliers[i] < 1e-6);
        }
    }

    #[test]
    fn box_bounds_written_as_rows_are_respected_when_the_minimiser_lies_outside() {
        // min (x-5)^2 with 0 <= x <= 1 -> x = 1.
        let h = vec![vec![2.0]];
        let g = vec![-10.0];
        let a = vec![vec![1.0], vec![-1.0]];
        let b = vec![1.0, 0.0];
        let solution = solve_qp(&h, &g, &a, &b, 1e-10, 100).unwrap_or_else(|| panic!("solves"));
        assert!(solution.converged);
        assert!((solution.x[0] - 1.0).abs() < 1e-6, "{:?}", solution.x);
        assert!(solution.multipliers[0] > 7.0 && solution.multipliers[1] < 1e-6);
    }

    #[test]
    fn an_infeasible_start_is_handled_by_the_infeasible_interior_method() {
        // x >= 3 written as -x <= -3, with min (x-0)^2: the initial slack
        // guess violates the row; the iterates must still reach x = 3.
        let h = vec![vec![2.0]];
        let g = vec![0.0];
        let a = vec![vec![-1.0]];
        let b = vec![-3.0];
        let solution = solve_qp(&h, &g, &a, &b, 1e-10, 100).unwrap_or_else(|| panic!("solves"));
        assert!(solution.converged);
        assert!((solution.x[0] - 3.0).abs() < 1e-6, "{:?}", solution.x);
    }
}
