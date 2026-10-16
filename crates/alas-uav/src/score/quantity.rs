// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dimensioned scalars and time series for the score expression language.
//!
//! Values are stored in SI. A dimension is the exponent vector over the base
//! quantities m, kg, s, A, K and EUR (cost). Exponents are stored doubled so
//! that `sqrt` of an integer-exponent quantity is representable (the XC
//! loading-speed term uses `sqrt(t)`); `pow` accepts any exponent that leaves
//! every stored exponent integral. Angles and counts are dimensionless.

use std::fmt;

/// Number of base dimensions: m, kg, s, A, K, EUR.
const NDIM: usize = 6;
const SYMBOLS: [&str; NDIM] = ["m", "kg", "s", "A", "K", "EUR"];

/// Exponent vector over the base quantities (stored as twice the exponent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dim([i16; NDIM]);

impl Dim {
    /// Dimensionless.
    pub const ONE: Dim = Dim([0; NDIM]);
    /// Length, m.
    pub const LENGTH: Dim = Dim([2, 0, 0, 0, 0, 0]);
    /// Mass, kg.
    pub const MASS: Dim = Dim([0, 2, 0, 0, 0, 0]);
    /// Time, s.
    pub const TIME: Dim = Dim([0, 0, 2, 0, 0, 0]);
    /// Electric current, A.
    pub const CURRENT: Dim = Dim([0, 0, 0, 2, 0, 0]);
    /// Temperature, K.
    pub const TEMPERATURE: Dim = Dim([0, 0, 0, 0, 2, 0]);
    /// Cost, EUR.
    pub const COST: Dim = Dim([0, 0, 0, 0, 0, 2]);

    /// Product of two dimensions.
    pub fn times(self, other: Dim) -> Dim {
        let mut out = self.0;
        for (o, b) in out.iter_mut().zip(other.0) {
            *o += b;
        }
        Dim(out)
    }

    /// Quotient of two dimensions.
    pub fn over(self, other: Dim) -> Dim {
        let mut out = self.0;
        for (o, b) in out.iter_mut().zip(other.0) {
            *o -= b;
        }
        Dim(out)
    }

    /// Integer power.
    pub fn powi(self, n: i16) -> Dim {
        let mut out = self.0;
        for o in &mut out {
            *o *= n;
        }
        Dim(out)
    }

    /// Real power; `None` when a stored exponent would not stay integral.
    pub fn powf(self, p: f64) -> Option<Dim> {
        let mut out = [0_i16; NDIM];
        for (o, a) in out.iter_mut().zip(self.0) {
            let scaled = f64::from(a) * p;
            let rounded = scaled.round();
            if (scaled - rounded).abs() > 1e-9 || rounded.abs() > 60.0 {
                return None;
            }
            *o = rounded as i16;
        }
        Some(Dim(out))
    }

    /// Square root; `None` for a quarter-integer result.
    pub fn sqrt(self) -> Option<Dim> {
        self.powf(0.5)
    }

    /// Dimensionless?
    pub fn is_one(self) -> bool {
        self == Dim::ONE
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        for (sym, e2) in SYMBOLS.iter().zip(self.0) {
            match e2 {
                0 => {}
                2 => parts.push((*sym).to_string()),
                _ if e2 % 2 == 0 => parts.push(format!("{sym}^{}", e2 / 2)),
                _ => parts.push(format!("{sym}^{}", f64::from(e2) / 2.0)),
            }
        }
        if parts.is_empty() {
            write!(f, "1")
        } else {
            write!(f, "{}", parts.join("*"))
        }
    }
}

/// A scalar in SI with its dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    /// Value in SI base units.
    pub value: f64,
    /// Dimension.
    pub dim: Dim,
}

impl Quantity {
    /// A dimensionless number.
    pub fn number(value: f64) -> Self {
        Self {
            value,
            dim: Dim::ONE,
        }
    }

    /// A quantity of the given dimension.
    pub fn new(value: f64, dim: Dim) -> Self {
        Self { value, dim }
    }
}

/// A piecewise-linear signal sampled in time (seconds) with a dimension.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Sample times, s, non-decreasing.
    pub t: Vec<f64>,
    /// Values in SI.
    pub y: Vec<f64>,
    /// Dimension of `y`.
    pub dim: Dim,
}

impl Series {
    /// Integral over time of `max(0, y - limit)` with exact handling of the
    /// crossings of the piecewise-linear signal; dimension of `y` times s.
    pub fn integral_above(&self, limit: f64) -> f64 {
        let mut total = 0.0;
        for k in 1..self.t.len() {
            let dt = self.t[k] - self.t[k - 1];
            if dt <= 0.0 {
                continue;
            }
            let a = self.y[k - 1] - limit;
            let b = self.y[k] - limit;
            total += if a >= 0.0 && b >= 0.0 {
                0.5 * (a + b) * dt
            } else if a > 0.0 {
                0.5 * a * dt * a / (a - b)
            } else if b > 0.0 {
                0.5 * b * dt * b / (b - a)
            } else {
                0.0
            };
        }
        total
    }

    /// Trapezoidal time integral.
    pub fn integral(&self) -> f64 {
        (1..self.t.len())
            .map(|k| 0.5 * (self.y[k] + self.y[k - 1]) * (self.t[k] - self.t[k - 1]))
            .sum()
    }

    /// Largest sample.
    pub fn max(&self) -> f64 {
        self.y.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    /// Time-weighted mean (the sample mean for a zero-length series).
    pub fn mean(&self) -> f64 {
        let span = match (self.t.first(), self.t.last()) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        };
        if span > 0.0 {
            self.integral() / span
        } else {
            self.y.iter().sum::<f64>() / self.y.len().max(1) as f64
        }
    }

    /// Non-overlapping block means over `window` seconds, each held constant
    /// over its block (the ACC measurement box: 10 Hz samples averaged every
    /// 0.5 s, then compared with the limit).
    pub fn block_mean(&self, window: f64) -> Series {
        let (Some(&t0), Some(&t1)) = (self.t.first(), self.t.last()) else {
            return self.clone();
        };
        if window <= 0.0 || !window.is_finite() || t1 <= t0 {
            return self.clone();
        }
        let mut t = Vec::new();
        let mut y = Vec::new();
        let mut start = t0;
        while start < t1 - 1e-12 {
            let end = (start + window).min(t1);
            let m = self.mean_over(start, end);
            t.extend([start, end]);
            y.extend([m, m]);
            start = end;
        }
        Series {
            t,
            y,
            dim: self.dim,
        }
    }

    /// Mean of the piecewise-linear signal over `[a, b]`.
    fn mean_over(&self, a: f64, b: f64) -> f64 {
        // Clip every sampled segment to [a, b]; segments of zero length
        // (a step written as two samples at one time) add no area, so a block
        // edge on a step is handled exactly.
        let mut area = 0.0;
        for k in 1..self.t.len() {
            let (t0, t1) = (self.t[k - 1], self.t[k]);
            let (lo, hi) = (t0.max(a), t1.min(b));
            if t1 <= t0 || hi <= lo {
                continue;
            }
            let slope = (self.y[k] - self.y[k - 1]) / (t1 - t0);
            let (y_lo, y_hi) = (
                self.y[k - 1] + slope * (lo - t0),
                self.y[k - 1] + slope * (hi - t0),
            );
            area += 0.5 * (y_lo + y_hi) * (hi - lo);
        }
        if b > a {
            area / (b - a)
        } else {
            self.value_at(a)
        }
    }

    /// Linear interpolation, held constant outside the sampled range.
    pub fn value_at(&self, time: f64) -> f64 {
        let n = self.t.len();
        if n == 0 {
            return 0.0;
        }
        if time <= self.t[0] {
            return self.y[0];
        }
        if time >= self.t[n - 1] {
            return self.y[n - 1];
        }
        let k = self.t.partition_point(|&t| t <= time);
        let (t0, t1) = (self.t[k - 1], self.t[k]);
        if t1 <= t0 {
            return self.y[k];
        }
        self.y[k - 1] + (self.y[k] - self.y[k - 1]) * (time - t0) / (t1 - t0)
    }
}
