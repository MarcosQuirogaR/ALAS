// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sampled legacy laws and the exact conditions of the fit.

use alas_config::FuselageConfig;
use alas_math::spline_fit::{FitConstraint, FitSample};

use super::fit::{sample_positions, Engine, Sample};
use super::law::{estimate_tip_radii, law_at, smoothed_lines, POLE_RELAXATION, POLE_ZONE_M};
use super::{features, LegacyFitOptions};
use crate::loft::LoftError;

/// Smoothing of the fit targets starts behind this station, m.
const SMOOTH_FROM_M: f64 = 0.3;
/// Standard deviation of the smoothing of the fit targets, m.
const SMOOTH_SIGMA_M: f64 = 0.1;

pub(super) fn pin(x: f64, order: usize, value: f64) -> FitConstraint {
    FitConstraint { x, order, value }
}

/// Weight turning a residual of a squared extent into one of the extent
/// (floored near the tip).
pub(super) fn extent_weight(extent: f64) -> f64 {
    1.0 / (2.0 * extent + 0.1).powi(2)
}

/// Everything the stages of the fit read.
pub(super) struct Problem<'a> {
    pub(super) config: &'a FuselageConfig,
    pub(super) options: &'a LegacyFitOptions,
    pub(super) length: f64,
    pub(super) degree: usize,
    /// End of the nose / start of the aft body, m.
    pub(super) x_nose: f64,
    /// The same as fractions of the length.
    pub(super) ua: f64,
    pub(super) ub: f64,
    pub(super) pin_cabin: bool,
    pub(super) point_tail: bool,
    /// Triple (C2) knot multiplicity at features, 1 for a cubic.
    pub(super) multiplicity: usize,
    pub(super) xs: Vec<f64>,
    pub(super) us: Vec<f64>,
    pub(super) data: Vec<Sample>,
    /// Law outlines at the samples.
    pub(super) crown: Vec<f64>,
    pub(super) keel: Vec<f64>,
    pub(super) plan: Vec<f64>,
    pub(super) n_meas: Vec<f64>,
    /// Smoothed fit targets of the outlines.
    pub(super) crown_t: Vec<f64>,
    pub(super) keel_t: Vec<f64>,
    pub(super) plan_t: Vec<f64>,
    /// Starting knots (triple at the cabin ends and the features).
    pub(super) initial: Vec<f64>,
    pub(super) features: Vec<f64>,
    pub(super) r_front: (f64, f64),
    pub(super) r_rear: f64,
    /// Error relaxation in the pole zones, and ones for the exponent.
    pub(super) relax: Vec<f64>,
    pub(super) ones: Vec<f64>,
}

impl<'a> Problem<'a> {
    pub(super) fn new(
        config: &'a FuselageConfig,
        length: f64,
        options: &'a LegacyFitOptions,
    ) -> Result<Self, LoftError> {
        let fail = |m: &str| LoftError::Fit(m.to_owned());
        if !(length.is_finite() && length > 0.0) {
            return Err(fail("length must be positive"));
        }
        if options.degree != 3 && options.degree != 5 {
            return Err(fail("degree must be 3 or 5"));
        }
        let degree = usize::from(options.degree);
        let x_nose = config.nose_loft_length_m();
        let x_aft = config.aft_body_start_m(length);
        if !(x_nose > 0.0 && x_aft > x_nose && x_aft < length) {
            return Err(fail("the nose, cabin and aft body do not fit the length"));
        }
        let base_tail = config
            .tail_end_width_m
            .is_some_and(|w| w.is_finite() && w > 0.0)
            && config
                .tail_end_height_m
                .is_some_and(|h| h.is_finite() && h > 0.0);
        let point_tail = !base_tail;

        let xs = sample_positions(length, options.samples, &[x_nose, x_aft]);
        let data: Vec<Sample> = xs
            .iter()
            .map(|&x| {
                let (a, e, zc, n) = law_at(config, x, length);
                Sample {
                    u: x / length,
                    a,
                    e,
                    zc,
                    n,
                }
            })
            .collect();
        let us: Vec<f64> = data.iter().map(|s| s.u).collect();
        let crown: Vec<f64> = data.iter().map(|s| s.zc + s.e).collect();
        let keel: Vec<f64> = data.iter().map(|s| s.zc - s.e).collect();
        let plan: Vec<f64> = data.iter().map(|s| s.a).collect();
        let n_meas: Vec<f64> = data.iter().map(|s| s.n).collect();

        // Smoothed targets: the law convolved with a Gaussian behind the pole
        // zone, so a curvature step of the law becomes a transition the spline
        // follows without overshooting (a few millimetres; the errors are still
        // measured against the unsmoothed law).
        let (mut crown_t, mut keel_t, mut plan_t) = (Vec::new(), Vec::new(), Vec::new());
        for (&x, s) in xs.iter().zip(&data) {
            let (c, k, p) = if x >= SMOOTH_FROM_M {
                smoothed_lines(config, length, x, SMOOTH_SIGMA_M)
            } else {
                (s.zc + s.e, s.zc - s.e, s.a)
            };
            crown_t.push(c);
            keel_t.push(k);
            plan_t.push(p);
        }

        // Features of the law and the starting knots.
        let (ua, ub) = (x_nose / length, x_aft / length);
        let grid = 3000usize;
        let pitch = length / grid as f64;
        let mut lines: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for i in 0..=grid {
            let (a, e, zc, _) = law_at(config, pitch * i as f64, length);
            lines[0].push(zc + e);
            lines[1].push(zc - e);
            lines[2].push(a);
        }
        let features = features::detect(&lines, pitch, &[x_nose, x_aft]);
        let multiplicity = if degree >= 5 { 3 } else { 1 };
        let mut initial: Vec<f64> = Vec::new();
        for u in [ua, ub]
            .into_iter()
            .chain(features.iter().map(|x| x / length))
        {
            initial.extend(std::iter::repeat_n(u, multiplicity));
        }
        for f in [0.2, 0.4, 0.6, 0.8] {
            initial.push(f * ua);
            initial.push(ub + f * (1.0 - ub));
        }
        initial.sort_by(f64::total_cmp);

        // The pole region cannot follow the law (a regular tip is a paraboloid,
        // the law is not): its error counts at 40 per cent.
        let relax: Vec<f64> = xs
            .iter()
            .map(|&x| {
                let tip = x < POLE_ZONE_M || (point_tail && x > length - POLE_ZONE_M);
                if tip {
                    POLE_RELAXATION
                } else {
                    1.0
                }
            })
            .collect();
        let ones = vec![1.0; xs.len()];

        Ok(Self {
            config,
            options,
            length,
            degree,
            x_nose,
            ua,
            ub,
            pin_cabin: config.upper_deck_hump().is_none(),
            point_tail,
            multiplicity,
            xs,
            us,
            data,
            crown,
            keel,
            plan,
            n_meas,
            crown_t,
            keel_t,
            plan_t,
            initial,
            features,
            r_front: estimate_tip_radii(config, length),
            r_rear: options.rear_tip_radius_m,
            relax,
            ones,
        })
    }

    /// The open cabin interval in which no knot may sit, if it is pinned.
    pub(super) fn keep_clear(&self) -> Option<(f64, f64)> {
        self.pin_cabin.then_some((self.ua, self.ub))
    }

    /// The growth engine; `exponent` selects the unrelaxed error weights.
    pub(super) fn engine(&self, exponent: bool) -> Engine<'_> {
        Engine {
            options: self.options,
            u: &self.us,
            relax: if exponent { &self.ones } else { &self.relax },
            keep_clear: self.keep_clear(),
        }
    }

    /// Constant-cabin conditions: the value at the forward end, `p' = p'' =
    /// p''' = 0` there and `p' = p'' = 0` at the aft end (a quintic with triple
    /// knots at both ends, whose cabin span is then constant).
    pub(super) fn cabin_pins(&self, value: f64) -> Vec<FitConstraint> {
        let mut c = Vec::new();
        if self.pin_cabin {
            c.push(pin(self.ua, 0, value));
            c.extend((1..=3).map(|o| pin(self.ua, o, 0.0)));
            c.extend((1..=2).map(|o| pin(self.ub, o, 0.0)));
        }
        c
    }

    /// Tip and cabin conditions of a squared extent with front tip radius `r`.
    pub(super) fn extent_pins(&self, r_front: f64, cabin_value: f64) -> Vec<FitConstraint> {
        let mut c = vec![pin(0.0, 0, 0.0), pin(0.0, 1, 2.0 * r_front * self.length)];
        if self.point_tail {
            c.push(pin(1.0, 0, 0.0));
            c.push(pin(1.0, 1, -2.0 * self.r_rear * self.length));
        }
        c.extend(self.cabin_pins(cabin_value));
        c
    }

    pub(super) fn samples(
        &self,
        y: impl Fn(usize, &Sample) -> f64,
        w: impl Fn(usize, &Sample) -> f64,
    ) -> Vec<FitSample> {
        self.data
            .iter()
            .enumerate()
            .map(|(i, s)| FitSample {
                x: s.u,
                y: y(i, s),
                weight: w(i, s),
            })
            .collect()
    }
}
