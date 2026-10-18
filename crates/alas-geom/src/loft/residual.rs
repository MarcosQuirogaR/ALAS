// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evaluation of the optional upper-arc cross-section residual (design 2.1,
//! "optional residual"; data in [`alas_config::loft::ResidualSpec`]).
//!
//! With `zeta in [0, 1]` the height parameter above the widest point and
//! `h(zeta) = (1 - zeta^n)^(1/n)` the unit superellipse half-width, the upper
//! arc of a section with coefficients `b` is
//!
//! ```text
//! y = a h(zeta) (1 + R(zeta; b)),     z = zc + e (zeta + k (1 - zeta^2)),
//! R(t; b) = sum_{j=0..3} b_j B_{j+3,6}(t)           (Bernstein, degree 6)
//! ```
//!
//! The lower arc is untouched. `R = R' = R'' = 0` at `zeta = 0`, so the section
//! stays C2 (G2) at the widest point; `z` is not modified, so crown, keel and
//! the side silhouette are exactly the plain section's, and the widest point
//! is still `y = a` while `h (1 + R) <= 1`. Area, perimeter, skin area and the
//! inverse half-width are evaluated exactly (to quadrature accuracy) on the
//! same two charts as the plain section; the half-width at a height is a
//! closed form because the arc is a graph `y(zeta(z))`.

use alas_config::loft::{residual_basis, residual_slope, residual_value, MAX_RESIDUAL_TERMS};
use alas_math::gauss::cached_rule;

use super::section::{partner, partner_dn, partner_du, Section};

/// Coefficient or moment vector, one entry per residual term.
pub(crate) type Terms = [f64; MAX_RESIDUAL_TERMS];

/// Number of points per quarter turn of the convexity polyline.
const CONVEXITY_POINTS: usize = 360;
/// Convexity tolerance: smallest allowed sine of the turning angle between
/// consecutive polyline edges.
const CONVEXITY_TOLERANCE: f64 = -1e-9;
/// Slack on `h (1 + R) <= 1` (the widest point is the shoulder).
const WIDEST_POINT_SLACK: f64 = 1e-9;

/// Why a residual section is not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ResidualDefect {
    /// The corrected upper arc is wider than the shoulder `a`.
    #[error("the corrected upper arc is wider than the shoulder (ratio {ratio} at zeta {zeta})")]
    WiderThanShoulder {
        /// `zeta` of the offending point.
        zeta: f64,
        /// `y / a` there.
        ratio: f64,
    },
    /// The section is not convex.
    #[error("the section is not convex (turning sine {turn} at phi {phi})")]
    NotConvex {
        /// Angle parameter of the offending vertex.
        phi: f64,
        /// Sine of the (negative) turning angle.
        turn: f64,
    },
}

/// `I_j = int_0^1 h w_j (1 - 2 k t) dt` and its derivatives in `n` and `k`
/// (with `w_j = B_{j+3,6}`), so that the residual adds
/// `2 a e sum_j b_j I_j` to the section area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Moments {
    pub i: Terms,
    pub i_n: Terms,
    pub i_k: Terms,
}

impl Section {
    /// This section with residual coefficients `b`.
    #[must_use]
    pub fn with_residual(mut self, b: [f64; MAX_RESIDUAL_TERMS]) -> Self {
        self.res = b;
        self
    }

    /// `true` if any residual coefficient is not zero.
    #[must_use]
    pub fn has_residual(&self) -> bool {
        self.res.iter().any(|v| *v != 0.0)
    }

    /// `(1 + R(t), dR/dt)` of this section at `zeta = t`.
    pub(crate) fn width_factor(&self, t: f64) -> (f64, f64) {
        (
            1.0 + residual_value(&self.res, t),
            residual_slope(&self.res, t),
        )
    }

    /// Residual moments by Gauss-Legendre on the two charts (24 nodes per
    /// piece, `u = u_max s^2`).
    #[allow(clippy::needless_range_loop)] // parallel arrays indexed by term
    pub(crate) fn residual_moments(&self) -> Moments {
        let mut m = Moments {
            i: [0.0; MAX_RESIDUAL_TERMS],
            i_n: [0.0; MAX_RESIDUAL_TERMS],
            i_k: [0.0; MAX_RESIDUAL_TERMS],
        };
        let Some(rule) = cached_rule(24) else {
            return Moments {
                i: [f64::NAN; MAX_RESIDUAL_TERMS],
                i_n: [f64::NAN; MAX_RESIDUAL_TERMS],
                i_k: [f64::NAN; MAX_RESIDUAL_TERMS],
            };
        };
        let (n, k) = (self.n, self.k);
        let u_max = self.chart_limit();
        for (s, ws) in rule.on_interval(0.0, 1.0) {
            let u = u_max * s * s;
            let du = 2.0 * u_max * s * ws;
            // Zeta chart: t = u.
            {
                let h = partner(u, n);
                let h_n = partner_dn(u, h, n);
                let w = residual_basis(u);
                for j in 0..MAX_RESIDUAL_TERMS {
                    m.i[j] += du * h * w[j] * (1.0 - 2.0 * k * u);
                    m.i_n[j] += du * h_n * w[j] * (1.0 - 2.0 * k * u);
                    m.i_k[j] += du * -2.0 * h * w[j] * u;
                }
            }
            // Eta chart: eta = u, t = h(eta); dt = |h'(eta)| d eta, h(t) = eta.
            {
                let t = partner(u, n);
                let dt = -partner_du(u, t, n);
                let eta_n = partner_dn_of_partner(u, t, n);
                let w = residual_basis(t);
                for j in 0..MAX_RESIDUAL_TERMS {
                    m.i[j] += du * dt * u * w[j] * (1.0 - 2.0 * k * t);
                    m.i_n[j] += du * dt * eta_n * w[j] * (1.0 - 2.0 * k * t);
                    m.i_k[j] += du * dt * -2.0 * u * w[j] * t;
                }
            }
        }
        m
    }

    /// The residual part of the area, `2 a e sum_j b_j I_j`, m^2.
    #[must_use]
    pub fn residual_area(&self) -> f64 {
        let m = self.residual_moments();
        let sum: f64 = self.res.iter().zip(m.i).map(|(b, i)| b * i).sum();
        2.0 * self.a * self.e * sum
    }

    /// Axial derivative of the residual area part. `a_x`, `e_x` are `da/dx`,
    /// `de/dx`; `n_x`, `k_x` those of the exponent and shoulder; `b_x` the
    /// derivatives of the coefficients.
    #[allow(clippy::needless_range_loop)] // parallel arrays indexed by term
    pub(crate) fn residual_area_rate(
        &self,
        a_x: f64,
        e_x: f64,
        n_x: f64,
        k_x: f64,
        b_x: &Terms,
    ) -> f64 {
        let m = self.residual_moments();
        let (mut value, mut shape, mut coeff) = (0.0, 0.0, 0.0);
        for j in 0..MAX_RESIDUAL_TERMS {
            value += self.res[j] * m.i[j];
            shape += self.res[j] * (m.i_n[j] * n_x + m.i_k[j] * k_x);
            coeff += b_x[j] * m.i[j];
        }
        2.0 * ((a_x * self.e + self.a * e_x) * value + self.a * self.e * (shape + coeff))
    }

    /// Check the residual section: the corrected arc must not exceed the
    /// shoulder width and the whole section (lower arc, shoulder joins and
    /// crown) must be convex.
    ///
    /// # Errors
    ///
    /// [`ResidualDefect`] describing the first violation.
    pub fn check_residual(&self) -> Result<(), ResidualDefect> {
        if !self.has_residual() {
            return Ok(());
        }
        for i in 1..=200 {
            let zeta = f64::from(i) / 200.0;
            let ratio = partner(zeta, self.n) * self.width_factor(zeta).0;
            if ratio > 1.0 + WIDEST_POINT_SLACK {
                return Err(ResidualDefect::WiderThanShoulder { zeta, ratio });
            }
        }
        let total = 2 * CONVEXITY_POINTS + 2 * (CONVEXITY_POINTS / 6);
        let phi0 = -std::f64::consts::FRAC_PI_6;
        let step = (std::f64::consts::PI + std::f64::consts::FRAC_PI_3) / total as f64;
        let pts: Vec<(f64, f64)> = (0..=total)
            .map(|i| self.point_at_angle(phi0 + step * i as f64))
            .collect();
        for (i, w) in pts.windows(3).enumerate() {
            let (e1, e2) = (
                (w[1].0 - w[0].0, w[1].1 - w[0].1),
                (w[2].0 - w[1].0, w[2].1 - w[1].1),
            );
            let (l1, l2) = (e1.0.hypot(e1.1), e2.0.hypot(e2.1));
            if l1 < 1e-12 || l2 < 1e-12 {
                continue;
            }
            let turn = (e1.0 * e2.1 - e1.1 * e2.0) / (l1 * l2);
            if turn < CONVEXITY_TOLERANCE {
                return Err(ResidualDefect::NotConvex {
                    phi: phi0 + step * (i + 1) as f64,
                    turn,
                });
            }
        }
        Ok(())
    }
}

/// `d eta / d n` at fixed `t` on `t^n + eta^n = 1`, with `t = (1 - eta^n)^(1/n)`
/// (`ln t = ln_1p(-eta^n) / n` keeps small `eta` accurate).
fn partner_dn_of_partner(eta: f64, t: f64, n: f64) -> f64 {
    let ln_t = (-eta.powf(n)).ln_1p() / n;
    -(t.powf(n) * ln_t + eta.powf(n) * eta.ln()) / (n * eta.powf(n - 1.0))
}
