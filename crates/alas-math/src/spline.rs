// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A cubic spline through a sequence of points, with an independently chosen
//! first- or second-derivative condition at each end.
//!
//! `docs/PORTING.md` carries this row with no third-party provenance: nothing
//! here is translated from a specific file. It exists because the future
//! `alas-geom::aircraft::airfoil` repanel needs the same construction SciPy's
//! `CubicSpline` provides -- resampling an airfoil's coordinates onto new
//! stations while pinning the leading-edge tangent and leaving the trailing
//! edge's curvature free -- and that construction has one mathematically
//! correct answer given the same knots, values and boundary conditions,
//! independent of which of several equivalent linear systems a particular
//! implementation happens to solve. This module solves the classical one:
//! Burden & Faires, *Numerical Analysis*, the cubic spline section, via the
//! second derivatives ("moments") at each knot. SciPy's own `CubicSpline`
//! solves an equivalent system in terms of first derivatives instead; the two
//! agree to a handful of ulps because they describe the same unique spline,
//! not because either copies the other -- confirmed directly against SciPy's
//! output for this crate's fixture, at the `linalg` tier the disagreement
//! between two different linear solves is sized for.
//!
//! # Boundary conditions
//!
//! Each end independently takes either [`Boundary::FirstDerivative`] (the
//! spline's slope at that end is pinned to a given vector -- "clamped", in
//! SciPy's terms) or [`Boundary::SecondDerivative`] (the spline's curvature
//! is pinned instead -- "natural" when the value is zero). SciPy's
//! `CubicSpline` also offers `'not-a-knot'` and `'periodic'` as whole-spline
//! presets; neither is translated here because the one caller this module is
//! built for (`Airfoil.repanel`, via `scipy.interpolate.CubicSpline(...,
//! bc_type=((2, (0, 0)), (1, (0, -1))))`) always states an explicit
//! first- or second-derivative condition at both ends, so those presets have
//! no case to cover yet.

/// A condition pinning one end of a spline.
#[derive(Debug, Clone, Copy)]
pub enum Boundary<'a> {
    /// The spline's first derivative at this end equals the given
    /// per-dimension vector ("clamped").
    FirstDerivative(&'a [f64]),
    /// The spline's second derivative at this end equals the given
    /// per-dimension vector ("natural" when every entry is zero).
    SecondDerivative(&'a [f64]),
}

/// Why a [`CubicSpline`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CubicSplineError {
    /// Fewer than two knots were given; a spline needs at least one segment.
    #[error("a cubic spline needs at least 2 points, got {0}")]
    TooFewPoints(usize),
    /// `y` did not have one value per knot in `x`.
    #[error("x has {expected} points but y has {actual} values")]
    LengthMismatch {
        /// Number of knots in `x`.
        expected: usize,
        /// Number of value rows in `y`.
        actual: usize,
    },
    /// `x` was not strictly increasing.
    #[error("knot {index} ({value}) is not strictly greater than the previous knot")]
    KnotsNotIncreasing {
        /// The offending knot's index.
        index: usize,
        /// The offending knot's value.
        value: f64,
    },
    /// A row of `y`, or a boundary condition's value slice, did not have the
    /// dimension the first row of `y` established.
    #[error(
        "expected every value to have dimension {expected}, but {what} at index {index} has {actual}"
    )]
    DimensionMismatch {
        /// What was mismatched: `"y"` or a boundary's label.
        what: &'static str,
        /// The offending item's index (0 for a boundary condition).
        index: usize,
        /// The dimension every other value has.
        expected: usize,
        /// The offending item's actual dimension.
        actual: usize,
    },
}

/// A cubic spline through `(x[i], y[i])`, evaluable at any `x`.
///
/// `y` is vector-valued (an airfoil coordinate is an `(x, y)` pair), so every
/// value is stored and interpolated as a same-length `Vec<f64>` rather than a
/// single `f64`; a scalar spline is the one-dimensional case of the same
/// construction.
#[derive(Debug, Clone)]
pub struct CubicSpline {
    x: Vec<f64>,
    y: Vec<Vec<f64>>,
    /// The second derivative ("moment") at each knot, one vector per knot,
    /// each of the same dimension as `y`.
    moments: Vec<Vec<f64>>,
}

impl CubicSpline {
    /// Build the spline through `x`/`y`, pinned by `lower`/`upper` at
    /// `x[0]`/`x[last]` respectively.
    ///
    /// `x` must be strictly increasing and have at least 2 entries; `y` must
    /// have the same length as `x`, with every row (and every boundary
    /// condition's value) the same dimension as `y[0]`.
    ///
    /// # Errors
    ///
    /// See [`CubicSplineError`].
    pub fn new(
        x: &[f64],
        y: &[Vec<f64>],
        lower: Boundary<'_>,
        upper: Boundary<'_>,
    ) -> Result<Self, CubicSplineError> {
        let n_points = x.len();
        if n_points < 2 {
            return Err(CubicSplineError::TooFewPoints(n_points));
        }
        if y.len() != n_points {
            return Err(CubicSplineError::LengthMismatch {
                expected: n_points,
                actual: y.len(),
            });
        }
        for (index, pair) in x.windows(2).enumerate() {
            if pair[1] <= pair[0] {
                return Err(CubicSplineError::KnotsNotIncreasing {
                    index: index + 1,
                    value: pair[1],
                });
            }
        }
        let dimension = y[0].len();
        for (index, row) in y.iter().enumerate() {
            if row.len() != dimension {
                return Err(CubicSplineError::DimensionMismatch {
                    what: "y",
                    index,
                    expected: dimension,
                    actual: row.len(),
                });
            }
        }
        for (label, boundary) in [("lower boundary", lower), ("upper boundary", upper)] {
            let value = match boundary {
                Boundary::FirstDerivative(v) | Boundary::SecondDerivative(v) => v,
            };
            if value.len() != dimension {
                return Err(CubicSplineError::DimensionMismatch {
                    what: label,
                    index: 0,
                    expected: dimension,
                    actual: value.len(),
                });
            }
        }

        let moments = solve_moments(x, y, lower, upper, dimension);

        Ok(Self {
            x: x.to_vec(),
            y: y.to_vec(),
            moments,
        })
    }

    /// The spline's value at `query_x`.
    ///
    /// Outside `[x[0], x[last]]`, this extrapolates using the polynomial of
    /// the nearest segment -- the same default `CubicSpline(...,
    /// extrapolate=True)` uses upstream -- rather than clamping or returning
    /// `NaN`, since nothing that calls this needs either of those yet.
    pub fn evaluate(&self, query_x: f64) -> Vec<f64> {
        let segment = self.segment_for(query_x);
        let h = self.x[segment + 1] - self.x[segment];
        let a = (self.x[segment + 1] - query_x) / h;
        let b = (query_x - self.x[segment]) / h;
        let h2_over_6 = h * h / 6.0;

        (0..self.y[0].len())
            .map(|dim| {
                a * self.y[segment][dim]
                    + b * self.y[segment + 1][dim]
                    + ((a.powi(3) - a) * self.moments[segment][dim]
                        + (b.powi(3) - b) * self.moments[segment + 1][dim])
                        * h2_over_6
            })
            .collect()
    }

    /// The index `i` such that `query_x` falls in segment `[x[i], x[i+1]]`,
    /// clamped to the first/last segment when `query_x` is outside the knot
    /// range (which is what makes extrapolation use the nearest segment's
    /// polynomial rather than panicking or requiring a separate code path).
    fn segment_for(&self, query_x: f64) -> usize {
        let last_segment = self.x.len() - 2;
        match self.x[1..self.x.len() - 1]
            .iter()
            .position(|&knot| query_x < knot)
        {
            Some(offset) => offset,
            None => last_segment,
        }
    }
}

/// Solve for the second derivative at every knot, for every dimension.
///
/// The classical tridiagonal system (Burden & Faires): for each interior
/// knot `i`, continuity of the first derivative across `x[i]` gives
/// `h[i-1]*M[i-1] + 2*(h[i-1]+h[i])*M[i] + h[i]*M[i+1] =
/// 6*((y[i+1]-y[i])/h[i] - (y[i]-y[i-1])/h[i-1])`.
/// The two boundary rows come from the same
/// continuity argument applied at the ends: a second-derivative condition
/// pins `M` directly; a first-derivative condition instead constrains the
/// adjacent pair (`M[0]`/`M[1]` or `M[n-1]`/`M[n]`) through the derivative of
/// that end segment's cubic. The whole system is tridiagonal regardless of
/// which boundary condition applies, so one Thomas-algorithm sweep solves it
/// for every dimension of `y` at once (the coefficient matrix does not depend
/// on the dimension, only the right-hand side does).
fn solve_moments(
    x: &[f64],
    y: &[Vec<f64>],
    lower: Boundary<'_>,
    upper: Boundary<'_>,
    dimension: usize,
) -> Vec<Vec<f64>> {
    let n = x.len() - 1; // segments; n + 1 knots, indices 0..=n
    let h: Vec<f64> = x.windows(2).map(|pair| pair[1] - pair[0]).collect();

    // Tridiagonal coefficients, one row per knot 0..=n.
    let mut sub = vec![0.0; n + 1]; // sub[i] multiplies M[i-1]
    let mut diag = vec![0.0; n + 1]; // diag[i] multiplies M[i]
    let mut sup = vec![0.0; n + 1]; // sup[i] multiplies M[i+1]
    let mut rhs = vec![vec![0.0; dimension]; n + 1];

    for i in 1..n {
        sub[i] = h[i - 1];
        diag[i] = 2.0 * (h[i - 1] + h[i]);
        sup[i] = h[i];
        for dim in 0..dimension {
            rhs[i][dim] =
                6.0 * ((y[i + 1][dim] - y[i][dim]) / h[i] - (y[i][dim] - y[i - 1][dim]) / h[i - 1]);
        }
    }

    match lower {
        Boundary::SecondDerivative(value) => {
            diag[0] = 1.0;
            rhs[0].copy_from_slice(value);
        }
        Boundary::FirstDerivative(value) => {
            diag[0] = 2.0 * h[0];
            sup[0] = h[0];
            for dim in 0..dimension {
                rhs[0][dim] = 6.0 * ((y[1][dim] - y[0][dim]) / h[0] - value[dim]);
            }
        }
    }
    match upper {
        Boundary::SecondDerivative(value) => {
            diag[n] = 1.0;
            rhs[n].copy_from_slice(value);
        }
        Boundary::FirstDerivative(value) => {
            sub[n] = h[n - 1];
            diag[n] = 2.0 * h[n - 1];
            for dim in 0..dimension {
                rhs[n][dim] = 6.0 * (value[dim] - (y[n][dim] - y[n - 1][dim]) / h[n - 1]);
            }
        }
    }

    thomas_solve(&sub, &diag, &sup, rhs)
}

/// The Thomas algorithm: a forward elimination and back-substitution sweep
/// for a tridiagonal system, generalised to a vector-valued right-hand side
/// (every dimension shares the same elimination, since it depends only on
/// `sub`/`diag`/`sup`).
///
/// `sub[0]` and `sup[len-1]` are never read (there is no sub-diagonal entry
/// on the first row or super-diagonal entry on the last). Panics only if
/// `rhs` is empty, which [`solve_moments`] never constructs -- a spline with
/// at least 2 knots always has at least 2 rows.
fn thomas_solve(sub: &[f64], diag: &[f64], sup: &[f64], mut rhs: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    let len = diag.len();

    let mut sup_prime = vec![0.0; len];
    sup_prime[0] = sup[0] / diag[0];
    for value in &mut rhs[0] {
        *value /= diag[0];
    }

    for i in 1..len {
        let denom = diag[i] - sub[i] * sup_prime[i - 1];
        sup_prime[i] = sup[i] / denom;
        let previous = rhs[i - 1].clone();
        for (value, previous_value) in rhs[i].iter_mut().zip(&previous) {
            *value = (*value - sub[i] * previous_value) / denom;
        }
    }

    for i in (0..len - 1).rev() {
        let next = rhs[i + 1].clone();
        for (value, next_value) in rhs[i].iter_mut().zip(&next) {
            *value -= sup_prime[i] * next_value;
        }
    }

    rhs
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(values: &[f64]) -> Vec<Vec<f64>> {
        values.iter().map(|&v| vec![v]).collect()
    }

    #[test]
    fn a_natural_spline_through_two_points_is_the_straight_line_between_them() {
        // With only one segment and both ends natural (curvature zero), the
        // unique cubic satisfying that is degree 1: the straight line.
        let x = [0.0, 2.0];
        let y = scalar(&[1.0, 5.0]);
        let spline = CubicSpline::new(
            &x,
            &y,
            Boundary::SecondDerivative(&[0.0]),
            Boundary::SecondDerivative(&[0.0]),
        )
        .expect("2 points is enough");

        for t in [0.0, 0.5, 1.0, 1.5, 2.0] {
            let expected = 1.0 + 2.0 * t;
            assert!((spline.evaluate(t)[0] - expected).abs() < 1e-12, "t={t}");
        }
    }

    #[test]
    fn a_clamped_spline_reproduces_the_source_polynomial_when_the_data_is_exactly_cubic() {
        // f(x) = x^3 - 2x^2 + x + 1, f'(x) = 3x^2 - 4x + 1. A cubic spline
        // clamped to the true derivative at both ends of data drawn from a
        // single cubic must reproduce that cubic exactly (it is a
        // degree-of-freedom match, not an approximation).
        let f = |x: f64| x.powi(3) - 2.0 * x.powi(2) + x + 1.0;
        let fp = |x: f64| 3.0 * x.powi(2) - 4.0 * x + 1.0;
        let x = [0.0, 0.4, 1.0, 1.7, 2.5];
        let y = scalar(&x.map(f));
        let spline = CubicSpline::new(
            &x,
            &y,
            Boundary::FirstDerivative(&[fp(x[0])]),
            Boundary::FirstDerivative(&[fp(x[x.len() - 1])]),
        )
        .expect("5 points is enough");

        for t in [0.0, 0.2, 0.4, 0.9, 1.3, 2.0, 2.5] {
            assert!(
                (spline.evaluate(t)[0] - f(t)).abs() < 1e-9,
                "t={t}: got {}, expected {}",
                spline.evaluate(t)[0],
                f(t)
            );
        }
    }

    #[test]
    fn the_spline_interpolates_every_knot_exactly() {
        let x = [0.0, 1.0, 2.5, 4.0, 4.2, 6.0];
        let y = scalar(&[0.5, 1.2, 0.9, -0.3, 0.4, 0.1]);
        let spline = CubicSpline::new(
            &x,
            &y,
            Boundary::SecondDerivative(&[0.0]),
            Boundary::FirstDerivative(&[-0.4]),
        )
        .expect("6 points is enough");

        for (i, &knot) in x.iter().enumerate() {
            assert!(
                (spline.evaluate(knot)[0] - y[i][0]).abs() < 1e-9,
                "knot {i} at x={knot}"
            );
        }
    }

    #[test]
    fn vector_valued_dimensions_interpolate_independently() {
        // A (x, y)-pair spline built from two unrelated scalar splines should
        // agree with them dimension by dimension -- the shared tridiagonal
        // solve must not let one dimension's data leak into another's.
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = vec![
            vec![0.0, 10.0],
            vec![1.0, 8.0],
            vec![0.5, 12.0],
            vec![2.0, 9.0],
        ];
        let spline = CubicSpline::new(
            &x,
            &y,
            Boundary::SecondDerivative(&[0.0, 0.0]),
            Boundary::SecondDerivative(&[0.0, 0.0]),
        )
        .expect("4 points is enough");

        let first_dim: Vec<Vec<f64>> = y.iter().map(|row| vec![row[0]]).collect();
        let second_dim: Vec<Vec<f64>> = y.iter().map(|row| vec![row[1]]).collect();
        let spline_a = CubicSpline::new(
            &x,
            &first_dim,
            Boundary::SecondDerivative(&[0.0]),
            Boundary::SecondDerivative(&[0.0]),
        )
        .expect("4 points is enough");
        let spline_b = CubicSpline::new(
            &x,
            &second_dim,
            Boundary::SecondDerivative(&[0.0]),
            Boundary::SecondDerivative(&[0.0]),
        )
        .expect("4 points is enough");

        for t in [0.0, 0.3, 1.1, 1.9, 2.5, 3.0] {
            let combined = spline.evaluate(t);
            assert!(
                (combined[0] - spline_a.evaluate(t)[0]).abs() < 1e-12,
                "t={t}"
            );
            assert!(
                (combined[1] - spline_b.evaluate(t)[0]).abs() < 1e-12,
                "t={t}"
            );
        }
    }

    #[test]
    fn fewer_than_two_points_is_an_error_not_a_panic() {
        assert_eq!(
            CubicSpline::new(
                &[1.0],
                &scalar(&[1.0]),
                Boundary::SecondDerivative(&[0.0]),
                Boundary::SecondDerivative(&[0.0]),
            )
            .unwrap_err(),
            CubicSplineError::TooFewPoints(1)
        );
    }

    #[test]
    fn a_value_vector_with_no_rows_is_an_error_not_a_panic() {
        assert_eq!(
            CubicSpline::new(
                &[0.0, 1.0],
                &[],
                Boundary::SecondDerivative(&[0.0]),
                Boundary::SecondDerivative(&[0.0]),
            )
            .unwrap_err(),
            CubicSplineError::LengthMismatch {
                expected: 2,
                actual: 0,
            }
        );
    }

    #[test]
    fn non_increasing_knots_are_an_error_not_a_panic() {
        assert_eq!(
            CubicSpline::new(
                &[0.0, 1.0, 1.0],
                &scalar(&[0.0, 1.0, 2.0]),
                Boundary::SecondDerivative(&[0.0]),
                Boundary::SecondDerivative(&[0.0]),
            )
            .unwrap_err(),
            CubicSplineError::KnotsNotIncreasing {
                index: 2,
                value: 1.0
            }
        );
    }

    #[test]
    fn a_mismatched_row_dimension_is_an_error_not_a_panic() {
        let y = vec![vec![0.0, 0.0], vec![1.0], vec![2.0, 0.0]];
        assert_eq!(
            CubicSpline::new(
                &[0.0, 1.0, 2.0],
                &y,
                Boundary::SecondDerivative(&[0.0, 0.0]),
                Boundary::SecondDerivative(&[0.0, 0.0]),
            )
            .unwrap_err(),
            CubicSplineError::DimensionMismatch {
                what: "y",
                index: 1,
                expected: 2,
                actual: 1,
            }
        );
    }
}
