// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clamped B-spline basis and curves of any degree up to [`MAX_DEGREE`], with
//! derivatives up to [`MAX_DERIVATIVE`] (de Boor / Cox recursion, Piegl and
//! Tiller, "The NURBS Book", 2nd ed., algorithms A2.1 and A2.3).
//!
//! This is the evaluator behind the lofted-geometry curves. It is unrelated
//! to [`crate::CubicBSpline`], which is an interpolating not-a-knot spline
//! built from data points and has no derivatives: here the caller supplies the
//! knot vector and the control coefficients directly.
//!
//! Conventions: the knot vector is non-decreasing and clamped, that is the
//! first and last `degree + 1` knots are equal, so the curve interpolates its
//! first and last coefficient and the domain is `[knots[degree], knots[n]]`
//! with `n = coeffs.len()`. The curve is right-continuous at an interior knot
//! of lower continuity. Evaluation clamps the argument into the domain rather
//! than extrapolating: a caller that needs to know about an out-of-range
//! argument checks [`ClampedBSpline::domain`] first.

/// Highest supported degree.
pub const MAX_DEGREE: usize = 7;
/// Highest derivative order returned by the evaluation functions.
pub const MAX_DERIVATIVE: usize = 3;

/// Basis-function derivative table: `table[k][j]` is the `k`-th derivative of
/// the `j`-th non-zero basis function (the one with coefficient index
/// `span - degree + j`) at the evaluation point.
pub type BasisTable = [[f64; MAX_DEGREE + 1]; MAX_DERIVATIVE + 1];

/// Why a clamped B-spline could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClampedBSplineError {
    /// Degree outside `1..=MAX_DEGREE`.
    #[error("degree {0} outside 1..={MAX_DEGREE}")]
    Degree(usize),
    /// `knots.len() != coeffs.len() + degree + 1`, or too few coefficients.
    #[error("{knots} knots and {coeffs} coefficients do not fit degree {degree}")]
    Counts {
        /// Knots supplied.
        knots: usize,
        /// Coefficients supplied.
        coeffs: usize,
        /// Degree requested.
        degree: usize,
    },
    /// A knot or coefficient is NaN or infinite.
    #[error("non-finite knot or coefficient at index {0}")]
    NonFinite(usize),
    /// Knots decrease at the given index.
    #[error("knots decrease at index {0}")]
    NotMonotone(usize),
    /// The first or last `degree + 1` knots are not all equal.
    #[error("knot vector is not clamped")]
    NotClamped,
    /// The domain has zero length.
    #[error("empty parameter domain")]
    EmptyDomain,
    /// An interior knot is repeated more than `degree` times, or an end knot
    /// more than `degree + 1` times.
    #[error("knot at index {0} has too high a multiplicity")]
    Multiplicity(usize),
}

/// Index of the knot span containing `u`: the `i` with
/// `knots[i] <= u < knots[i + 1]`, and the last non-empty span at the upper
/// end of the domain (A2.1). `n_coeffs` is the number of coefficients.
#[must_use]
pub fn find_span(knots: &[f64], degree: usize, n_coeffs: usize, u: f64) -> usize {
    let n = n_coeffs - 1;
    if u >= knots[n + 1] {
        return n;
    }
    if u <= knots[degree] {
        return degree;
    }
    let (mut low, mut high) = (degree, n + 1);
    let mut mid = (low + high) / 2;
    while u < knots[mid] || u >= knots[mid + 1] {
        if u < knots[mid] {
            high = mid;
        } else {
            low = mid;
        }
        mid = (low + high) / 2;
    }
    mid
}

/// Non-zero basis functions of `degree` on `span` and their derivatives up to
/// order [`MAX_DERIVATIVE`] at `u` (A2.3). Orders above `degree` are zero.
#[must_use]
pub fn basis_derivatives(knots: &[f64], degree: usize, span: usize, u: f64) -> BasisTable {
    let p = degree;
    let mut ndu = [[0.0_f64; MAX_DEGREE + 1]; MAX_DEGREE + 1];
    let mut left = [0.0_f64; MAX_DEGREE + 1];
    let mut right = [0.0_f64; MAX_DEGREE + 1];
    ndu[0][0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            ndu[j][r] = right[r + 1] + left[j - r];
            let temp = ndu[r][j - 1] / ndu[j][r];
            ndu[r][j] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        ndu[j][j] = saved;
    }

    let mut ders: BasisTable = [[0.0; MAX_DEGREE + 1]; MAX_DERIVATIVE + 1];
    for j in 0..=p {
        ders[0][j] = ndu[j][p];
    }
    let top = MAX_DERIVATIVE.min(p);
    let mut a = [[0.0_f64; MAX_DEGREE + 1]; 2];
    for r in 0..=p {
        let (mut s1, mut s2) = (0usize, 1usize);
        a[0][0] = 1.0;
        for k in 1..=top {
            let mut d = 0.0;
            // Signed indices of the published algorithm; every index used to
            // read `ndu` or `a` below is non-negative.
            let rk = r as isize - k as isize;
            let pk = (p - k) as isize;
            if rk >= 0 {
                a[s2][0] = a[s1][0] / ndu[(pk + 1) as usize][rk as usize];
                d = a[s2][0] * ndu[rk as usize][pk as usize];
            }
            let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
            let j2 = if (r as isize) - 1 <= pk { k - 1 } else { p - r };
            for j in j1..=j2 {
                let idx = (rk + j as isize) as usize;
                a[s2][j] = (a[s1][j] - a[s1][j - 1]) / ndu[(pk + 1) as usize][idx];
                d += a[s2][j] * ndu[idx][pk as usize];
            }
            if (r as isize) <= pk {
                a[s2][k] = -a[s1][k - 1] / ndu[(pk + 1) as usize][r];
                d += a[s2][k] * ndu[r][pk as usize];
            }
            ders[k][r] = d;
            std::mem::swap(&mut s1, &mut s2);
        }
    }
    let mut factor = p as f64;
    for (k, row) in ders.iter_mut().enumerate().take(top + 1).skip(1) {
        for v in row.iter_mut().take(p + 1) {
            *v *= factor;
        }
        factor *= (p - k) as f64;
    }
    ders
}

/// A clamped B-spline curve `sum_i coeffs[i] N_i(u)` of one scalar variable.
#[derive(Debug, Clone, PartialEq)]
pub struct ClampedBSpline {
    knots: Vec<f64>,
    coeffs: Vec<f64>,
    degree: usize,
}

impl ClampedBSpline {
    /// Validate and build the spline.
    ///
    /// # Errors
    ///
    /// [`ClampedBSplineError`] when the degree, counts, monotonicity,
    /// clamping, domain or interior multiplicities are invalid.
    pub fn new(
        knots: Vec<f64>,
        coeffs: Vec<f64>,
        degree: usize,
    ) -> Result<Self, ClampedBSplineError> {
        if degree == 0 || degree > MAX_DEGREE {
            return Err(ClampedBSplineError::Degree(degree));
        }
        if coeffs.len() <= degree || knots.len() != coeffs.len() + degree + 1 {
            return Err(ClampedBSplineError::Counts {
                knots: knots.len(),
                coeffs: coeffs.len(),
                degree,
            });
        }
        if let Some(i) = knots.iter().position(|k| !k.is_finite()) {
            return Err(ClampedBSplineError::NonFinite(i));
        }
        if let Some(i) = coeffs.iter().position(|c| !c.is_finite()) {
            return Err(ClampedBSplineError::NonFinite(i));
        }
        if let Some(i) = (1..knots.len()).find(|&i| knots[i] < knots[i - 1]) {
            return Err(ClampedBSplineError::NotMonotone(i));
        }
        let n = coeffs.len();
        let first_ok = knots[..=degree].iter().all(|&k| k == knots[0]);
        let last_ok = knots[n..].iter().all(|&k| k == knots[n]);
        if !first_ok || !last_ok {
            return Err(ClampedBSplineError::NotClamped);
        }
        if knots[n] <= knots[degree] {
            return Err(ClampedBSplineError::EmptyDomain);
        }
        // End multiplicity above degree + 1 leaves an empty first or last span
        // and the basis evaluates to NaN there.
        if knots[degree + 1] == knots[degree] {
            return Err(ClampedBSplineError::Multiplicity(degree + 1));
        }
        if knots[n - 1] == knots[n] {
            return Err(ClampedBSplineError::Multiplicity(n - 1));
        }
        let mut run = 1usize;
        for i in (degree + 1)..n {
            run = if knots[i] == knots[i - 1] { run + 1 } else { 1 };
            if run > degree {
                return Err(ClampedBSplineError::Multiplicity(i));
            }
        }
        Ok(Self {
            knots,
            coeffs,
            degree,
        })
    }

    /// Polynomial degree.
    #[must_use]
    pub fn degree(&self) -> usize {
        self.degree
    }

    /// The full knot vector.
    #[must_use]
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }

    /// The control coefficients.
    #[must_use]
    pub fn coeffs(&self) -> &[f64] {
        &self.coeffs
    }

    /// Number of coefficients.
    #[must_use]
    pub fn num_coeffs(&self) -> usize {
        self.coeffs.len()
    }

    /// The parameter domain `(first, last)`.
    #[must_use]
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.coeffs.len()])
    }

    /// Distinct knot values over the domain, ascending: the span boundaries.
    #[must_use]
    pub fn breakpoints(&self) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        for &k in &self.knots[self.degree..=self.coeffs.len()] {
            if out.last().is_none_or(|&last| k > last) {
                out.push(k);
            }
        }
        out
    }

    /// Value and derivatives `0..=3` at `u` (clamped into the domain).
    #[must_use]
    pub fn derivatives(&self, u: f64) -> [f64; MAX_DERIVATIVE + 1] {
        let (lo, hi) = self.domain();
        let u = u.clamp(lo, hi);
        let span = find_span(&self.knots, self.degree, self.coeffs.len(), u);
        self.derivatives_in_span(span, u)
    }

    /// Value and derivatives `0..=3` of the polynomial piece of `span`
    /// evaluated at `u`, which may be the span's end: the one-sided limit at a
    /// knot. `span` must index a non-empty span (`knots[span] < knots[span+1]`).
    #[must_use]
    pub fn derivatives_in_span(&self, span: usize, u: f64) -> [f64; MAX_DERIVATIVE + 1] {
        let p = self.degree;
        let table = basis_derivatives(&self.knots, p, span, u);
        let mut out = [0.0; MAX_DERIVATIVE + 1];
        for (k, slot) in out.iter_mut().enumerate() {
            let mut sum = 0.0;
            let window = &self.coeffs[span - p..=span];
            for (b, c) in table[k].iter().zip(window) {
                sum += b * c;
            }
            *slot = sum;
        }
        out
    }

    /// Index of the non-empty span containing `u` (clamped into the domain).
    #[must_use]
    pub fn span_of(&self, u: f64) -> usize {
        let (lo, hi) = self.domain();
        find_span(&self.knots, self.degree, self.coeffs.len(), u.clamp(lo, hi))
    }

    /// The curve value at `u`.
    #[must_use]
    pub fn eval(&self, u: f64) -> f64 {
        self.derivatives(u)[0]
    }

    /// The `order`-th derivative at `u`; zero above `degree`, and `None`
    /// above [`MAX_DERIVATIVE`].
    #[must_use]
    pub fn derivative(&self, u: f64, order: usize) -> Option<f64> {
        self.derivatives(u).get(order).copied()
    }

    /// Maximal parameter intervals on which the curve is constant to within
    /// `tol`, found from the coefficients that act on each knot span (all
    /// equal means the span's polynomial is constant). Adjacent spans merge.
    #[must_use]
    pub fn constant_intervals(&self, tol: f64) -> Vec<(f64, f64)> {
        let p = self.degree;
        let mut out: Vec<(f64, f64)> = Vec::new();
        for i in p..self.coeffs.len() {
            if self.knots[i + 1] <= self.knots[i] {
                continue;
            }
            let window = &self.coeffs[i - p..=i];
            let lo = window.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = window.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if hi - lo > tol {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.1 == self.knots[i] => last.1 = self.knots[i + 1],
                _ => out.push((self.knots[i], self.knots[i + 1])),
            }
        }
        out
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn clamped_uniform(n: usize, p: usize) -> Vec<f64> {
        let interior = n - p - 1;
        let mut k = vec![0.0; p + 1];
        for i in 1..=interior {
            k.push(i as f64 / (interior + 1) as f64);
        }
        k.extend(std::iter::repeat_n(1.0, p + 1));
        k
    }

    /// Coefficients of the spline that interpolates `f` at the Greville
    /// abscissae (Schoenberg-Whitney holds there), by a square collocation
    /// solve: a polynomial of degree <= p is reproduced exactly.
    fn collocate(knots: &[f64], p: usize, n: usize, f: impl Fn(f64) -> f64) -> Vec<f64> {
        let sites: Vec<f64> = (0..n)
            .map(|i| knots[i + 1..=i + p].iter().sum::<f64>() / p as f64)
            .collect();
        let mut a = vec![vec![0.0; n]; n];
        for (row, &u) in a.iter_mut().zip(&sites) {
            let span = find_span(knots, p, n, u);
            let t = basis_derivatives(knots, p, span, u);
            for j in 0..=p {
                row[span - p + j] = t[0][j];
            }
        }
        let b: Vec<Vec<f64>> = sites.iter().map(|&u| vec![f(u)]).collect();
        crate::linalg::solve(&a, &b)
            .unwrap()
            .into_iter()
            .map(|r| r[0])
            .collect()
    }

    #[test]
    fn bspline_partition_of_unity_and_derivative_sum_zero() {
        for p in [1usize, 2, 3, 5, 7] {
            let n = p + 6;
            let knots = clamped_uniform(n, p);
            for step in 0..=200 {
                let u = f64::from(step) / 200.0;
                let span = find_span(&knots, p, n, u);
                let t = basis_derivatives(&knots, p, span, u);
                let sum0: f64 = t[0][..=p].iter().sum();
                assert!((sum0 - 1.0).abs() < 1e-13, "p={p} u={u} sum={sum0}");
                for row in &t[1..] {
                    let s: f64 = row[..=p].iter().sum();
                    assert!(s.abs() < 1e-9, "p={p} u={u} derivative sum {s}");
                }
            }
        }
    }

    #[test]
    fn bspline_reproduces_a_polynomial_and_its_derivatives() {
        // f(u) = 1 - 2u + 3u^2 - u^3 + 0.5u^4 - 0.25u^5, exact for degree 5.
        let f =
            |u: f64| 1.0 - 2.0 * u + 3.0 * u * u - u.powi(3) + 0.5 * u.powi(4) - 0.25 * u.powi(5);
        let d1 = |u: f64| -2.0 + 6.0 * u - 3.0 * u * u + 2.0 * u.powi(3) - 1.25 * u.powi(4);
        let d2 = |u: f64| 6.0 - 6.0 * u + 6.0 * u * u - 5.0 * u.powi(3);
        let d3 = |u: f64| -6.0 + 12.0 * u - 15.0 * u * u;
        let (p, n) = (5usize, 11usize);
        let knots = clamped_uniform(n, p);
        let coeffs = collocate(&knots, p, n, f);
        let s = ClampedBSpline::new(knots, coeffs, p).unwrap();
        let mut worst = 0.0_f64;
        for step in 0..=400 {
            let u = f64::from(step) / 400.0;
            let d = s.derivatives(u);
            worst = worst
                .max((d[0] - f(u)).abs())
                .max((d[1] - d1(u)).abs())
                .max((d[2] - d2(u)).abs())
                .max((d[3] - d3(u)).abs());
        }
        assert!(worst < 1e-10, "worst polynomial reproduction error {worst}");
    }

    #[test]
    fn bspline_derivatives_match_central_finite_differences() {
        let (p, n) = (5usize, 12usize);
        let knots = clamped_uniform(n, p);
        let coeffs: Vec<f64> = (0..n)
            .map(|i| ((i as f64) * 0.7).sin() + 0.1 * i as f64)
            .collect();
        let s = ClampedBSpline::new(knots, coeffs, p).unwrap();
        let h = 1e-5;
        for step in 1..40 {
            let u = f64::from(step) / 40.0 + 0.0123;
            let d = s.derivatives(u);
            let dp = s.derivatives(u + h);
            let dm = s.derivatives(u - h);
            for k in 0..3 {
                let fd = (dp[k] - dm[k]) / (2.0 * h);
                assert!(
                    (fd - d[k + 1]).abs() < 1e-5 * (1.0 + d[k + 1].abs()),
                    "order {} at {u}: fd {fd} analytic {}",
                    k + 1,
                    d[k + 1]
                );
            }
        }
    }

    #[test]
    fn bspline_cubic_matches_hand_evaluated_bezier() {
        // One cubic span: a Bezier curve with control points 0, 1, 3, 2.
        let s = ClampedBSpline::new(
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            vec![0.0, 1.0, 3.0, 2.0],
            3,
        )
        .unwrap();
        let u = 0.3_f64;
        let b = [
            (1.0 - u).powi(3),
            3.0 * u * (1.0 - u).powi(2),
            3.0 * u * u * (1.0 - u),
            u.powi(3),
        ];
        let expected = b[1] + 3.0 * b[2] + 2.0 * b[3];
        assert!((s.eval(u) - expected).abs() < 1e-14);
        assert_eq!(s.eval(0.0), 0.0);
        assert_eq!(s.eval(1.0), 2.0);
        assert_eq!(s.eval(2.0), 2.0, "evaluation clamps into the domain");
    }

    #[test]
    fn bspline_rejects_invalid_input() {
        let ok = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let c = vec![0.0; 4];
        assert!(ClampedBSpline::new(ok.clone(), c.clone(), 3).is_ok());
        // Excess end multiplicity (an empty first or last span) is refused.
        let excess = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
        assert!(matches!(
            ClampedBSpline::new(excess, vec![0.0; 6], 3),
            Err(ClampedBSplineError::Multiplicity(_))
        ));
        let excess_end = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0];
        assert!(matches!(
            ClampedBSpline::new(excess_end, vec![0.0; 6], 3),
            Err(ClampedBSplineError::Multiplicity(_))
        ));
        assert!(matches!(
            ClampedBSpline::new(ok.clone(), c.clone(), 0),
            Err(ClampedBSplineError::Degree(0))
        ));
        assert!(matches!(
            ClampedBSpline::new(ok.clone(), vec![0.0; 5], 3),
            Err(ClampedBSplineError::Counts { .. })
        ));
        let mut unclamped = ok.clone();
        unclamped[3] = 0.1;
        assert!(matches!(
            ClampedBSpline::new(unclamped, c.clone(), 3),
            Err(ClampedBSplineError::NotClamped)
        ));
        let mut decreasing = vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.4, 1.0, 1.0, 1.0, 1.0];
        decreasing.truncate(10);
        assert!(matches!(
            ClampedBSpline::new(decreasing, vec![0.0; 6], 3),
            Err(ClampedBSplineError::NotMonotone(_))
        ));
        let nan = vec![f64::NAN; 4];
        assert!(matches!(
            ClampedBSpline::new(ok, nan, 3),
            Err(ClampedBSplineError::NonFinite(0))
        ));
    }

    #[test]
    fn bspline_one_sided_limits_agree_across_a_simple_knot() {
        // A cubic with a simple interior knot is C2: value, slope and
        // curvature agree from both sides, the third derivative does not.
        let knots = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
        let s = ClampedBSpline::new(knots, vec![0.0, 1.0, -1.0, 2.0, 0.5], 3).unwrap();
        let left = s.derivatives_in_span(3, 0.5);
        let right = s.derivatives_in_span(4, 0.5);
        for k in 0..3 {
            assert!((left[k] - right[k]).abs() < 1e-13, "order {k}");
        }
        assert!((left[3] - right[3]).abs() > 1e-3);
    }

    #[test]
    fn bspline_constant_intervals_find_the_flat_middle() {
        // Quintic with equal coefficients over the middle: constant there.
        let p = 5;
        let knots = vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let coeffs = vec![0.0, 0.3, 0.9, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0];
        let s = ClampedBSpline::new(knots, coeffs, p).unwrap();
        let flat = s.constant_intervals(1e-12);
        assert_eq!(flat.len(), 1);
        assert!(
            (flat[0].0 - 0.6).abs() < 1e-12 && (flat[0].1 - 0.8).abs() < 1e-12,
            "{flat:?}"
        );
        let mid = s.derivatives(0.7);
        assert!((mid[0] - 1.0).abs() < 1e-13 && mid[1].abs() < 1e-12 && mid[3].abs() < 1e-10);
    }
}
