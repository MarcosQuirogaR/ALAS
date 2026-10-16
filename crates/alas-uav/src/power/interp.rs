// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small numerical helpers: validated 1-D tables and a bracketed root finder.

use super::PowerError;

/// A validated `y(x)` table with strictly increasing `x`.
#[derive(Debug, Clone, PartialEq)]
pub struct Table1 {
    x: Vec<f64>,
    y: Vec<f64>,
}

impl Table1 {
    pub fn new(x: Vec<f64>, y: Vec<f64>, what: &str) -> Result<Self, PowerError> {
        let ok = x.len() == y.len()
            && !x.is_empty()
            && x.iter().chain(&y).all(|v| v.is_finite())
            && x.windows(2).all(|w| w[1] > w[0]);
        if ok {
            Ok(Self { x, y })
        } else {
            Err(PowerError::InvalidInput(format!(
                "{what}: table needs equal-length finite columns and strictly increasing x"
            )))
        }
    }

    /// Linear interpolation, clamped to the end values outside the range.
    pub fn eval_clamped(&self, q: f64) -> f64 {
        let n = self.x.len();
        if n == 1 || q <= self.x[0] {
            return self.y[0];
        }
        if q >= self.x[n - 1] {
            return self.y[n - 1];
        }
        let i = self
            .x
            .partition_point(|&v| v <= q)
            .saturating_sub(1)
            .min(n - 2);
        let t = (q - self.x[i]) / (self.x[i + 1] - self.x[i]);
        self.y[i] + t * (self.y[i + 1] - self.y[i])
    }
}

/// Root of `f` in `[a, b]` by Brent's method.  `None` unless the end values
/// bracket a sign change or `f` returns `None` (a non-evaluable point).
pub fn brent(f: &mut dyn FnMut(f64) -> Option<f64>, a: f64, b: f64, tol: f64) -> Option<f64> {
    let (mut a, mut b) = (a, b);
    let (mut fa, mut fb) = (f(a)?, f(b)?);
    if fa == 0.0 {
        return Some(a);
    }
    if fb == 0.0 {
        return Some(b);
    }
    if fa.signum() == fb.signum() {
        return None;
    }
    let (mut c, mut fc) = (a, fa);
    let (mut d, mut e) = (b - a, b - a);
    for _ in 0..200 {
        if fb.signum() == fc.signum() {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * f64::EPSILON * b.abs() + 0.5 * tol;
        let xm = 0.5 * (c - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return Some(b);
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * xm * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * xm * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            }
            p = p.abs();
            if 2.0 * p < (3.0 * xm * q - (tol1 * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol1 { d } else { tol1.copysign(xm) };
        fb = f(b)?;
    }
    Some(b)
}
