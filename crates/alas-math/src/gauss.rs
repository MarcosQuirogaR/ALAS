// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Gauss-Legendre quadrature rules.
//!
//! Nodes are the roots of the Legendre polynomial `P_n`, found by Newton
//! iteration from the Tricomi starting guess (Abramowitz and Stegun 25.4.29;
//! weights `2 / ((1 - x^2) P_n'(x)^2)`), to a few units in the last place for
//! the orders the lofted-geometry code uses. [`cached_rule`] serves the common
//! orders 8, 12, 24 and 48 from a table built once per process.

use std::sync::OnceLock;

/// An `n`-point Gauss-Legendre rule on `[-1, 1]`: exact for polynomials of
/// degree up to `2n - 1`. Nodes ascend; weights sum to 2.
#[derive(Debug, Clone, PartialEq)]
pub struct GaussRule {
    /// Abscissae in `(-1, 1)`, ascending.
    pub nodes: Vec<f64>,
    /// Weights, positive, symmetric about the middle.
    pub weights: Vec<f64>,
}

impl GaussRule {
    /// Number of points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the rule has no points (never true for a built rule).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// `(x, w)` pairs mapped affinely onto `[a, b]`: the weights include the
    /// Jacobian `(b - a) / 2`.
    pub fn on_interval(&self, a: f64, b: f64) -> impl Iterator<Item = (f64, f64)> + '_ {
        let half = 0.5 * (b - a);
        let mid = 0.5 * (b + a);
        self.nodes
            .iter()
            .zip(&self.weights)
            .map(move |(&x, &w)| (mid + half * x, half * w))
    }

    /// The integral of `f` over `[a, b]`.
    pub fn integrate(&self, a: f64, b: f64, mut f: impl FnMut(f64) -> f64) -> f64 {
        self.on_interval(a, b).map(|(x, w)| w * f(x)).sum()
    }
}

/// The `n`-point rule, computed on the spot. `n = 0` gives an empty rule.
#[must_use]
pub fn gauss_legendre(n: usize) -> GaussRule {
    let mut nodes = vec![0.0; n];
    let mut weights = vec![0.0; n];
    let nf = n as f64;
    for i in 0..n.div_ceil(2) {
        // Roots are symmetric: solve for the negative half, mirror the rest.
        let mut x = -(std::f64::consts::PI * (i as f64 + 0.75) / (nf + 0.5)).cos();
        let mut dp = 1.0;
        for _ in 0..100 {
            let (mut p0, mut p1) = (1.0, x);
            for k in 2..=n {
                let kf = k as f64;
                let p2 = ((2.0 * kf - 1.0) * x * p1 - (kf - 1.0) * p0) / kf;
                p0 = p1;
                p1 = p2;
            }
            let pn = p1;
            let pn1 = p0;
            dp = nf * (x * pn - pn1) / (x * x - 1.0);
            let step = pn / dp;
            x -= step;
            if step.abs() < 1e-16 {
                break;
            }
        }
        let w = 2.0 / ((1.0 - x * x) * dp * dp);
        nodes[i] = x;
        weights[i] = w;
        nodes[n - 1 - i] = -x;
        weights[n - 1 - i] = w;
    }
    if n % 2 == 1 {
        nodes[n / 2] = 0.0;
    }
    GaussRule { nodes, weights }
}

/// Orders served by [`cached_rule`].
pub const CACHED_ORDERS: [usize; 4] = [8, 12, 24, 48];

/// The tabulated rule of order 8, 12, 24 or 48, built once; `None` for any
/// other order (use [`gauss_legendre`]).
#[must_use]
pub fn cached_rule(n: usize) -> Option<&'static GaussRule> {
    static TABLE: OnceLock<Vec<GaussRule>> = OnceLock::new();
    let index = CACHED_ORDERS.iter().position(|&m| m == n)?;
    let table = TABLE.get_or_init(|| CACHED_ORDERS.iter().map(|&m| gauss_legendre(m)).collect());
    table.get(index)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_legendre_matches_published_eight_point_values() {
        // Abramowitz and Stegun Table 25.4, n = 8.
        let r = gauss_legendre(8);
        assert!((r.weights[4] - 0.362_683_783_378_362).abs() < 1e-15);
        assert!((r.nodes[4] - 0.183_434_642_495_649_8).abs() < 1e-15);
        assert!((r.nodes[0] + 0.960_289_856_497_536_3).abs() < 1e-15);
        assert!((r.weights[0] - 0.101_228_536_290_376_26).abs() < 1e-15);
    }

    #[test]
    fn gauss_legendre_integrates_polynomials_up_to_degree_2n_minus_1_exactly() {
        for &n in &CACHED_ORDERS {
            let rule = cached_rule(n).unwrap();
            let sum: f64 = rule.weights.iter().sum();
            assert!((sum - 2.0).abs() < 1e-14, "n={n} weight sum {sum}");
            for degree in [0usize, 1, 2, 7, 8, 15, 2 * n - 1] {
                let exact = if degree % 2 == 1 {
                    0.0
                } else {
                    2.0 / (degree as f64 + 1.0)
                };
                let got = rule.integrate(-1.0, 1.0, |x| x.powi(degree as i32));
                assert!(
                    (got - exact).abs() < 1e-13,
                    "n={n} degree={degree} got={got} exact={exact}"
                );
            }
        }
    }

    #[test]
    fn gauss_legendre_maps_to_an_interval_and_integrates_a_smooth_function() {
        let rule = cached_rule(12).unwrap();
        let got = rule.integrate(0.0, std::f64::consts::PI, f64::sin);
        assert!((got - 2.0).abs() < 1e-13);
        let shifted = rule.integrate(2.0, 5.0, |x| x * x);
        assert!((shifted - (125.0 - 8.0) / 3.0).abs() < 1e-12);
    }

    #[test]
    fn gauss_legendre_odd_order_has_a_zero_node_and_uncached_orders_are_none() {
        let r = gauss_legendre(5);
        assert_eq!(r.nodes[2], 0.0);
        assert!((r.weights[2] - 128.0 / 225.0).abs() < 1e-15);
        assert!(cached_rule(7).is_none());
        assert!(gauss_legendre(0).is_empty());
    }
}
