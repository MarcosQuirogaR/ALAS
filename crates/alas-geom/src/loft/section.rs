// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The warped-superellipse cross-section family (design 2.1).
//!
//! A section is centred on `y = 0` and described by five numbers: half-width
//! `a`, centre height `zc`, half-height `e`, exponent `n` and shoulder offset
//! `k`. With `eta = sgn(cos) |cos|^(2/n)` and `zeta = sgn(sin) |sin|^(2/n)`
//! (so `|eta|^n + |zeta|^n = 1`)
//!
//! ```text
//! y = a * eta
//! z = zc + e * (zeta + k * (1 - zeta^2))          (abs(k) < 0.5: monotone warp)
//! ```
//!
//! The top and bottom are exactly `zc +- e` at `y = 0`, the widest point is
//! exactly `y = a` at `z = zc + k e`, and the area `4 a e g(n)` with
//! `g(n) = Gamma(1 + 1/n)^2 / Gamma(1 + 2/n)` does not depend on `k` because
//! the warp changes `z` by an odd function of `zeta` at fixed `|eta|`.
//! `n = 2, k = 0` is the ellipse.
//!
//! An optional upper-arc residual `y *= 1 + R(zeta)` (see [`super::residual`])
//! narrows or widens the section above the widest point without touching `z`,
//! the crown or the keel; the closed-form area then gains a quadrature term.

use alas_math::gauss::cached_rule;
use alas_math::special::{digamma, gamma};

/// Fraction of the quarter turn handled by the `zeta` chart: the chart split
/// is where `|zeta| = |eta| = 2^(-1/n)`.
fn chart_limit(n: f64) -> f64 {
    (-std::f64::consts::LN_2 / n).exp()
}

/// `h(u; n) = (1 - u^n)^(1/n)`: the partner coordinate of `u` on the unit
/// superellipse, for `0 <= u <= 2^(-1/n)`.
pub(super) fn partner(u: f64, n: f64) -> f64 {
    (1.0 - u.powf(n)).max(0.0).powf(1.0 / n)
}

/// `dh/du = -u^(n-1) h^(1-n)`.
pub(super) fn partner_du(u: f64, h: f64, n: f64) -> f64 {
    if u <= 0.0 {
        0.0
    } else {
        -u.powf(n - 1.0) * h.powf(1.0 - n)
    }
}

/// `dh/dn`, with `ln h = ln(1 - u^n) / n`.
pub(super) fn partner_dn(u: f64, h: f64, n: f64) -> f64 {
    if u <= 0.0 {
        return 0.0;
    }
    let un = u.powf(n);
    h * (-(1.0 - un).ln() / (n * n) - un * u.ln() / (n * (1.0 - un)))
}

/// `4 g(n)`: the area of the unit superellipse `|x|^n + |y|^n = 1`.
#[must_use]
pub fn unit_area(n: f64) -> f64 {
    4.0 * gamma(1.0 + 1.0 / n).powi(2) / gamma(1.0 + 2.0 / n)
}

/// `d(unit_area)/dn`, exact: `unit_area(n) * 2 (psi(1 + 2/n) - psi(1 + 1/n)) / n^2`.
#[must_use]
pub fn unit_area_dn(n: f64) -> f64 {
    unit_area(n) * 2.0 * (digamma(1.0 + 2.0 / n) - digamma(1.0 + 1.0 / n)) / (n * n)
}

/// One cross-section of the family, in the fuselage frame (metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    /// Half-width `a`, m.
    pub a: f64,
    /// Half-height `e`, m.
    pub e: f64,
    /// Centre height `zc`, m.
    pub zc: f64,
    /// Exponent `n`.
    pub n: f64,
    /// Shoulder offset `k`.
    pub k: f64,
    /// Upper-arc residual coefficients `b_0..b_3` (zero: plain section), see
    /// [`super::residual`].
    pub res: [f64; 4],
}

/// Which quarter-chart a boundary integral is evaluated on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chart {
    /// Parameter `zeta = sigma u`, `eta = h(u)`: the flat part (sides near the
    /// top and bottom).
    Zeta,
    /// Parameter `eta = u`, `zeta = sigma h(u)`: the steep part near the
    /// widest point.
    Eta,
}

/// The first-order geometry of one chart point at section parameters
/// `(a, e, n, k)`: `(y, z - zc)`, `(dy/du, dz/du)` at fixed section.
pub(crate) struct ChartPoint {
    /// `h(u)`, the partner coordinate.
    pub h: f64,
    /// `dh/dn`.
    pub h_n: f64,
    /// `zeta` of the point.
    pub zeta: f64,
    /// `d y / d u`.
    pub y_u: f64,
    /// `d z / d u`.
    pub z_u: f64,
    /// Residual width factor `1 + R` of the point (1 without a residual).
    pub fac: f64,
    /// `dR/d zeta` of the point (0 without a residual).
    pub r_t: f64,
}

impl Section {
    /// A section from its five numbers.
    #[must_use]
    pub fn new(a: f64, e: f64, zc: f64, n: f64, k: f64) -> Self {
        Self {
            a,
            e,
            zc,
            n,
            k,
            res: [0.0; 4],
        }
    }

    /// `4 a e g(n)` plus the residual part `2 a e sum_j b_j I_j` (exact to
    /// quadrature accuracy, see [`super::residual`]).
    #[must_use]
    pub fn area(&self) -> f64 {
        let base = self.a * self.e * unit_area(self.n);
        if self.has_residual() {
            base + self.residual_area()
        } else {
            base
        }
    }

    /// Crown height `zc + e`, m.
    #[must_use]
    pub fn crown_z(&self) -> f64 {
        self.zc + self.e
    }

    /// Keel height `zc - e`, m.
    #[must_use]
    pub fn keel_z(&self) -> f64 {
        self.zc - self.e
    }

    /// Height of the widest point `zc + k e`, m.
    #[must_use]
    pub fn shoulder_z(&self) -> f64 {
        self.zc + self.k * self.e
    }

    /// The warp `zeta + k (1 - zeta^2)`.
    fn warp(&self, zeta: f64) -> f64 {
        zeta + self.k * (1.0 - zeta * zeta)
    }

    /// `zeta` of the boundary point at height `z`, or `None` outside
    /// `[keel, crown]` (and for a degenerate section). The warp is a
    /// quadratic in `zeta`, inverted in the cancellation-free form
    /// `zeta = 2 (w - k) / (1 + sqrt(1 - 4 k (w - k)))`, `w = (z - zc) / e`.
    #[must_use]
    pub fn zeta_at(&self, z: f64) -> Option<f64> {
        if self.e.is_nan() || self.e <= 0.0 || !z.is_finite() {
            return None;
        }
        let w = (z - self.zc) / self.e;
        if w.abs() > 1.0 + 1e-12 {
            return None;
        }
        let w = w.clamp(-1.0, 1.0);
        let disc = 1.0 - 4.0 * self.k * (w - self.k);
        let zeta = 2.0 * (w - self.k) / (1.0 + disc.max(0.0).sqrt());
        Some(zeta.clamp(-1.0, 1.0))
    }

    /// Half-width of the section at height `z`: `a (1 - |zeta|^n)^(1/n)`,
    /// zero outside the section.
    #[must_use]
    pub fn half_width_at(&self, z: f64) -> f64 {
        self.zeta_at(z).map_or(0.0, |zeta| {
            let w = self.a * partner(zeta.abs(), self.n);
            if zeta > 0.0 && self.has_residual() {
                w * self.width_factor(zeta).0
            } else {
                w
            }
        })
    }

    /// First-order geometry of a chart point; `u` in `[0, 2^(-1/n)]`,
    /// `sigma = +-1`.
    pub(crate) fn chart_point(&self, chart: Chart, sigma: f64, u: f64) -> ChartPoint {
        let n = self.n;
        let h = partner(u, n);
        let h_u = partner_du(u, h, n);
        let h_n = partner_dn(u, h, n);
        // The residual acts on the upper arc (sigma > 0) only.
        let active = sigma > 0.0 && self.has_residual();
        let (fac, r_t) = if active {
            self.width_factor(match chart {
                Chart::Zeta => u,
                Chart::Eta => h,
            })
        } else {
            (1.0, 0.0)
        };
        match chart {
            Chart::Zeta => ChartPoint {
                h,
                h_n,
                zeta: sigma * u,
                y_u: if active {
                    self.a * (h_u * fac + h * r_t)
                } else {
                    self.a * h_u
                },
                z_u: self.e * (sigma - 2.0 * self.k * u),
                fac,
                r_t,
            },
            Chart::Eta => ChartPoint {
                h,
                h_n,
                zeta: sigma * h,
                y_u: if active {
                    self.a * (fac + u * r_t * h_u)
                } else {
                    self.a
                },
                z_u: self.e * (sigma - 2.0 * self.k * h) * h_u,
                fac,
                r_t,
            },
        }
    }

    /// Chart limit `2^(-1/n)`.
    pub(crate) fn chart_limit(&self) -> f64 {
        chart_limit(self.n)
    }

    /// The warp value for the chart point (used by the surface integral).
    pub(crate) fn warp_of(&self, zeta: f64) -> f64 {
        self.warp(zeta)
    }

    /// Perimeter of the section, m.
    ///
    /// Arc length of the right half (the section is symmetric about `y = 0`)
    /// in four pieces on two charts so every integrand is smooth: the
    /// `zeta` chart for `|zeta| <= 2^(-1/n)` on both the upper and lower
    /// side, and the `eta` chart for `|eta| <= 2^(-1/n)` likewise. Each piece
    /// is integrated with the 24-point Gauss-Legendre rule after the
    /// substitution `u = u_max s^2`, which weakens the `u^(n-1)` end
    /// non-analyticity for non-integer `n`. Accuracy is a few 1e-12 relative
    /// for `n` from 2 to 6 (and down to 1.8, below the allowed range).
    #[must_use]
    pub fn perimeter(&self) -> f64 {
        let Some(rule) = cached_rule(24) else {
            return f64::NAN;
        };
        let u_max = self.chart_limit();
        let mut total = 0.0;
        for chart in [Chart::Zeta, Chart::Eta] {
            for sigma in [1.0, -1.0] {
                total += rule.integrate(0.0, 1.0, |s| {
                    let u = u_max * s * s;
                    let p = self.chart_point(chart, sigma, u);
                    p.y_u.hypot(p.z_u) * 2.0 * u_max * s
                });
            }
        }
        2.0 * total
    }

    /// Boundary point `(y, z)` for the angle-like parameter `phi` in
    /// `[0, 2 pi)` of the design (`eta = sgn(cos) |cos|^(2/n)` and so on).
    #[must_use]
    pub fn point_at_angle(&self, phi: f64) -> (f64, f64) {
        let (s, c) = phi.sin_cos();
        let p = 2.0 / self.n;
        let eta = c.signum() * c.abs().powf(p);
        let zeta = s.signum() * s.abs().powf(p);
        let fac = if zeta > 0.0 && self.has_residual() {
            self.width_factor(zeta).0
        } else {
            1.0
        };
        (self.a * eta * fac, self.zc + self.e * self.warp(zeta))
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn section_area_of_the_ellipse_and_the_square_limit() {
        assert!((unit_area(2.0) - PI).abs() < 1e-14);
        let s = Section::new(3.0, 2.0, 0.0, 2.0, 0.0);
        assert!((s.area() - PI * 6.0).abs() < 1e-13);
        // n = 1 is the diamond of area 2, large n tends to the square of area 4.
        assert!((unit_area(1.0) - 2.0).abs() < 1e-13);
        assert!((unit_area(40.0) - 4.0).abs() < 0.05);
    }

    #[test]
    fn section_area_is_independent_of_the_shoulder_offset() {
        let flat = Section::new(1.7, 1.1, 0.3, 2.4, 0.0).area();
        for k in [-0.45, -0.2, 0.3, 0.49] {
            let s = Section::new(1.7, 1.1, 0.3, 2.4, k);
            assert!((s.area() - flat).abs() < 1e-13, "k = {k}");
            // Independent check: shoelace area of a dense boundary polygon.
            let m = 20_000;
            let pts: Vec<(f64, f64)> = (0..m)
                .map(|i| s.point_at_angle(2.0 * PI * i as f64 / m as f64))
                .collect();
            let mut twice = 0.0;
            for i in 0..m {
                let (y0, z0) = pts[i];
                let (y1, z1) = pts[(i + 1) % m];
                twice += y0 * z1 - y1 * z0;
            }
            assert!((0.5 * twice.abs() - flat).abs() < 2e-4, "k = {k}");
        }
    }

    #[test]
    fn section_area_derivative_in_n_matches_a_central_difference() {
        for n in [1.9, 2.0, 2.6, 4.0] {
            let h = 1e-5;
            let fd = (unit_area(n + h) - unit_area(n - h)) / (2.0 * h);
            assert!((unit_area_dn(n) - fd).abs() < 1e-8, "n = {n}");
        }
    }

    #[test]
    fn section_perimeter_matches_the_circle_the_ellipse_and_the_quartic_reference() {
        let circle = Section::new(2.5, 2.5, 1.0, 2.0, 0.0);
        assert!((circle.perimeter() - 2.0 * PI * 2.5).abs() < 1e-11);
        // Ellipse a = 2, b = 1: 4 a E(m = 1 - b^2/a^2) = 9.688448220547675
        // (Ramanujan-independent reference from the complete elliptic integral).
        let ellipse = Section::new(2.0, 1.0, 0.0, 2.0, 0.0);
        assert!(
            (ellipse.perimeter() - 9.688_448_220_547_675).abs() < 1e-10,
            "{}",
            ellipse.perimeter()
        );
        // Superellipses against adaptive quadrature of the arc length (scipy
        // `quad`, tolerance 1e-14): unit n = 4, n = 1.8 and 6, and a
        // 1.7 x 1.1 section with n = 2.4.
        for (n, a, e, want, tol) in [
            (4.0, 1.0, 1.0, 7.017_697_943_564, 1e-10),
            (1.8, 1.0, 1.0, 6.154_933_595_384, 1e-9),
            (6.0, 1.0, 1.0, 7.317_726_358_60, 1e-9),
            (2.4, 1.7, 1.1, 9.183_612_393_254, 1e-9),
        ] {
            let p = Section::new(a, e, 0.0, n, 0.0).perimeter();
            assert!((p - want).abs() < tol, "n = {n}: {p} vs {want}");
        }
    }

    #[test]
    fn section_perimeter_scales_linearly_and_agrees_with_a_polygon() {
        // Scaling a and e together scales the perimeter linearly.
        let a = Section::new(1.2, 0.9, 0.0, 2.3, 0.2);
        let b = Section::new(2.4, 1.8, 5.0, 2.3, 0.2);
        assert!((b.perimeter() - 2.0 * a.perimeter()).abs() < 1e-11);
        // A polygon through the boundary is shorter but within its resolution.
        let m = 40_000;
        let pts: Vec<(f64, f64)> = (0..=m)
            .map(|i| a.point_at_angle(2.0 * PI * i as f64 / m as f64))
            .collect();
        let poly: f64 = pts
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        assert!(
            (poly - a.perimeter()).abs() < 1e-4,
            "{poly} vs {}",
            a.perimeter()
        );
    }

    #[test]
    fn section_half_width_round_trip_and_extreme_heights() {
        let s = Section::new(1.9, 2.1, 0.4, 2.4, 0.3);
        assert!((s.half_width_at(s.shoulder_z()) - 1.9).abs() < 1e-12);
        assert!(s.half_width_at(s.crown_z()).abs() < 1e-7);
        assert!(s.half_width_at(s.keel_z()).abs() < 1e-7);
        assert_eq!(s.half_width_at(s.crown_z() + 0.01), 0.0);
        for phi in [0.3, 0.9, 1.4, 2.2, 3.5, 5.0, 6.0] {
            let (y, z) = s.point_at_angle(phi);
            let w = s.half_width_at(z);
            assert!(
                (w - y.abs()).abs() < 1e-10,
                "phi = {phi}: {w} vs {}",
                y.abs()
            );
        }
        let ellipse = Section::new(2.0, 1.0, 0.0, 2.0, 0.0);
        let w = ellipse.half_width_at(0.6);
        assert!((w - 2.0 * (1.0_f64 - 0.36).sqrt()).abs() < 1e-14);
    }
}
