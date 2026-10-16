// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact (to quadrature accuracy) wetted area and volume of a [`Loft`].
//!
//! The lateral surface is `r(x, u) = (x, y, z)` with the chart parameter `u`
//! of [`Section::chart_point`] held fixed along `x`, so the skin element
//! `|r_x x r_u| du dx` carries the axial slope the trapezoid-of-perimeters
//! formula omits. Per knot span of the curves the integrand is smooth, so
//! composite Gauss-Legendre is used: an outer rule in `x`, and an inner rule
//! in `s` over each of the four chart pieces (`u = u_max(x) s^2`). On a span
//! that starts or ends at a point closure the outer abscissa is
//! `x = x_tip +- w tau^2`, which removes the `sqrt(x)` behaviour of the
//! half-extents at a paraboloid tip.

use alas_config::loft::{residual_basis, Closure};
use alas_math::gauss::cached_rule;

use super::body::area_of;
use super::curves::Jets;
use super::section::{Chart, Section};
use super::{Integrals, Loft};

/// Outer (axial) rule order of the production evaluation.
const OUTER_NODES: usize = 12;
/// Inner (circumferential, per piece) rule order of the production evaluation.
const INNER_NODES: usize = 24;
/// Rule order of the volume integral per span.
const VOLUME_NODES: usize = 24;
/// A half-extent below this is treated as zero when forming slopes, m.
const MIN_EXTENT: f64 = 1e-14;

/// How the outer abscissa maps onto a span.
#[derive(Clone, Copy)]
enum Map {
    Plain,
    FromStart,
    FromEnd,
}

impl Loft {
    /// Exact lateral wetted area of the outer skin in m^2, excluding the flat
    /// end faces of base closures (see [`Loft::base_area`]).
    #[must_use]
    pub fn wetted_area(&self) -> f64 {
        self.integrals().wetted_m2
    }

    /// Enclosed volume in m^3, exact for the section family.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.integrals().volume_m3
    }

    fn integrals(&self) -> Integrals {
        *self.integrals_cell().get_or_init(|| Integrals {
            wetted_m2: self.lateral_area_with(OUTER_NODES, INNER_NODES),
            volume_m3: self.volume_with(VOLUME_NODES),
        })
    }

    /// Spans of the curves in built local `x`, each with the abscissa map
    /// (or two maps when a short span touches both point closures).
    fn spans(&self) -> Vec<(f64, f64, Map)> {
        let points = self.curves.breakpoints();
        let length = self.curves.length();
        let tol = 1e-12 * length;
        let front = matches!(self.spec.front, Closure::Point { .. });
        let rear = matches!(self.spec.rear, Closure::Point { .. });
        let mut out = Vec::new();
        for w in points.windows(2) {
            let (x0, x1) = (w[0], w[1]);
            if x1 - x0 <= tol {
                continue;
            }
            let at_front = front && x0 <= tol;
            let at_rear = rear && x1 >= length - tol;
            match (at_front, at_rear) {
                (true, true) => {
                    let mid = 0.5 * (x0 + x1);
                    out.push((x0, mid, Map::FromStart));
                    out.push((mid, x1, Map::FromEnd));
                }
                (true, false) => out.push((x0, x1, Map::FromStart)),
                (false, true) => out.push((x0, x1, Map::FromEnd)),
                (false, false) => out.push((x0, x1, Map::Plain)),
            }
        }
        out
    }

    /// Lateral area with the given cached outer and inner rule orders.
    pub(super) fn lateral_area_with(&self, outer: usize, inner: usize) -> f64 {
        let (Some(outer_rule), Some(inner_rule)) = (cached_rule(outer), cached_rule(inner)) else {
            return f64::NAN;
        };
        let mut total = 0.0;
        for (x0, x1, map) in self.spans() {
            let width = x1 - x0;
            for (tau, w) in outer_rule.on_interval(0.0, 1.0) {
                let (x, dx) = match map {
                    Map::Plain => (x0 + width * tau, width * w),
                    Map::FromStart => (x0 + width * tau * tau, 2.0 * width * tau * w),
                    Map::FromEnd => (x1 - width * tau * tau, 2.0 * width * tau * w),
                };
                let jets = self.curves.jets(x, None);
                total += dx * skin_density(&jets, inner_rule);
            }
        }
        total
    }

    /// Volume with the given cached rule order per span.
    pub(super) fn volume_with(&self, nodes: usize) -> f64 {
        let Some(rule) = cached_rule(nodes) else {
            return f64::NAN;
        };
        self.spans()
            .iter()
            .map(|&(x0, x1, _)| {
                rule.on_interval(x0, x1)
                    .map(|(x, w)| w * area_of(&self.curves.jets(x, None)))
                    .sum::<f64>()
            })
            .sum()
    }
}

/// Skin area per unit length at a station: both sides of the body, the four
/// chart pieces of the right half, `2 * sum |r_x x r_u| du`.
fn skin_density(j: &Jets, rule: &alas_math::gauss::GaussRule) -> f64 {
    let a = j.a2[0].max(0.0).sqrt();
    let e = j.e2[0].max(0.0).sqrt();
    let a_x = if a > MIN_EXTENT {
        j.a2[1] / (2.0 * a)
    } else {
        0.0
    };
    let e_x = if e > MIN_EXTENT {
        j.e2[1] / (2.0 * e)
    } else {
        0.0
    };
    let b = [j.b[0][0], j.b[1][0], j.b[2][0], j.b[3][0]];
    let b_x = [j.b[0][1], j.b[1][1], j.b[2][1], j.b[3][1]];
    // A residual is active where a coefficient or its slope is not zero.
    let active = b.iter().chain(&b_x).any(|v| *v != 0.0);
    let sec = Section::new(a, e, 0.0, j.n[0].max(1.0), j.k[0]).with_residual(b);
    let (n_x, k_x, zc_x) = (j.n[1], j.k[1], j.zc[1]);
    let u_max = sec.chart_limit();
    let mut total = 0.0;
    for chart in [Chart::Zeta, Chart::Eta] {
        for sigma in [1.0, -1.0] {
            total += rule.integrate(0.0, 1.0, |s| {
                let u = u_max * s * s;
                let p = sec.chart_point(chart, sigma, u);
                let zeta = p.zeta;
                let warp = sec.warp_of(zeta);
                let common = zc_x + e_x * warp + e * k_x * (1.0 - zeta * zeta);
                // `R_x = sum_j b_j' w_j(zeta)`: the axial change of the residual
                // at fixed zeta (upper arc only).
                let r_x = if active && sigma > 0.0 {
                    let w = residual_basis(zeta);
                    b_x.iter().zip(w).map(|(d, w)| d * w).sum::<f64>()
                } else {
                    0.0
                };
                let (y_x, z_x) = match chart {
                    Chart::Zeta => (
                        (a_x * p.h + a * p.h_n * n_x) * p.fac + a * p.h * r_x,
                        common,
                    ),
                    Chart::Eta => (
                        a_x * u * p.fac + a * u * (p.r_t * p.h_n * n_x + r_x),
                        common + e * (1.0 - 2.0 * sec.k * zeta) * sigma * p.h_n * n_x,
                    ),
                };
                let c0 = y_x * p.z_u - z_x * p.y_u;
                (c0 * c0 + p.z_u * p.z_u + p.y_u * p.y_u).sqrt() * 2.0 * u_max * s
            });
        }
    }
    2.0 * total
}
