// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact wetted area and volume of a [`NacelleLoft`].
//!
//! The skin is `P(t, th) = (x(t), a(t) cos th, zc(t) + e(t) sin th)` with
//! `a = r`, `e = ellipticity r`, `zc = -droop r / R`. Its area element is
//! `|P_t x P_th|`, integrated by Gauss-Legendre (24 points per panel in `t`,
//! 12 per quarter turn in `th`). The integrand is analytic in `th` and in `t`
//! on each panel, so this is spectrally accurate; a circular section reduces
//! to `2 pi r sqrt(x'^2 + r'^2)` exactly. The volume of the solid of each body
//! is `integral pi e a dx`.

use std::f64::consts::{FRAC_PI_2, PI};

use alas_math::gauss::cached_rule;

use super::nacelle::{NacelleIntegrals, NacelleLoft};
use super::nacelle_pieces::Piece;

impl NacelleLoft {
    /// Lateral wetted area of the outer skin of all bodies, m2. Excludes the
    /// inlet duct, the trailing-edge ring, the fan-exhaust annulus and the
    /// plug-tip disc (see the corresponding accessors).
    #[must_use]
    pub fn wetted_area_m2(&self) -> f64 {
        self.integrals().wetted_m2
    }

    /// Volume enclosed by the outer skin of all bodies (each closed by flat
    /// discs), m3.
    #[must_use]
    pub fn volume_m3(&self) -> f64 {
        self.integrals().volume_m3
    }

    fn integrals(&self) -> NacelleIntegrals {
        *self.integrals.get_or_init(|| {
            let eps = self.shape.ellipticity;
            let droop_per_r = self.shape.droop_m / self.max_radius_m;
            let mut wetted = 0.0;
            let mut volume = 0.0;
            for body in &self.bodies {
                for piece in &body.pieces {
                    wetted += piece_area(piece, eps, droop_per_r);
                    volume += piece_volume(piece, eps);
                }
            }
            NacelleIntegrals {
                wetted_m2: wetted,
                volume_m3: volume,
            }
        })
    }
}

fn piece_area(piece: &Piece, eps: f64, droop_per_r: f64) -> f64 {
    let (Some(rt), Some(rth)) = (cached_rule(24), cached_rule(12)) else {
        return f64::NAN;
    };
    let panels = piece.panels();
    let mut total = 0.0;
    for pair in panels.windows(2) {
        for (t, wt) in rt.on_interval(pair[0], pair[1]) {
            let p = piece.eval(t);
            let (a, da) = (p.r, p.dr);
            let (e, de) = (eps * p.r, eps * p.dr);
            let dzc = -droop_per_r * p.dr;
            let mut ring = 0.0;
            for quarter in 0..4 {
                let lo = quarter as f64 * FRAC_PI_2;
                for (th, wth) in rth.on_interval(lo, lo + FRAC_PI_2) {
                    let (s, c) = th.sin_cos();
                    let i = da * e * c * c + a * s * (dzc + de * s);
                    let j = -p.dx * e * c;
                    let k = -p.dx * a * s;
                    ring += wth * (i * i + j * j + k * k).sqrt();
                }
            }
            total += wt * ring;
        }
    }
    total
}

fn piece_volume(piece: &Piece, eps: f64) -> f64 {
    let Some(rt) = cached_rule(24) else {
        return f64::NAN;
    };
    let mut total = 0.0;
    for pair in piece.panels().windows(2) {
        total += rt.integrate(pair[0], pair[1], |t| {
            let p = piece.eval(t);
            PI * eps * p.r * p.r * p.dx
        });
    }
    total
}
