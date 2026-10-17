// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The gamma and digamma functions for positive arguments.
//!
//! The area of a superellipse `|x/a|^n + |y/b|^n = 1` is
//! `4 a b Gamma(1 + 1/n)^2 / Gamma(1 + 2/n)` and its derivative in `n` needs
//! the digamma function; these are the only special functions the lofted
//! sections use, so they live here rather than in a dependency.
//!
//! Gamma is the Lanczos approximation with `g = 7` and nine coefficients
//! (Numerical Recipes 3rd ed. section 6.1 / Godfrey), relative error about
//! 1e-15 for `x > 0`. Digamma shifts the argument up to 10 by the recurrence
//! `psi(x) = psi(x + 1) - 1/x` and applies the asymptotic series
//! (Abramowitz and Stegun 6.3.18).

const LANCZOS_G: f64 = 7.0;
const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

/// `ln Gamma(x)` for `x > 0`; NaN otherwise.
#[must_use]
pub fn ln_gamma(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return f64::NAN;
    }
    if x < 0.5 {
        // Reflection: Gamma(x) Gamma(1 - x) = pi / sin(pi x).
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = LANCZOS[0];
    for (i, c) in LANCZOS.iter().enumerate().skip(1) {
        sum += c / (x + i as f64);
    }
    let t = x + LANCZOS_G + 0.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

/// `Gamma(x)` for `x > 0`; NaN otherwise. Exact integers for small integer
/// arguments to within the approximation error.
#[must_use]
pub fn gamma(x: f64) -> f64 {
    ln_gamma(x).exp()
}

/// The digamma function `psi(x) = Gamma'(x) / Gamma(x)` for `x > 0`.
#[must_use]
pub fn digamma(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return f64::NAN;
    }
    let mut x = x;
    let mut shift = 0.0;
    while x < 10.0 {
        shift -= 1.0 / x;
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    let series = inv2
        * (1.0 / 12.0
            - inv2 * (1.0 / 120.0 - inv2 * (1.0 / 252.0 - inv2 * (1.0 / 240.0 - inv2 / 132.0))));
    shift + x.ln() - 0.5 * inv - series
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_matches_factorials_and_half_integers() {
        let mut fact = 1.0;
        for k in 1..=15u32 {
            let g = gamma(f64::from(k));
            assert!(
                (g / fact - 1.0).abs() < 1e-13,
                "Gamma({k}) = {g}, want {fact}"
            );
            fact *= f64::from(k);
        }
        let sqrt_pi = std::f64::consts::PI.sqrt();
        assert!((gamma(0.5) / sqrt_pi - 1.0).abs() < 1e-14);
        assert!((gamma(1.5) / (0.5 * sqrt_pi) - 1.0).abs() < 1e-14);
        assert!(gamma(-1.0).is_nan());
    }

    #[test]
    fn digamma_matches_known_values_and_the_gamma_derivative() {
        let euler = 0.577_215_664_901_532_9;
        assert!((digamma(1.0) + euler).abs() < 1e-13);
        let psi_half = -euler - 2.0 * 2.0_f64.ln();
        assert!((digamma(0.5) - psi_half).abs() < 1e-13);
        // psi(x) is d/dx ln Gamma(x): compare with a central difference.
        for x in [1.2, 1.55, 2.7, 9.0, 30.0] {
            let h = 1e-6 * x;
            let fd = (ln_gamma(x + h) - ln_gamma(x - h)) / (2.0 * h);
            assert!((digamma(x) - fd).abs() < 1e-7, "x={x}");
        }
    }
}
