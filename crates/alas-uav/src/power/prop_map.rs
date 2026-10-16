// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propeller coefficient maps from the database (`PropPerfSet`).
//!
//! Rows are `(omega, J, CT, CP)` with `J = V / (n D)`, `n = omega / 2 pi`,
//! `T = CT rho n^2 D^4`, `P = CP rho n^3 D^5`.  They are grouped into constant
//! `omega` curves; a lookup interpolates linearly in `J` inside each curve that
//! covers the requested `J` and then linearly in `omega` between the bracketing
//! curves.  There is NO extrapolation: a point outside the data returns `None`
//! (the repository behaviour kept from `PropellerPerformanceMap`).  The
//! Reynolds column is not used for a correction (documented limitation: the
//! plan's Re correction needs Re-resolved UIUC data, not implemented here).

use alas_uav_db::PropPerfSet;

use super::{finite_positive, PowerError};

#[derive(Debug, Clone, PartialEq)]
struct Curve {
    omega: f64,
    j: Vec<f64>,
    ct: Vec<f64>,
    cp: Vec<f64>,
}

impl Curve {
    fn at(&self, j: f64) -> Option<(f64, f64)> {
        let (first, last) = (*self.j.first()?, *self.j.last()?);
        if j < first || j > last {
            return None;
        }
        if self.j.len() == 1 {
            return Some((self.ct[0], self.cp[0]));
        }
        let i = self
            .j
            .partition_point(|&x| x <= j)
            .saturating_sub(1)
            .min(self.j.len() - 2);
        let t = (j - self.j[i]) / (self.j[i + 1] - self.j[i]);
        Some((
            self.ct[i] + t * (self.ct[i + 1] - self.ct[i]),
            self.cp[i] + t * (self.cp[i + 1] - self.cp[i]),
        ))
    }
}

/// One constant-speed curve: `(omega, [(J, CT, CP)])`.
pub type MapCurve = (f64, Vec<(f64, f64, f64)>);

/// A `(omega, J) -> (CT, CP)` table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropMapTable {
    diameter_m: f64,
    curves: Vec<Curve>,
}

impl PropMapTable {
    /// Build from a database map of a propeller of diameter `diameter_m`.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] when the map is empty or malformed.
    pub fn from_set(set: &PropPerfSet, diameter_m: f64) -> Result<Self, PowerError> {
        finite_positive(diameter_m, "propeller diameter")?;
        set.validate()
            .map_err(|e| PowerError::InvalidInput(e.to_string()))?;
        let mut rows: Vec<_> = set.rows.iter().collect();
        rows.sort_by(|a, b| {
            a.omega_rad_s
                .total_cmp(&b.omega_rad_s)
                .then(a.j.total_cmp(&b.j))
        });
        let mut curves: Vec<Curve> = Vec::new();
        for row in rows {
            let same = curves
                .last()
                .is_some_and(|c| (row.omega_rad_s - c.omega).abs() <= 1.0e-4 * c.omega);
            if !same {
                curves.push(Curve {
                    omega: row.omega_rad_s,
                    j: Vec::new(),
                    ct: Vec::new(),
                    cp: Vec::new(),
                });
            }
            if let Some(c) = curves.last_mut() {
                if c.j.last().is_some_and(|&last| row.j <= last) {
                    continue; // duplicate or unordered J within a curve: keep the first
                }
                c.j.push(row.j);
                c.ct.push(row.ct);
                c.cp.push(row.cp);
            }
        }
        Ok(Self { diameter_m, curves })
    }

    /// Build from computed curves `(omega, [(J, CT, CP)])` (for example a
    /// tabulated BEMT); each curve needs strictly increasing `J` and at least
    /// two points, and the curves strictly increasing `omega`.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for unordered or non-finite data.
    pub fn from_curves(diameter_m: f64, curves: Vec<MapCurve>) -> Result<Self, PowerError> {
        finite_positive(diameter_m, "propeller diameter")?;
        let ok = !curves.is_empty()
            && curves.windows(2).all(|w| w[1].0 > w[0].0)
            && curves.iter().all(|(w, rows)| {
                w.is_finite()
                    && *w > 0.0
                    && rows.len() >= 2
                    && rows.windows(2).all(|r| r[1].0 > r[0].0)
                    && rows
                        .iter()
                        .all(|r| r.0.is_finite() && r.1.is_finite() && r.2.is_finite())
            });
        if !ok {
            return Err(PowerError::InvalidInput(
                "propeller curves must be non-empty, finite and ordered in omega and J".into(),
            ));
        }
        let curves = curves
            .into_iter()
            .map(|(omega, rows)| Curve {
                omega,
                j: rows.iter().map(|r| r.0).collect(),
                ct: rows.iter().map(|r| r.1).collect(),
                cp: rows.iter().map(|r| r.2).collect(),
            })
            .collect();
        Ok(Self { diameter_m, curves })
    }

    /// Propeller diameter, m.
    pub fn diameter_m(&self) -> f64 {
        self.diameter_m
    }

    /// Lowest and highest tabulated shaft speed, rad/s.
    pub fn omega_range(&self) -> (f64, f64) {
        (
            self.curves[0].omega,
            self.curves[self.curves.len() - 1].omega,
        )
    }

    /// `(CT, CP)` at shaft speed `omega` (rad/s) and advance ratio `j`, or
    /// `None` outside the tabulated data.
    pub fn coefficients(&self, omega: f64, j: f64) -> Option<(f64, f64)> {
        if !(omega.is_finite() && j.is_finite() && j >= 0.0) {
            return None;
        }
        let avail: Vec<(f64, (f64, f64))> = self
            .curves
            .iter()
            .filter_map(|c| c.at(j).map(|v| (c.omega, v)))
            .collect();
        let (lo, hi) = (avail.first()?, avail.last()?);
        if omega < lo.0 || omega > hi.0 {
            return None;
        }
        if avail.len() == 1 {
            return ((omega - lo.0).abs() <= 1.0e-9 * lo.0).then_some(lo.1);
        }
        let i = avail
            .partition_point(|a| a.0 <= omega)
            .saturating_sub(1)
            .min(avail.len() - 2);
        let (a, b) = (avail[i], avail[i + 1]);
        let t = (omega - a.0) / (b.0 - a.0);
        Some((
            a.1 .0 + t * (b.1 .0 - a.1 .0),
            a.1 .1 + t * (b.1 .1 - a.1 .1),
        ))
    }
}
