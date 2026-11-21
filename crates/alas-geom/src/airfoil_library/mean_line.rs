// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The NACA `a` mean line and the cambered 64A-series section built on it.
//!
//! The mean line is the loading `a` family of Abbott and von Doenhoff, Theory of
//! Wing Sections (1959), section 6.6: uniform chordwise loading to `x = a`, then
//! falling linearly to zero at the trailing edge, scaled to the design lift
//! coefficient `cl_i`. The section is the thickness form combined with the mean line
//! normal to the line, as in the book's section 6.4. The modified shape of the
//! `a = 0.8 (modified)` line is not modelled: the ordinates here are the unmodified
//! `a` family.
//!
//! Frame: `x` is chord fraction from the leading edge, `y` chord fractions up.

use std::f64::consts::PI;

use super::{morph_airfoil, normalize_coordinates, DEFAULT_MORPH_N_POINTS};
use crate::aircraft::airfoil::Airfoil;
use crate::selig;

/// `t ln t`, zero at `t = 0`.
fn t_ln_t(t: f64) -> f64 {
    if t <= 0.0 {
        0.0
    } else {
        t * t.ln()
    }
}

/// `t^2 ln |t|`, zero at `t = 0`.
fn t2_ln_t(t: f64) -> f64 {
    if t == 0.0 {
        0.0
    } else {
        t * t * t.abs().ln()
    }
}

fn constants(a: f64) -> (f64, f64) {
    let g = -1.0 / (1.0 - a) * (a * a * (0.5 * a.ln() - 0.25) + 0.25);
    let h =
        1.0 / (1.0 - a) * (0.5 * (1.0 - a).powi(2) * (1.0 - a).ln() - 0.25 * (1.0 - a).powi(2)) + g;
    (g, h)
}

/// Mean-line ordinate and slope `(y, dy/dx)` at chord fraction `x` for loading `a`
/// (`0 <= a < 1`) and design lift coefficient `cl_i`.
pub fn mean_line(a: f64, cl_i: f64, x: f64) -> (f64, f64) {
    let (g, h) = constants(a);
    let k = cl_i / (2.0 * PI * (a + 1.0));
    let y = k
        * (1.0 / (1.0 - a)
            * (0.5 * t2_ln_t(a - x) - 0.5 * t2_ln_t(1.0 - x) + 0.25 * (1.0 - x).powi(2)
                - 0.25 * (a - x).powi(2))
            - t_ln_t(x)
            + g
            - h * x);
    let ln_x = if x > 0.0 { x.ln() } else { 0.0 };
    let slope = k
        * (1.0 / (1.0 - a)
            * (-(a - x) * (a - x).abs().max(1e-300).ln() * f64::from(u8::from(x != a))
                + (1.0 - x) * (1.0 - x).max(1e-300).ln() * f64::from(u8::from(x != 1.0)))
            - ln_x
            - 1.0
            - h);
    (y, slope)
}

/// The NACA 64A-series section of `thickness_fraction` thickness (the 64A010 form
/// scaled) on the `a` mean line of design lift `cl_i`, `None` if the thickness form
/// is not in the corpus.
pub fn cambered_64a(thickness_fraction: f64, a: f64, cl_i: f64) -> Option<Airfoil> {
    let (_, base) = selig::get("naca64a010")?;
    let symmetric = morph_airfoil(
        &normalize_coordinates(base),
        thickness_fraction / 0.10,
        1.0,
        DEFAULT_MORPH_N_POINTS,
    );
    let coords = &symmetric.coordinates;
    let le = coords
        .iter()
        .enumerate()
        .min_by(|p, q| p.1 .0.total_cmp(&q.1 .0))
        .map_or(0, |(i, _)| i);
    let place = |&(x, y_half): &(f64, f64), upper: bool| {
        let (yc, slope) = mean_line(a, cl_i, x.clamp(0.0, 1.0));
        let theta = slope.atan();
        let t = y_half.abs();
        if upper {
            (x - t * theta.sin(), yc + t * theta.cos())
        } else {
            (x + t * theta.sin(), yc - t * theta.cos())
        }
    };
    let mut out: Vec<(f64, f64)> = coords[..=le].iter().map(|p| place(p, true)).collect();
    out.extend(coords[le + 1..].iter().map(|p| place(p, false)));
    Some(Airfoil::from_coordinates(
        "naca64a109",
        normalize_coordinates(&out),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mean_line_closes_at_both_ends_and_slopes_match_the_ordinates() {
        for a in [0.2, 0.5, 0.8] {
            let (y0, _) = mean_line(a, 1.0, 0.0);
            let (y1, _) = mean_line(a, 1.0, 1.0);
            assert!(y0.abs() < 1e-12 && y1.abs() < 1e-12, "a {a}: {y0} {y1}");
            for x in [0.1, 0.3, 0.6, 0.9] {
                let h = 1e-6;
                let fd = (mean_line(a, 1.0, x + h).0 - mean_line(a, 1.0, x - h).0) / (2.0 * h);
                assert!((fd - mean_line(a, 1.0, x).1).abs() < 1e-5, "a {a} x {x}");
            }
        }
    }

    #[test]
    fn the_ideal_loading_gives_the_design_lift_coefficient() {
        // Thin-airfoil theory: at the ideal angle the lift is 2 * integral of
        // (dy/dx) cos(theta) dtheta over 0..pi, with x = (1 - cos theta) / 2.
        for a in [0.8, 0.5] {
            let n = 20000;
            let mut sum = 0.0;
            for i in 0..n {
                let th = (f64::from(i) + 0.5) * PI / f64::from(n);
                let x = 0.5 * (1.0 - th.cos());
                sum += mean_line(a, 1.0, x).1 * th.cos() * PI / f64::from(n);
            }
            assert!(
                (2.0 * sum - 1.0).abs() < 2e-3 || (2.0 * sum + 1.0).abs() < 2e-3,
                "a {a}: {}",
                2.0 * sum
            );
        }
    }
}
