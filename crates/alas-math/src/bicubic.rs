// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An interpolating bicubic spline over a rectangular grid, laid out the way
//! FITPACK lays one out.
//!
//! mission analysis model builds its lift and induced-drag surrogates by handing a table of
//! vortex-lattice results to `scipy.interpolate.RectBivariateSpline` with its
//! defaults, and the mission then flies against the surrogate rather than
//! against the table. Every mission number therefore depends on this surface,
//! which makes "a bicubic interpolant through the same points" too weak a
//! specification to port against: the interpolation problem is only
//! well-posed once the knots are fixed, and infinitely many bicubic surfaces
//! pass through the same grid.
//!
//! The knots come from Dierckx's `regrid` with a smoothing factor of zero
//! (P. Dierckx, *Curve and Surface Fitting with Splines*, Oxford University
//! Press, 1993, chapter 4). For a cubic and `m` data points along an axis,
//! the vector is `m + 4` long: the first and last data value four times over,
//! and the data points `x[2] .. x[m - 3]` in between. The two data points
//! adjacent to each end are deliberately not knots, which is what makes the
//! system square and gives the resulting curve its not-a-knot end behaviour.
//! Nothing about that placement follows from the words "cubic interpolation",
//! and getting it wrong produces a smooth, plausible, wrong surface -- so the
//! parity fixture records the knot vectors themselves and not only the values.
//!
//! Outside the data rectangle the argument is clamped to the boundary knots,
//! so the surface is constant beyond every edge rather than continuing the
//! edge polynomial. This matters as soon as the mission asks for a Mach
//! number the training grid does not reach: the upstream answer is the edge
//! value, not a cubic extrapolation of it, and the difference between those
//! two grows without bound. That clamp is the one thing this module does not
//! share with [`crate::CubicBSpline`], whose upstream fills NaN outside its
//! range instead; the knot rule, the basis recurrence and the solve are the
//! same construction and live there.

use crate::bspline::{collocation, knot_vector, span_and_basis, DEGREE, ORDER};
use crate::linalg::solve;

/// Which axis a construction error refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The first (row) axis of the grid.
    X,
    /// The second (column) axis of the grid.
    Y,
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::X => f.write_str("x"),
            Self::Y => f.write_str("y"),
        }
    }
}

/// Why a [`BicubicSpline`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum BicubicSplineError {
    /// An axis had fewer than four points. A cubic through fewer than four
    /// points is underdetermined, and upstream raises rather than dropping to
    /// a lower degree.
    #[error("the {axis} axis has {count} points; a bicubic needs at least {ORDER}")]
    TooFewPoints {
        /// The offending axis.
        axis: Axis,
        /// How many points it had.
        count: usize,
    },
    /// An axis was not strictly increasing.
    #[error("{axis}[{index}] ({value}) is not strictly greater than its predecessor")]
    NotIncreasing {
        /// The offending axis.
        axis: Axis,
        /// The offending index.
        index: usize,
        /// The offending value.
        value: f64,
    },
    /// An axis coordinate was not finite.
    #[error("{axis}[{index}] ({value}) is not finite")]
    NonFiniteAxis {
        /// The offending axis.
        axis: Axis,
        /// The offending coordinate's index.
        index: usize,
        /// The offending value.
        value: f64,
    },
    /// `z` was not `x.len()` rows of `y.len()` values.
    #[error("z has {actual} values in row {row}, but the {axis} axis has {expected} points")]
    ShapeMismatch {
        /// Which axis the row should have matched.
        axis: Axis,
        /// The offending row (0 when `z` itself has the wrong number of rows).
        row: usize,
        /// How long that axis is.
        expected: usize,
        /// How long the row was.
        actual: usize,
    },
    /// A grid ordinate was not finite.
    #[error("z[{row}][{column}] ({value}) is not finite")]
    NonFiniteValue {
        /// The offending grid row.
        row: usize,
        /// The offending grid column.
        column: usize,
        /// The offending value.
        value: f64,
    },
    /// The collocation matrix could not be factored.
    ///
    /// Not expected for strictly increasing data: the knot placement above
    /// satisfies the Schoenberg-Whitney condition by construction, which makes
    /// the matrix non-singular. Library code does not panic on a numerical
    /// surprise, so a caller sees this instead.
    #[error("the {axis} collocation matrix was numerically singular at row {row}")]
    Singular {
        /// Which direction's solve failed.
        axis: Axis,
        /// The elimination step with no usable pivot.
        row: usize,
    },
}

/// A bicubic spline interpolating a rectangular grid of values.
#[derive(Debug, Clone)]
pub struct BicubicSpline {
    tx: Vec<f64>,
    ty: Vec<f64>,
    /// Tensor-product coefficients, row-major: `nx` rows of `ny`, where
    /// `nx = tx.len() - ORDER` and `ny = ty.len() - ORDER`.
    coefficients: Vec<f64>,
    nx: usize,
    ny: usize,
}

impl BicubicSpline {
    /// Interpolate `z` over the grid `x` by `y`, where `z[i][j]` is the value
    /// at `(x[i], y[j])`.
    ///
    /// Both axes must be strictly increasing and at least [`ORDER`] long.
    ///
    /// # Errors
    ///
    /// See [`BicubicSplineError`].
    pub fn interpolate(x: &[f64], y: &[f64], z: &[Vec<f64>]) -> Result<Self, BicubicSplineError> {
        check_axis(Axis::X, x)?;
        check_axis(Axis::Y, y)?;
        if z.len() != x.len() {
            return Err(BicubicSplineError::ShapeMismatch {
                axis: Axis::X,
                row: 0,
                expected: x.len(),
                actual: z.len(),
            });
        }
        for (row, values) in z.iter().enumerate() {
            if values.len() != y.len() {
                return Err(BicubicSplineError::ShapeMismatch {
                    axis: Axis::Y,
                    row,
                    expected: y.len(),
                    actual: values.len(),
                });
            }
            if let Some((column, &value)) = values
                .iter()
                .enumerate()
                .find(|(_, value)| !value.is_finite())
            {
                return Err(BicubicSplineError::NonFiniteValue { row, column, value });
            }
        }

        let tx = knot_vector(x);
        let ty = knot_vector(y);

        // The tensor-product interpolation conditions are `Bx C By' = Z`, so
        // the two directions separate: eliminate x first, then y on the
        // transpose. Solving one m-by-m system per direction rather than one
        // (mx*my)-by-(mx*my) system is what makes this cheap, and it is
        // exactly the reduction the upstream routine performs.
        let along_x = solve(&collocation(&tx, x), z)
            .map_err(|row| BicubicSplineError::Singular { axis: Axis::X, row })?;
        let along_y = solve(&collocation(&ty, y), &transpose(&along_x))
            .map_err(|row| BicubicSplineError::Singular { axis: Axis::Y, row })?;
        let coefficients = transpose(&along_y);

        let nx = x.len();
        let ny = y.len();
        Ok(Self {
            tx,
            ty,
            coefficients: coefficients.into_iter().flatten().collect(),
            nx,
            ny,
        })
    }

    /// The surface's value at `(x, y)`.
    ///
    /// Outside the data rectangle the argument is clamped to the boundary
    /// knots, so the surface is constant beyond every edge.
    pub fn evaluate(&self, x: f64, y: f64) -> f64 {
        // `span_and_basis` clamps, which is exactly the constant-beyond-the-
        // edge behaviour this surface wants; see the module documentation.
        let (span_x, basis_x) = span_and_basis(&self.tx, self.nx, x);
        let (span_y, basis_y) = span_and_basis(&self.ty, self.ny, y);

        let first_x = span_x - DEGREE;
        let first_y = span_y - DEGREE;
        let mut value = 0.0;
        for (i, weight_x) in basis_x.iter().enumerate() {
            let row = (first_x + i) * self.ny + first_y;
            for (j, weight_y) in basis_y.iter().enumerate() {
                value += self.coefficients[row + j] * weight_x * weight_y;
            }
        }
        value
    }

    /// The knot vectors in each direction, `x` first.
    pub fn knots(&self) -> (&[f64], &[f64]) {
        (&self.tx, &self.ty)
    }

    /// The tensor-product coefficients, row-major over the `x` direction.
    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }
}

fn check_axis(axis: Axis, values: &[f64]) -> Result<(), BicubicSplineError> {
    if values.len() < ORDER {
        return Err(BicubicSplineError::TooFewPoints {
            axis,
            count: values.len(),
        });
    }
    if let Some((index, &value)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(BicubicSplineError::NonFiniteAxis { axis, index, value });
    }
    for (index, pair) in values.windows(2).enumerate() {
        if pair[1] <= pair[0] {
            return Err(BicubicSplineError::NotIncreasing {
                axis,
                index: index + 1,
                value: pair[1],
            });
        }
    }
    Ok(())
}

fn transpose(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let columns = matrix.first().map_or(0, Vec::len);
    (0..columns)
        .map(|column| matrix.iter().map(|row| row[column]).collect())
        .collect()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn grid(x: &[f64], y: &[f64], f: impl Fn(f64, f64) -> f64) -> Vec<Vec<f64>> {
        x.iter()
            .map(|&xi| y.iter().map(|&yj| f(xi, yj)).collect())
            .collect()
    }

    #[test]
    fn the_surface_reproduces_every_grid_value() {
        let x = [0.0, 0.7, 1.5, 2.0, 3.0, 4.2];
        let y = [-1.0, 0.0, 0.5, 2.0, 3.0];
        let z = grid(&x, &y, |a, b| (a * 0.6).sin() + b * b * 0.25);
        let spline = BicubicSpline::interpolate(&x, &y, &z).expect("a well-formed grid");

        for (i, &xi) in x.iter().enumerate() {
            for (j, &yj) in y.iter().enumerate() {
                assert!(
                    (spline.evaluate(xi, yj) - z[i][j]).abs() < 1e-10,
                    "at ({xi}, {yj})"
                );
            }
        }
    }

    #[test]
    fn a_bicubic_polynomial_is_reproduced_everywhere_not_only_at_the_nodes() {
        // Data drawn from a single tensor-product bicubic lies in the spline
        // space exactly, so interpolation must return it at every point.
        // A wrong knot vector still matches at the nodes and fails here.
        let f = |a: f64, b: f64| {
            (1.0 + 2.0 * a - 0.5 * a * a + 0.3 * a * a * a)
                * (2.0 - b + 0.25 * b * b - 0.1 * b * b * b)
        };
        let x = [0.0, 0.4, 1.1, 1.9, 2.6, 3.3, 4.0];
        let y = [-2.0, -0.7, 0.3, 1.2, 2.5, 3.1];
        let z = grid(&x, &y, f);
        let spline = BicubicSpline::interpolate(&x, &y, &z).expect("a well-formed grid");

        for step_x in 0..17 {
            for step_y in 0..13 {
                let a = 4.0 * step_x as f64 / 16.0;
                let b = -2.0 + 5.1 * step_y as f64 / 12.0;
                let error = (spline.evaluate(a, b) - f(a, b)).abs();
                assert!(error < 1e-9, "at ({a}, {b}): off by {error}");
            }
        }
    }

    #[test]
    fn the_surface_is_constant_outside_the_data_rectangle() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [0.0, 1.0, 2.0, 3.0];
        let z = grid(&x, &y, |a, b| a * a + b);
        let spline = BicubicSpline::interpolate(&x, &y, &z).expect("a well-formed grid");

        let corner = spline.evaluate(4.0, 3.0);
        assert!((spline.evaluate(100.0, 3.0) - corner).abs() < 1e-12);
        assert!((spline.evaluate(4.0, 50.0) - corner).abs() < 1e-12);
        assert!((spline.evaluate(9e9, 9e9) - corner).abs() < 1e-12);

        let other_corner = spline.evaluate(0.0, 0.0);
        assert!((spline.evaluate(-7.0, -7.0) - other_corner).abs() < 1e-12);
    }

    #[test]
    fn an_axis_shorter_than_four_points_is_an_error_not_a_panic() {
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 1.0, 2.0, 3.0];
        let z = grid(&x, &y, |a, b| a + b);
        assert_eq!(
            BicubicSpline::interpolate(&x, &y, &z).unwrap_err(),
            BicubicSplineError::TooFewPoints {
                axis: Axis::X,
                count: 3
            }
        );
    }

    #[test]
    fn a_non_increasing_axis_is_an_error_not_a_panic() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 1.0, 3.0];
        let z = grid(&x, &y, |a, b| a + b);
        assert_eq!(
            BicubicSpline::interpolate(&x, &y, &z).unwrap_err(),
            BicubicSplineError::NotIncreasing {
                axis: Axis::Y,
                index: 2,
                value: 1.0
            }
        );
    }

    #[test]
    fn a_grid_that_does_not_match_its_axes_is_an_error_not_a_panic() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 2.0, 3.0];
        let mut z = grid(&x, &y, |a, b| a + b);
        z[2].pop();
        assert_eq!(
            BicubicSpline::interpolate(&x, &y, &z).unwrap_err(),
            BicubicSplineError::ShapeMismatch {
                axis: Axis::Y,
                row: 2,
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn non_finite_axes_and_grid_values_are_rejected_before_solving() {
        let x = [0.0, 1.0, 2.0, f64::NAN];
        let y = [0.0, 1.0, 2.0, 3.0];
        let z = grid(&x, &y, |a, b| a + b);
        assert!(matches!(
            BicubicSpline::interpolate(&x, &y, &z),
            Err(BicubicSplineError::NonFiniteAxis { axis: Axis::X, .. })
        ));

        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 2.0, 3.0];
        let mut z = grid(&x, &y, |a, b| a + b);
        z[2][1] = f64::NEG_INFINITY;
        assert!(matches!(
            BicubicSpline::interpolate(&x, &y, &z),
            Err(BicubicSplineError::NonFiniteValue {
                row: 2,
                column: 1,
                ..
            })
        ));
    }
}
