// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An interpolating cubic B-spline through a strictly increasing sequence of
//! points, and the knot, basis and solve primitives the bicubic surface next
//! door is assembled from as well.
//!
//! native aerodynamic model's `Atmosphere` class has two altitude models and defaults to
//! the one that is *not* the closed-form ISA: `"differentiable"`, a cubic
//! B-spline fitted through the ISA at thirty-eight altitudes, built so that a
//! gradient-based optimizer sees a smooth function. Every module in the
//! reference implementation that writes `Atmosphere(altitude=...)`
//! without naming a method: the turbofan cycle, the performance envelope,
//! the aerodynamic analysis, stability: flies against that fit and not
//! against the ISA. The two disagree by up to 1% in temperature, so a port
//! that substituted the closed form would be wrong by four thousand times the
//! `closed` tier before any physics had happened. This module is what makes
//! reproducing the fit possible; `alas-atmo::differentiable` is the fit
//! itself.
//!
//! Upstream reaches the spline through three layers: native aerodynamic model's
//! `InterpolatedModel`, its `numpy.interpn` shim, and finally CasADi's
//! `interpolant(..., "bspline", ...)`. Nothing here is translated from any of
//! them. What CasADi builds in the one-dimensional cubic case is the
//! not-a-knot interpolating B-spline, which is a uniquely determined object
//! given the data: for `m` points the knot vector is `m + 4` long, the first
//! and last data value four times over with the data points `x[2] .. x[m-3]`
//! in between, and the coefficients are whatever reproduces the data at every
//! `x`. The two points adjacent to each end are deliberately not knots, which
//! is what makes the system square and names the end condition. This module
//! solves that system directly, and the parity fixture records CasADi's own
//! output as the check on it: the same arrangement [`crate::CubicSpline`]
//! has with SciPy.
//!
//! That knot rule is the same one Dierckx's `regrid` uses at a smoothing
//! factor of zero, which is why [`crate::BicubicSpline`] shares this module's
//! primitives rather than carrying a second copy of them. The two differ only
//! in what they do outside the data range, and that decision belongs to each
//! caller: the bicubic surface clamps to its boundary knots, while this
//! module returns NaN, because `InterpolatedModel` is constructed with
//! `fill_value=np.nan` and `interpn` overwrites every out-of-range result
//! with it. An altitude outside the fitted band produces NaN upstream, and
//! reproducing that is how a caller finds out it left the model's domain
//! instead of receiving a plausible extrapolation.

use crate::linalg::solve;

/// The spline's degree. This module is the cubic case specifically, which is
/// the only one either caller asks for; the knot rule above and the basis
/// recurrence below both branch on parity of the degree in the general
/// formulation, and reproducing branches nothing reaches would be untested
/// code.
pub(crate) const DEGREE: usize = 3;

/// Non-zero basis functions at any point: a degree-`k` spline has `k + 1`.
pub(crate) const ORDER: usize = DEGREE + 1;

/// Why a [`CubicBSpline`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CubicBSplineError {
    /// Fewer than four points were given. A cubic through fewer than four
    /// points is underdetermined, and upstream raises rather than dropping to
    /// a lower degree.
    #[error("a cubic B-spline needs at least {ORDER} points, got {0}")]
    TooFewPoints(usize),
    /// `x` was not strictly increasing.
    #[error("x[{index}] ({value}) is not strictly greater than its predecessor")]
    NotIncreasing {
        /// The offending index.
        index: usize,
        /// The offending value.
        value: f64,
    },
    /// A coordinate or ordinate was not finite.
    #[error("{what}[{index}] ({value}) is not finite")]
    NonFinite {
        /// The input vector containing the value (`x` or `y`).
        what: &'static str,
        /// The offending index.
        index: usize,
        /// The offending value.
        value: f64,
    },
    /// `y` did not have one value per point of `x`.
    #[error("x has {expected} points but y has {actual} values")]
    LengthMismatch {
        /// How many points `x` has.
        expected: usize,
        /// How many values `y` had.
        actual: usize,
    },
    /// The collocation matrix could not be factored.
    ///
    /// Not expected for strictly increasing data: the knot placement above
    /// satisfies the Schoenberg-Whitney condition by construction, which makes
    /// the matrix non-singular. Library code does not panic on a numerical
    /// surprise, so a caller sees this instead.
    #[error("the collocation matrix was numerically singular at row {row}")]
    Singular {
        /// The elimination step with no usable pivot.
        row: usize,
    },
}

/// A cubic B-spline interpolating `(x[i], y[i])`, evaluable anywhere in
/// `[x[0], x[last]]`.
#[derive(Debug, Clone)]
pub struct CubicBSpline {
    knots: Vec<f64>,
    coefficients: Vec<f64>,
    lower: f64,
    upper: f64,
}

impl CubicBSpline {
    /// Interpolate `y` over the points `x`.
    ///
    /// `x` must be strictly increasing and at least [`ORDER`] long, and `y`
    /// must be the same length.
    ///
    /// # Errors
    ///
    /// See [`CubicBSplineError`].
    pub fn interpolate(x: &[f64], y: &[f64]) -> Result<Self, CubicBSplineError> {
        if x.len() < ORDER {
            return Err(CubicBSplineError::TooFewPoints(x.len()));
        }
        if y.len() != x.len() {
            return Err(CubicBSplineError::LengthMismatch {
                expected: x.len(),
                actual: y.len(),
            });
        }
        if let Some((index, &value)) = x.iter().enumerate().find(|(_, value)| !value.is_finite()) {
            return Err(CubicBSplineError::NonFinite {
                what: "x",
                index,
                value,
            });
        }
        if let Some((index, &value)) = y.iter().enumerate().find(|(_, value)| !value.is_finite()) {
            return Err(CubicBSplineError::NonFinite {
                what: "y",
                index,
                value,
            });
        }
        for (index, pair) in x.windows(2).enumerate() {
            if pair[1] <= pair[0] {
                return Err(CubicBSplineError::NotIncreasing {
                    index: index + 1,
                    value: pair[1],
                });
            }
        }

        let knots = knot_vector(x);
        let rhs: Vec<Vec<f64>> = y.iter().map(|&value| vec![value]).collect();
        let solved = solve(&collocation(&knots, x), &rhs)
            .map_err(|row| CubicBSplineError::Singular { row })?;

        Ok(Self {
            knots,
            coefficients: solved.into_iter().map(|row| row[0]).collect(),
            lower: x[0],
            upper: x[x.len() - 1],
        })
    }

    /// The spline's value at `arg`, or NaN outside `[x[0], x[last]]`.
    ///
    /// The NaN is upstream's `fill_value`, not a failure signal invented
    /// here; see the module documentation.
    pub fn evaluate(&self, arg: f64) -> f64 {
        if !(self.lower..=self.upper).contains(&arg) {
            return f64::NAN;
        }
        let (span, weights) = span_and_basis(&self.knots, self.coefficients.len(), arg);
        let first = span - DEGREE;
        weights
            .iter()
            .enumerate()
            .map(|(i, weight)| weight * self.coefficients[first + i])
            .sum()
    }

    /// The knot vector, `x.len() + ORDER` long.
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }

    /// The B-spline coefficients, one per data point.
    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }
}

/// The not-a-knot knot vector for `values`: the end values repeated [`ORDER`]
/// times, with the interior data points `values[2] .. values[len - 3]`
/// between them.
///
/// See the module documentation for why those two points at each end are
/// skipped. When `values` has exactly [`ORDER`] entries there are no interior
/// knots at all and the whole range is a single polynomial patch.
pub(crate) fn knot_vector(values: &[f64]) -> Vec<f64> {
    let last = values.len() - 1;
    let mut knots = vec![values[0]; ORDER];
    knots.extend_from_slice(&values[2..last - 1]);
    knots.extend(std::iter::repeat_n(values[last], ORDER));
    knots
}

/// The index of the knot interval containing `arg`, and the [`ORDER`]
/// non-zero basis function values there.
///
/// `arg` is clamped into `[t[DEGREE], t[count]]` first, so a caller that has
/// already decided what to do outside the data range gets the boundary
/// polynomial rather than an index out of bounds. The returned span always
/// lies in `DEGREE ..= count - 1`, so the basis functions it names are
/// `span - DEGREE ..= span`, all of which index a real coefficient.
///
/// The span is the last knot in that range at or below `arg`, found by
/// bisection over the non-decreasing knots `t[DEGREE + 1 .. count]`. A NaN
/// argument compares false against every knot and takes the first span.
pub(crate) fn span_and_basis(t: &[f64], count: usize, arg: f64) -> (usize, [f64; ORDER]) {
    let arg = arg.clamp(t[DEGREE], t[count]);
    let span = DEGREE + t[DEGREE + 1..count].partition_point(|&knot| knot <= arg);
    (span, basis(t, span, arg))
}

/// The [`ORDER`] non-zero B-spline basis functions at `arg`, by the Cox-de
/// Boor recurrence run upwards from the constant.
///
/// The value at index `i` on return is the basis function of index
/// `span - DEGREE + i`. The recurrence is arranged one pass per degree,
/// writing both the "left" contribution into the current slot and the "right"
/// contribution into the next, which is the arrangement every reference
/// implementation of this recurrence uses and so accumulates rounding the
/// same way they do.
pub(crate) fn basis(t: &[f64], span: usize, arg: f64) -> [f64; ORDER] {
    let mut h = [0.0; ORDER];
    h[0] = 1.0;

    for degree in 1..=DEGREE {
        let previous = h;
        h[0] = 0.0;
        for i in 0..degree {
            let right = span + i + 1;
            let left = right - degree;
            let f = previous[i] / (t[right] - t[left]);
            h[i] += f * (t[right] - arg);
            h[i + 1] = f * (arg - t[left]);
        }
    }
    h
}

/// The collocation matrix `B[p][q] = B_q(values[p])`.
///
/// Square by construction: `m` points produce `m + ORDER` knots and therefore
/// `m` basis functions.
pub(crate) fn collocation(t: &[f64], values: &[f64]) -> Vec<Vec<f64>> {
    let count = values.len();
    values
        .iter()
        .map(|&value| {
            let (span, weights) = span_and_basis(t, count, value);
            let mut row = vec![0.0; count];
            row[span - DEGREE..=span].copy_from_slice(&weights);
            row
        })
        .collect()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_knot_vector_repeats_the_ends_and_skips_the_second_point_in_from_each() {
        let knots = knot_vector(&[0.0, 0.5, 1.0, 2.0, 3.5, 4.0]);
        assert_eq!(
            knots,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 4.0, 4.0, 4.0]
        );
    }

    #[test]
    fn the_bisected_span_is_the_one_a_linear_scan_finds() {
        fn linear_span(t: &[f64], count: usize, arg: f64) -> usize {
            let arg = arg.clamp(t[DEGREE], t[count]);
            let mut span = DEGREE;
            while span < count - 1 && arg >= t[span + 1] {
                span += 1;
            }
            span
        }
        for values in [
            vec![1.0, 2.0, 3.0, 4.0],
            vec![0.0, 0.5, 1.0, 2.0, 3.5, 4.0],
            (0..38).map(|i| f64::from(i).powf(1.7)).collect(),
        ] {
            let knots = knot_vector(&values);
            let count = values.len();
            let low = values[0] - 1.0;
            let high = values[count - 1] + 1.0;
            let mut probes: Vec<f64> = (0..=400)
                .map(|step| low + (high - low) * f64::from(step) / 400.0)
                .collect();
            probes.extend_from_slice(&knots);
            for arg in probes {
                assert_eq!(
                    span_and_basis(&knots, count, arg).0,
                    linear_span(&knots, count, arg),
                    "arg {arg}"
                );
            }
            assert_eq!(span_and_basis(&knots, count, f64::NAN).0, DEGREE);
        }
    }

    #[test]
    fn the_smallest_dataset_has_no_interior_knots_at_all() {
        let knots = knot_vector(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(knots, vec![1.0, 1.0, 1.0, 1.0, 4.0, 4.0, 4.0, 4.0]);
    }

    #[test]
    fn the_spline_reproduces_every_data_point() {
        let x: [f64; 7] = [0.0, 0.7, 1.5, 2.0, 3.0, 4.2, 5.5];
        let y: Vec<f64> = x.iter().map(|v| (v * 0.6).sin() + 0.1 * v * v).collect();
        let spline = CubicBSpline::interpolate(&x, &y).expect("well-formed data");

        for (&xi, &yi) in x.iter().zip(&y) {
            assert!((spline.evaluate(xi) - yi).abs() < 1e-12, "at {xi}");
        }
    }

    #[test]
    fn a_cubic_polynomial_is_reproduced_everywhere_not_only_at_the_nodes() {
        // Data drawn from a single cubic lies in the spline space exactly, so
        // interpolation must return it at every point. A wrong knot vector
        // still matches at the nodes and fails here.
        let f = |v: f64| 1.0 + 2.0 * v - 0.5 * v * v + 0.3 * v * v * v;
        let x = [0.0, 0.4, 1.1, 1.9, 2.6, 3.3, 4.0];
        let y: Vec<f64> = x.iter().map(|&v| f(v)).collect();
        let spline = CubicBSpline::interpolate(&x, &y).expect("well-formed data");

        for step in 0..=40 {
            let arg = 4.0 * f64::from(step) / 40.0;
            let error = (spline.evaluate(arg) - f(arg)).abs();
            assert!(error < 1e-11, "at {arg}: off by {error}");
        }
    }

    #[test]
    fn the_end_conditions_leave_no_curvature_break_at_the_second_point_in() {
        // "Not a knot" means the second and second-to-last data points are
        // not knots, so the third derivative is continuous across them,
        // which is the whole content of the end condition and the one thing a
        // natural or clamped spline through the same data would not satisfy.
        let x: [f64; 7] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y: Vec<f64> = x.iter().map(|v| v.exp() * 0.1).collect();
        let spline = CubicBSpline::interpolate(&x, &y).expect("well-formed data");

        let third_derivative = |at: f64| {
            let h = 1e-2;
            (spline.evaluate(at + 2.0 * h) - 2.0 * spline.evaluate(at + h)
                + 2.0 * spline.evaluate(at - h)
                - spline.evaluate(at - 2.0 * h))
                / (2.0 * h * h * h)
        };
        // Across x[1], the first interior data point that is not a knot.
        let left = third_derivative(0.5);
        let right = third_derivative(1.5);
        assert!(
            (left - right).abs() < 1e-6 * left.abs().max(1.0),
            "third derivative jumped from {left} to {right} across x[1]"
        );
    }

    #[test]
    fn the_basis_functions_at_any_point_sum_to_one() {
        // Partition of unity: the defining property of a B-spline basis, and
        // the cheapest check that the recurrence's denominators are paired
        // with the right knots.
        let x = [0.0, 1.0, 2.0, 3.0, 4.5, 6.0, 7.0];
        let t = knot_vector(&x);
        for step in 0..=70 {
            let arg = f64::from(step) * 0.1;
            let (_, weights) = span_and_basis(&t, x.len(), arg);
            let total: f64 = weights.iter().sum();
            assert!((total - 1.0).abs() < 1e-14, "at {arg}: sum was {total}");
        }
    }

    #[test]
    fn a_point_outside_the_data_range_is_nan_rather_than_an_extrapolation() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [0.0, 1.0, 4.0, 9.0, 16.0];
        let spline = CubicBSpline::interpolate(&x, &y).expect("well-formed data");

        assert!(spline.evaluate(-1e-9).is_nan());
        assert!(spline.evaluate(4.0 + 1e-9).is_nan());
        // The endpoints themselves are inside the domain.
        assert!((spline.evaluate(0.0) - 0.0).abs() < 1e-12);
        assert!((spline.evaluate(4.0) - 16.0).abs() < 1e-12);
    }

    #[test]
    fn fewer_than_four_points_is_an_error_not_a_panic() {
        assert_eq!(
            CubicBSpline::interpolate(&[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0]).unwrap_err(),
            CubicBSplineError::TooFewPoints(3)
        );
    }

    #[test]
    fn a_non_increasing_x_is_an_error_not_a_panic() {
        assert_eq!(
            CubicBSpline::interpolate(&[0.0, 1.0, 1.0, 2.0], &[0.0; 4]).unwrap_err(),
            CubicBSplineError::NotIncreasing {
                index: 2,
                value: 1.0
            }
        );
    }

    #[test]
    fn a_y_of_the_wrong_length_is_an_error_not_a_panic() {
        assert_eq!(
            CubicBSpline::interpolate(&[0.0, 1.0, 2.0, 3.0], &[0.0; 3]).unwrap_err(),
            CubicBSplineError::LengthMismatch {
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn a_non_finite_coordinate_or_value_is_an_error_not_a_nan_spline() {
        assert!(matches!(
            CubicBSpline::interpolate(&[0.0, 1.0, 2.0, f64::NAN], &[0.0; 4]),
            Err(CubicBSplineError::NonFinite { what: "x", .. })
        ));
        assert!(matches!(
            CubicBSpline::interpolate(&[0.0, 1.0, 2.0, 3.0], &[0.0, f64::INFINITY, 0.0, 0.0]),
            Err(CubicBSplineError::NonFinite { what: "y", .. })
        ));
    }
}
