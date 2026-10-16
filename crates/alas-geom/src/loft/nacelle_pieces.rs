// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Profile pieces of the nacelle revolve (design 3.1): the lip superellipse
//! with a vertical tangent at the highlight, and quintic Hermite pieces.
//!
//! A piece is a curve `(x(t), r(t))`, `t` in `[0, 1]`, of the axial station
//! and the local radius. Quintic pieces carry `x` linear in `t`, so they are
//! graphs `r(x)`; the lip is parametrised by the radial fraction `w` of its
//! rise, in which the superellipse is smooth up to the vertical tangent.
//! Slope `m = dr/dx` and curvature `c = d2r/dx2` at the ends of a quintic are
//! free data, so a chain of pieces is G2 where they are matched.

/// A profile point and its first two derivatives with respect to the piece
/// parameter.
#[derive(Debug, Clone, Copy)]
pub(super) struct Point {
    pub x: f64,
    pub r: f64,
    pub dx: f64,
    pub dr: f64,
    pub ddx: f64,
    pub ddr: f64,
}

impl Point {
    /// `dr/dx` (infinite at a vertical tangent).
    pub fn slope(&self) -> f64 {
        self.dr / self.dx
    }

    /// `d2r/dx2`.
    pub fn curvature(&self) -> f64 {
        (self.ddr * self.dx - self.dr * self.ddx) / self.dx.powi(3)
    }
}

/// One piece of a profile.
#[derive(Debug, Clone, Copy)]
pub(super) enum Piece {
    /// `x = x0 + ax (1 - (1 - w^n)^(1/n))`, `r = r0 + delta w`, `w = t w_end`:
    /// a superellipse with centre `(x0 + ax, r0)`, semi-axes `ax` and `delta`,
    /// vertical tangent at `t = 0`.
    Lip {
        x0: f64,
        r0: f64,
        ax: f64,
        delta: f64,
        n: f64,
        w_end: f64,
    },
    /// `x = x0 + len t`, `r = sum c[k] t^k`.
    Poly { x0: f64, len: f64, c: [f64; 6] },
}

/// Breakpoints in `t` for quadrature of a lip (the integrand is mildly
/// non-analytic at the highlight for exponents other than two).
const LIP_PANELS: [f64; 4] = [0.0, 0.1, 0.4, 1.0];
const POLY_PANELS: [f64; 2] = [0.0, 1.0];

impl Piece {
    /// Quintic Hermite piece from `(r0, m0, c0)` at `x0` to `(r1, m1, c1)` at
    /// `x0 + len`: radius, slope `dr/dx` and curvature `d2r/dx2` at each end.
    #[allow(clippy::too_many_arguments)]
    pub fn hermite(
        x0: f64,
        len: f64,
        r0: f64,
        m0: f64,
        c0: f64,
        r1: f64,
        m1: f64,
        c1: f64,
    ) -> Self {
        let v0 = m0 * len;
        let a0 = c0 * len * len;
        let v1 = m1 * len;
        let a1 = c1 * len * len;
        let d = r1 - r0 - v0 - 0.5 * a0;
        let e = v1 - v0 - a0;
        let f = a1 - a0;
        Piece::Poly {
            x0,
            len,
            c: [
                r0,
                v0,
                0.5 * a0,
                10.0 * d - 4.0 * e + 0.5 * f,
                -15.0 * d + 7.0 * e - f,
                6.0 * d - 3.0 * e + 0.5 * f,
            ],
        }
    }

    /// A constant-radius piece (the cylinder).
    pub fn cylinder(x0: f64, len: f64, r: f64) -> Self {
        Piece::Poly {
            x0,
            len,
            c: [r, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Whether the piece is a constant radius.
    pub fn is_cylinder(&self) -> bool {
        matches!(self, Piece::Poly { c, .. } if c[1..].iter().all(|&v| v == 0.0))
    }

    /// The point and derivatives at parameter `t`.
    pub fn eval(&self, t: f64) -> Point {
        match *self {
            Piece::Lip {
                x0,
                r0,
                ax,
                delta,
                n,
                w_end,
            } => {
                let w = (t * w_end).clamp(0.0, 1.0);
                let q = (1.0 - w.powf(n)).max(1.0e-300);
                Point {
                    x: x0 + ax * (1.0 - q.powf(1.0 / n)),
                    r: r0 + delta * w,
                    dx: ax * w.powf(n - 1.0) * q.powf(1.0 / n - 1.0) * w_end,
                    dr: delta * w_end,
                    ddx: ax * (n - 1.0) * w.powf(n - 2.0) * q.powf(1.0 / n - 2.0) * w_end * w_end,
                    ddr: 0.0,
                }
            }
            Piece::Poly { x0, len, c } => {
                let (mut r, mut d1, mut d2) = (0.0, 0.0, 0.0);
                for &ck in c.iter().rev() {
                    d2 = d2 * t + 2.0 * d1;
                    d1 = d1 * t + r;
                    r = r * t + ck;
                }
                Point {
                    x: x0 + len * t,
                    r,
                    dx: len,
                    dr: d1,
                    ddx: 0.0,
                    ddr: d2,
                }
            }
        }
    }

    /// Quadrature breakpoints in `t`.
    pub fn panels(&self) -> &'static [f64] {
        match self {
            Piece::Lip { .. } => &LIP_PANELS,
            Piece::Poly { .. } => &POLY_PANELS,
        }
    }

    /// Axial station of the start and of the end.
    pub fn x_span(&self) -> (f64, f64) {
        (self.eval(0.0).x, self.eval(1.0).x)
    }
}
