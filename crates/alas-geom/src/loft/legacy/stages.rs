// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The three fits of `from_legacy`: plan line, crown and keel, exponent.

use alas_math::spline_fit::{FitConstraint, FitOutcome, FitSample};

use super::fit::{fit_curve, gap_series, lambda_for, Grown};
use super::problem::{extent_weight, pin, Problem};
use crate::loft::LoftError;

/// Station behind which the crown and keel lines are fitted directly, m.
const BLEND_X_M: f64 = 0.6;
/// Penalty weight of the stage-2 refit, relative to the data scale.
const REFIT_LAMBDA_REL: f64 = 1e-16;

/// Result of the crown and keel stages.
pub(super) struct Lines {
    pub(super) zc: FitOutcome,
    pub(super) e2: FitOutcome,
    pub(super) inserted: usize,
    pub(super) crown_gap: Vec<f64>,
    pub(super) keel_gap: Vec<f64>,
}

impl Problem<'_> {
    /// The plan line, fitted as `A = a^2`.
    pub(super) fn plan_stage(&self) -> Result<Grown, LoftError> {
        let samples = self.samples(
            |i, _| self.plan_t[i] * self.plan_t[i],
            |_, s| extent_weight(s.a),
        );
        let pins = self.extent_pins(self.r_front.0, (0.5 * self.config.diameter_m).powi(2));
        let lam = lambda_for(self.options.lambda_rel, &samples);
        self.engine(false).grow(
            self.initial.clone(),
            &|knots, scale| {
                Ok(vec![fit_curve(
                    "A",
                    self.degree,
                    knots,
                    &samples,
                    &pins,
                    lam * scale,
                )?])
            },
            &|c| {
                let fitted: Vec<f64> = self
                    .us
                    .iter()
                    .map(|&u| c[0].spline.eval(u).max(0.0).sqrt())
                    .collect();
                gap_series(&self.xs, &self.plan, &fitted)
            },
            self.options.tolerance_m,
            self.options.max_deviation_m,
        )
    }

    /// The exponent: every coefficient raised to at least 2 later keeps
    /// `n >= 2` (convex hull).
    pub(super) fn exponent_stage(&self) -> Result<Grown, LoftError> {
        let samples = self.samples(|_, s| s.n, |_, _| 1.0);
        let mut pins = vec![pin(0.0, 0, 2.0)];
        if self.point_tail {
            pins.push(pin(1.0, 0, 2.0));
        }
        pins.extend(self.cabin_pins(2.0));
        let lam = lambda_for(self.options.lambda_rel, &samples);
        self.engine(true).grow(
            self.initial.clone(),
            &|knots, scale| {
                Ok(vec![fit_curve(
                    "n",
                    self.degree,
                    knots,
                    &samples,
                    &pins,
                    lam * scale,
                )?])
            },
            &|c| {
                self.us
                    .iter()
                    .zip(&self.n_meas)
                    .map(|(&u, m)| (c[0].spline.eval(u) - m).abs())
                    .collect()
            },
            0.05,
            0.15,
        )
    }

    /// Crown and keel, then the stored `zc` and `E`.
    ///
    /// Away from the nose the physical lines are smooth splines, so each is
    /// fitted directly with its own penalty scale (stage 1): `zc` and `e` are
    /// then their half sum and half difference, and the crown cannot ring
    /// through an imperfect cancellation of two separately fitted curves.
    /// Stage 2 converts to the stored `zc` and `E = e^2` by an ordinary refit
    /// on a denser knot vector; ahead of the blend station the targets are the
    /// (smoothed) law itself, because there `e` has a square-root behaviour at
    /// the pole that only `E = e^2` represents.
    pub(super) fn lines_stage(&self) -> Result<Lines, LoftError> {
        let blend_x = BLEND_X_M.min(0.5 * self.x_nose);
        let cfg = self.config;
        let (crown_cabin, keel_cabin) = (
            cfg.cabin_z_m + 0.5 * cfg.effective_height_m(),
            cfg.cabin_z_m - 0.5 * cfg.effective_height_m(),
        );
        let line_samples = |y: &[f64]| -> Vec<FitSample> {
            self.samples(
                |i, _| y[i],
                |_, s| {
                    if s.u * self.length >= 0.8 * blend_x {
                        1.0
                    } else {
                        0.0
                    }
                },
            )
        };
        let (crown_samples, keel_samples) =
            (line_samples(&self.crown_t), line_samples(&self.keel_t));
        let body: Vec<bool> = self.xs.iter().map(|&x| x >= blend_x).collect();
        let engine = self.engine(false);
        let grow_line = |name: &str,
                         samples: &[FitSample],
                         pins: &[FitConstraint],
                         meas: &[f64]|
         -> Result<Grown, LoftError> {
            let lam = lambda_for(self.options.lambda_rel, samples);
            engine.grow(
                self.initial.clone(),
                &|knots, scale| {
                    Ok(vec![fit_curve(
                        name,
                        self.degree,
                        knots,
                        samples,
                        pins,
                        lam * scale,
                    )?])
                },
                &|c| {
                    let fitted: Vec<f64> = self.us.iter().map(|&u| c[0].spline.eval(u)).collect();
                    let mut gaps = gap_series(&self.xs, meas, &fitted);
                    for (g, inside) in gaps.iter_mut().zip(&body) {
                        if !inside {
                            *g = 0.0;
                        }
                    }
                    gaps
                },
                self.options.tolerance_m,
                self.options.max_deviation_m,
            )
        };
        // Each line has its own penalty scale: the crown is nearly straight
        // and can be smoothed far harder than the keel, whose belly curves.
        let mut crown_grown = grow_line(
            "crown",
            &crown_samples,
            &self.cabin_pins(crown_cabin),
            &self.crown,
        )?;
        let mut keel_grown = grow_line(
            "keel",
            &keel_samples,
            &self.cabin_pins(keel_cabin),
            &self.keel,
        )?;
        let (crown_line, keel_line) = (crown_grown.curves.remove(0), keel_grown.curves.remove(0));

        // Stage 2 targets: the law at the nose, the smooth lines behind it.
        let blend = |x: f64| {
            let t = ((x - blend_x) / blend_x).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let (mut zc_targets, mut e_targets) = (Vec::new(), Vec::new());
        for (i, s) in self.data.iter().enumerate() {
            let t = blend(s.u * self.length);
            let (c, k) = (crown_line.spline.eval(s.u), keel_line.spline.eval(s.u));
            let (law_zc, law_e) = (
                0.5 * (self.crown_t[i] + self.keel_t[i]),
                0.5 * (self.crown_t[i] - self.keel_t[i]),
            );
            zc_targets.push((1.0 - t) * law_zc + t * 0.5 * (c + k));
            e_targets.push((1.0 - t) * law_e + t * 0.5 * (c - k).max(0.0));
        }
        let zc2 = self.samples(|i, _| zc_targets[i], |_, _| 1.0);
        let e2 = self.samples(
            |i, _| e_targets[i] * e_targets[i],
            |i, _| extent_weight(e_targets[i]),
        );
        let dense = self.dense_knots(&[&crown_line, &keel_line], blend_x);
        let mut zc_pins = vec![pin(0.0, 0, self.data[0].zc)];
        if self.point_tail {
            zc_pins.push(pin(1.0, 0, self.data[self.data.len() - 1].zc));
        }
        zc_pins.extend(self.cabin_pins(cfg.cabin_z_m));
        let e_pins = self.extent_pins(self.r_front.1, (0.5 * cfg.effective_height_m()).powi(2));
        let zc = fit_curve(
            "zc",
            self.degree,
            &dense,
            &zc2,
            &zc_pins,
            lambda_for(REFIT_LAMBDA_REL, &zc2),
        )?;
        let e2 = fit_curve(
            "E",
            self.degree,
            &dense,
            &e2,
            &e_pins,
            lambda_for(REFIT_LAMBDA_REL, &e2),
        )?;
        let (mut top, mut bottom) = (Vec::new(), Vec::new());
        for &u in &self.us {
            let e = e2.spline.eval(u).max(0.0).sqrt();
            let z = zc.spline.eval(u);
            top.push(z + e);
            bottom.push(z - e);
        }
        Ok(Lines {
            crown_gap: gap_series(&self.xs, &self.crown, &top),
            keel_gap: gap_series(&self.xs, &self.keel, &bottom),
            inserted: crown_grown.inserted + keel_grown.inserted,
            zc,
            e2,
        })
    }

    /// Knots for the stage-2 refit: those of both stage-1 lines, their span
    /// midpoints (never inside the pinned cabin), and a dense ladder through
    /// the pole zone and the blend, with multiplicity capped at the feature
    /// multiplicity.
    fn dense_knots(&self, lines: &[&FitOutcome], blend_x: f64) -> Vec<f64> {
        let keep_clear = self.keep_clear();
        let degree = self.degree;
        let mut dense: Vec<f64> = Vec::new();
        for curve in lines {
            let k = curve.spline.knots();
            dense.extend_from_slice(&k[degree + 1..k.len() - degree - 1]);
            for w in curve.spline.breakpoints().windows(2) {
                let mid = 0.5 * (w[0] + w[1]);
                if keep_clear.is_none_or(|(lo, hi)| mid <= lo || mid >= hi) {
                    dense.push(mid);
                }
            }
        }
        dense.sort_by(f64::total_cmp);
        let mut counted: Vec<f64> = Vec::new();
        for &u in &dense {
            let same = counted.iter().filter(|&&c| (c - u).abs() < 1e-12).count();
            if same < self.multiplicity {
                counted.push(u);
            }
        }
        for x in [
            0.05, 0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.05, 1.2, 1.4, 1.6, 1.8, 2.0, 2.3, 2.6, 3.0,
        ] {
            let u = x / self.length;
            if x < 6.0 * blend_x && x < self.x_nose && counted.iter().all(|d| (d - u).abs() > 1e-4)
            {
                counted.push(u);
            }
        }
        counted.sort_by(f64::total_cmp);
        counted
    }
}
