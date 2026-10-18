// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Penalised, constrained least-squares fit of a clamped B-spline to weighted
//! samples.
//!
//! For a fixed knot vector the spline `s(u) = sum_i c_i N_i(u)` is linear in
//! the coefficients, so
//!
//! ```text
//! minimise   sum_k w_k (s(x_k) - y_k)^2  +  lambda * integral (s^(d)(u))^2 du
//! subject to s^(order_j)(x_j) = value_j                (equality constraints)
//! ```
//!
//! is a quadratic programme with a unique solution when the data and the
//! constraints fix the null space of the roughness penalty. With `d = 3` the
//! penalty is the fairness energy of a curve whose third derivative is the
//! change of curvature (the minimum-variation family of fair curves); the
//! quadratics it ignores are exactly the shapes the data must then pin down.
//!
//! The equality constraints are eliminated directly (Gauss-Jordan with complete
//! pivoting expresses the pivot coefficients through the free ones), and the
//! remaining unconstrained problem is solved as one stacked least-squares
//! system `[sqrt(W) B; sqrt(lambda) L] c = [sqrt(W) y; 0]`, with `L` the
//! penalty evaluated at Gauss points, by the Householder QR of
//! [`crate::lstsq`]. The normal equations are deliberately not formed: a strong
//! penalty makes `B^T W B + lambda R` ill conditioned by the square of the
//! penalty-to-data ratio, which costs the exactness on the penalty's null
//! space (quadratics for `d = 3`) that the stacked form keeps. The coefficient
//! count is a few dozen, so the dense system costs microseconds.
//!
//! `lambda` is dimensional: it multiplies the integral over the knot domain of
//! the squared `penalty_order`-th derivative with respect to the spline
//! argument, in units of `y^2 / u^(2 d - 1)`. Callers normalise the argument to
//! `[0, 1]` and pick `lambda` relative to the data weights.

use crate::clamped_bspline::{
    basis_derivatives, find_span, ClampedBSpline, ClampedBSplineError, MAX_DERIVATIVE,
};
use crate::gauss::gauss_legendre;
use crate::lstsq::{least_squares, LeastSquaresError};

/// A weighted data point `(x, y)`; the squared residual is multiplied by
/// `weight`. A zero weight drops the sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitSample {
    /// Abscissa, inside the spline domain.
    pub x: f64,
    /// Ordinate.
    pub y: f64,
    /// Non-negative weight.
    pub weight: f64,
}

/// A hard linear equality: the `order`-th derivative at `x` equals `value`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitConstraint {
    /// Abscissa, inside the spline domain.
    pub x: f64,
    /// Derivative order, `0..=3`.
    pub order: usize,
    /// Required value.
    pub value: f64,
}

/// What a fit returns.
#[derive(Debug, Clone, PartialEq)]
pub struct FitOutcome {
    /// The fitted spline.
    pub spline: ClampedBSpline,
    /// Root-mean-square residual over samples with positive weight.
    pub rms_residual: f64,
    /// Largest absolute residual over samples with positive weight.
    pub max_residual: f64,
    /// Largest violation of any equality constraint.
    pub max_constraint_error: f64,
}

/// Why a fit failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SplineFitError {
    /// The knot vector or degree is invalid.
    #[error("invalid spline: {0}")]
    Spline(#[from] ClampedBSplineError),
    /// Penalty order outside `1..=3` or above the degree.
    #[error("penalty order {order} is outside 1..=min(3, degree {degree})")]
    PenaltyOrder {
        /// Order requested.
        order: usize,
        /// Spline degree.
        degree: usize,
    },
    /// A constraint asks for a derivative order above 3.
    #[error("constraint derivative order {0} is above {MAX_DERIVATIVE}")]
    ConstraintOrder(usize),
    /// A sample, constraint or `lambda` is not finite, a weight is negative,
    /// or an abscissa lies outside the domain.
    #[error("non-finite or out-of-domain input in {0}")]
    BadInput(&'static str),
    /// No sample has positive weight and there are no constraints.
    #[error("no data")]
    NoData,
    /// The stacked least-squares problem is rank-deficient or underdetermined:
    /// the data and constraints do not determine the spline (too few samples
    /// for `lambda = 0`, or unsampled spans with a penalty that ignores them).
    #[error("the fit is not determined by the data and constraints: {0}")]
    Undetermined(#[from] LeastSquaresError),
}

/// Fit a clamped B-spline of `degree` on `knots` to `samples` subject to
/// `constraints`, penalising the `penalty_order`-th derivative with `lambda`.
///
/// # Errors
///
/// [`SplineFitError`] for an invalid knot vector, bad inputs, or a system the
/// data cannot determine.
pub fn fit_penalised(
    knots: Vec<f64>,
    degree: usize,
    samples: &[FitSample],
    constraints: &[FitConstraint],
    lambda: f64,
    penalty_order: usize,
) -> Result<FitOutcome, SplineFitError> {
    let n = knots
        .len()
        .checked_sub(degree + 1)
        .ok_or(ClampedBSplineError::Counts {
            knots: knots.len(),
            coeffs: 0,
            degree,
        })?;
    // Validates the knot vector; the coefficients are replaced below.
    let template = ClampedBSpline::new(knots.clone(), vec![0.0; n], degree)?;
    if penalty_order == 0 || penalty_order > MAX_DERIVATIVE || penalty_order > degree {
        return Err(SplineFitError::PenaltyOrder {
            order: penalty_order,
            degree,
        });
    }
    if !lambda.is_finite() || lambda < 0.0 {
        return Err(SplineFitError::BadInput("lambda"));
    }
    let (lo, hi) = template.domain();
    let in_domain = |x: f64| x.is_finite() && x >= lo - 1e-12 && x <= hi + 1e-12;
    for s in samples {
        if !(in_domain(s.x) && s.y.is_finite() && s.weight.is_finite() && s.weight >= 0.0) {
            return Err(SplineFitError::BadInput("samples"));
        }
    }
    for c in constraints {
        if c.order > MAX_DERIVATIVE {
            return Err(SplineFitError::ConstraintOrder(c.order));
        }
        if !(in_domain(c.x) && c.value.is_finite()) {
            return Err(SplineFitError::BadInput("constraints"));
        }
    }
    if constraints.is_empty() && samples.iter().all(|s| s.weight == 0.0) {
        return Err(SplineFitError::NoData);
    }

    // Rows of the stacked least-squares matrix: sqrt(w) B for the data and
    // sqrt(lambda w_q) B^(d) at quadrature nodes for the roughness penalty. The
    // penalty integrand is a polynomial of degree 2 (p - d) on each span, so
    // p - d + 1 Gauss points integrate it exactly.
    let mut rows: Vec<Vec<f64>> = Vec::new();
    let mut target: Vec<f64> = Vec::new();
    for s in samples.iter().filter(|s| s.weight > 0.0) {
        let x = s.x.clamp(lo, hi);
        let span = find_span(&knots, degree, n, x);
        let table = basis_derivatives(&knots, degree, span, x);
        let root = s.weight.sqrt();
        let mut row = vec![0.0; n];
        for a in 0..=degree {
            row[span - degree + a] = root * table[0][a];
        }
        rows.push(row);
        target.push(root * s.y);
    }
    if lambda > 0.0 {
        let rule = gauss_legendre(degree - penalty_order + 1);
        for span in degree..n {
            let (a, b) = (knots[span], knots[span + 1]);
            if b <= a {
                continue;
            }
            for (u, w) in rule.on_interval(a, b) {
                let table = basis_derivatives(&knots, degree, span, u);
                let root = (lambda * w).sqrt();
                let mut row = vec![0.0; n];
                for j in 0..=degree {
                    row[span - degree + j] = root * table[penalty_order][j];
                }
                rows.push(row);
                target.push(0.0);
            }
        }
    }

    // Equality constraints by direct elimination: Gauss-Jordan with complete
    // pivoting expresses the pivot coefficients through the free ones,
    // c = c_part + T c_free, so the constrained problem becomes an
    // unconstrained one in the free coefficients. Rows are normalised to a
    // unit largest entry; a row that elimination reduces to zero is redundant
    // if its right-hand side vanishes too and contradictory otherwise.
    let mut cmat: Vec<Vec<f64>> = Vec::with_capacity(constraints.len());
    let mut cval: Vec<f64> = Vec::with_capacity(constraints.len());
    for c in constraints {
        let x = c.x.clamp(lo, hi);
        let span = find_span(&knots, degree, n, x);
        let table = basis_derivatives(&knots, degree, span, x);
        let scale = table[c.order][..=degree]
            .iter()
            .fold(0.0_f64, |m, v| m.max(v.abs()))
            .max(f64::MIN_POSITIVE);
        let mut row = vec![0.0; n];
        for a in 0..=degree {
            row[span - degree + a] = table[c.order][a] / scale;
        }
        cmat.push(row);
        cval.push(c.value / scale);
    }
    let mut pivots: Vec<(usize, usize)> = Vec::new();
    let mut pivot_col = vec![false; n];
    let mut row_done = vec![false; cmat.len()];
    for _ in 0..cmat.len() {
        let mut best = (0.0_f64, 0usize, 0usize);
        for (r, row) in cmat.iter().enumerate().filter(|(r, _)| !row_done[*r]) {
            for (c, v) in row.iter().enumerate().filter(|(c, _)| !pivot_col[*c]) {
                if v.abs() > best.0 {
                    best = (v.abs(), r, c);
                }
            }
        }
        if best.0 < 1e-11 {
            break;
        }
        let (_, r, c) = best;
        let p = cmat[r][c];
        for v in &mut cmat[r] {
            *v /= p;
        }
        cval[r] /= p;
        let pivot_row = cmat[r].clone();
        let pivot_val = cval[r];
        for k in 0..cmat.len() {
            if k != r {
                let f = cmat[k][c];
                if f != 0.0 {
                    for (v, pv) in cmat[k].iter_mut().zip(&pivot_row) {
                        *v -= f * pv;
                    }
                    cval[k] -= f * pivot_val;
                }
            }
        }
        row_done[r] = true;
        pivot_col[c] = true;
        pivots.push((r, c));
    }
    if (0..cmat.len()).any(|r| !row_done[r] && cval[r].abs() > 1e-9) {
        return Err(SplineFitError::BadInput("contradictory constraints"));
    }
    let free: Vec<usize> = (0..n).filter(|&c| !pivot_col[c]).collect();
    let mut c_part = vec![0.0; n];
    for &(r, c) in &pivots {
        c_part[c] = cval[r];
    }

    let coeffs = if free.is_empty() {
        c_part
    } else {
        let mut reduced: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
        let mut reduced_target: Vec<f64> = Vec::with_capacity(rows.len());
        for (row, &t) in rows.iter().zip(&target) {
            let mut out: Vec<f64> = free.iter().map(|&f| row[f]).collect();
            let mut shift = t;
            for &(r, c) in &pivots {
                let a = row[c];
                if a != 0.0 {
                    for (o, &f) in out.iter_mut().zip(&free) {
                        *o -= a * cmat[r][f];
                    }
                    shift -= a * cval[r];
                }
            }
            reduced.push(out);
            reduced_target.push(shift);
        }
        let free_values = least_squares(&reduced, &reduced_target)?;
        let mut coeffs = c_part;
        for (&f, &v) in free.iter().zip(&free_values) {
            coeffs[f] = v;
        }
        for &(r, c) in &pivots {
            let mut value = cval[r];
            for (&f, &v) in free.iter().zip(&free_values) {
                value -= cmat[r][f] * v;
            }
            coeffs[c] = value;
        }
        coeffs
    };
    if coeffs.iter().any(|c| !c.is_finite()) {
        return Err(SplineFitError::BadInput("non-finite solution"));
    }
    let spline = ClampedBSpline::new(knots, coeffs, degree)?;

    let (mut sum_sq, mut count, mut max_residual) = (0.0, 0usize, 0.0_f64);
    for s in samples.iter().filter(|s| s.weight > 0.0) {
        let r = spline.eval(s.x) - s.y;
        sum_sq += r * r;
        count += 1;
        max_residual = max_residual.max(r.abs());
    }
    let max_constraint_error = constraints.iter().fold(0.0_f64, |m, c| {
        let got = spline.derivatives(c.x)[c.order];
        m.max((got - c.value).abs())
    });
    Ok(FitOutcome {
        spline,
        rms_residual: if count == 0 {
            0.0
        } else {
            (sum_sq / count as f64).sqrt()
        },
        max_residual,
        max_constraint_error,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn knots(n: usize, p: usize) -> Vec<f64> {
        let interior = n - p - 1;
        let mut k = vec![0.0; p + 1];
        for i in 1..=interior {
            k.push(i as f64 / (interior + 1) as f64);
        }
        k.extend(std::iter::repeat_n(1.0, p + 1));
        k
    }

    fn samples(f: impl Fn(f64) -> f64, count: usize) -> Vec<FitSample> {
        (0..count)
            .map(|i| {
                let x = i as f64 / (count - 1) as f64;
                FitSample {
                    x,
                    y: f(x),
                    weight: 1.0,
                }
            })
            .collect()
    }

    #[test]
    fn bspline_fit_reproduces_a_quintic_polynomial_without_penalty() {
        let f = |u: f64| 0.3 - u + 2.0 * u * u - 1.5 * u.powi(3) + u.powi(4) - 0.4 * u.powi(5);
        let out = fit_penalised(knots(10, 5), 5, &samples(f, 80), &[], 0.0, 3).unwrap();
        assert!(out.max_residual < 1e-10, "residual {}", out.max_residual);
    }

    #[test]
    fn bspline_fit_with_third_derivative_penalty_keeps_quadratics_exactly() {
        // The penalty has the quadratics in its null space, so even a large
        // lambda must leave a quadratic data set untouched.
        let f = |u: f64| 1.0 + 2.0 * u - 3.0 * u * u;
        let out = fit_penalised(knots(12, 5), 5, &samples(f, 60), &[], 1e3, 3).unwrap();
        assert!(out.max_residual < 1e-8, "residual {}", out.max_residual);
    }

    #[test]
    fn bspline_fit_honours_value_and_derivative_constraints_exactly() {
        let f = |u: f64| (3.0 * u).sin();
        let constraints = [
            FitConstraint {
                x: 0.0,
                order: 0,
                value: 0.0,
            },
            FitConstraint {
                x: 0.0,
                order: 1,
                value: 3.0,
            },
            FitConstraint {
                x: 1.0,
                order: 1,
                value: 0.0,
            },
            FitConstraint {
                x: 1.0,
                order: 2,
                value: 0.0,
            },
            FitConstraint {
                x: 1.0,
                order: 3,
                value: 0.0,
            },
        ];
        let out = fit_penalised(knots(14, 5), 5, &samples(f, 100), &constraints, 1e-6, 3).unwrap();
        assert!(
            out.max_constraint_error < 1e-9,
            "{}",
            out.max_constraint_error
        );
        let end = out.spline.derivatives(1.0);
        assert!(end[1].abs() < 1e-9 && end[2].abs() < 1e-8 && end[3].abs() < 1e-6);
    }

    #[test]
    fn bspline_fit_smooths_noise_and_penalty_reduces_roughness() {
        // Deterministic pseudo-noise of amplitude 0.01 on a smooth curve.
        let noisy: Vec<FitSample> = (0..200)
            .map(|i| {
                let x = f64::from(i) / 199.0;
                let noise = 0.01 * ((f64::from(i) * 12.9898).sin() * 43758.5453).fract();
                FitSample {
                    x,
                    y: (2.0 * x).exp() + noise,
                    weight: 1.0,
                }
            })
            .collect();
        let rough = fit_penalised(knots(30, 5), 5, &noisy, &[], 0.0, 3).unwrap();
        let smooth = fit_penalised(knots(30, 5), 5, &noisy, &[], 1e-7, 3).unwrap();
        let err = |o: &FitOutcome| {
            (0..=100)
                .map(|i| {
                    let x = f64::from(i) / 100.0;
                    (o.spline.eval(x) - (2.0 * x).exp()).abs()
                })
                .fold(0.0_f64, f64::max)
        };
        assert!(
            err(&smooth) < err(&rough),
            "{} vs {}",
            err(&smooth),
            err(&rough)
        );
        assert!(err(&smooth) < 0.02);
    }

    #[test]
    fn bspline_fit_rejects_bad_input() {
        let s = samples(|u| u, 20);
        assert!(matches!(
            fit_penalised(knots(8, 3), 3, &s, &[], -1.0, 3),
            Err(SplineFitError::BadInput(_))
        ));
        assert!(matches!(
            fit_penalised(knots(8, 3), 3, &s, &[], 0.0, 4),
            Err(SplineFitError::PenaltyOrder { .. })
        ));
        assert!(matches!(
            fit_penalised(knots(8, 3), 3, &[], &[], 1.0, 3),
            Err(SplineFitError::NoData)
        ));
    }
}
