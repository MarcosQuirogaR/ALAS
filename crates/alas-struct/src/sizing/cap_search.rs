// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact cap-search acceleration: narrow the root bracket, then replay the
//! original bisection arithmetic, evaluating only undecided midpoints.

use super::law::station_cap_dimensions;
use super::section::{cap_inertia, non_cap_ei};
use super::WingboxSizing;
use crate::allowables::bending_allowable_pa;
use alas_config::materials::MaterialSpec;

pub(super) struct CapSearch<'a> {
    section: &'a WingboxSizing,
    station: usize,
    moment: f64,
    minimum_gauge: f64,
    height: f64,
    chord: f64,
    non_cap: f64,
    cap_cover: f64,
    cap_modulus: f64,
    web_modulus: f64,
    web_allowable: f64,
}

struct Trial {
    utilization: f64,
    residual: f64,
    derivative: f64,
}

impl<'a> CapSearch<'a> {
    pub(super) fn new(
        section: &'a WingboxSizing,
        station: usize,
        moment: f64,
        minimum_gauge: f64,
        skin: &MaterialSpec,
        web: &MaterialSpec,
        cap: &MaterialSpec,
    ) -> Self {
        let height = section
            .spars
            .iter()
            .map(|spar| spar.h[station])
            .fold(0.0_f64, f64::max);
        Self {
            section,
            station,
            moment: moment.abs(),
            minimum_gauge,
            height,
            chord: section.chord[station],
            non_cap: non_cap_ei(section, station, skin, web),
            cap_cover: (cap.e_pa / bending_allowable_pa(cap))
                .max(skin.e_pa / bending_allowable_pa(skin))
                * height,
            cap_modulus: cap.e_pa,
            web_modulus: web.e_pa,
            web_allowable: bending_allowable_pa(web),
        }
    }

    fn trial<const DERIVATIVE: bool>(&self, area: f64) -> Trial {
        let mut stiffness = 0.0;
        let mut derivative = 0.0;
        let mut clear_height = 0.0_f64;
        let mut clear_derivative = 0.0;
        for spar in &self.section.spars {
            let height = spar.h[self.station];
            let requested = if self.height > 0.0 {
                area * height / self.height
            } else {
                0.0
            };
            let (width, thickness) =
                station_cap_dimensions(requested, self.chord, height, self.minimum_gauge);
            stiffness += self.cap_modulus * cap_inertia(height, width, thickness);
            let (width_slope, thickness_slope) = if DERIVATIVE {
                dimension_slopes(
                    requested,
                    height / self.height,
                    self.chord,
                    height,
                    self.minimum_gauge,
                )
            } else {
                (0.0, 0.0)
            };
            if DERIVATIVE {
                let offset = 0.5 * (height - thickness);
                derivative += self.cap_modulus
                    * (2.0
                        * width_slope
                        * thickness
                        * (offset * offset + thickness * thickness / 12.0)
                        + 0.5 * width * (height - 2.0 * thickness).powi(2) * thickness_slope);
            }
            let clear = (height - 2.0 * thickness).max(0.0);
            if clear > clear_height {
                clear_height = clear;
                clear_derivative = -2.0 * thickness_slope;
            }
        }
        let ei = self.non_cap + stiffness;
        let web_lever = self.web_modulus * clear_height / self.web_allowable;
        let lever = self.cap_cover.max(web_lever);
        let lever_derivative = if web_lever > self.cap_cover {
            self.web_modulus * clear_derivative / self.web_allowable
        } else {
            0.0
        };
        Trial {
            utilization: 0.5 * (self.moment / ei) * lever,
            residual: ei - 0.5 * self.moment * lever,
            derivative: derivative - 0.5 * self.moment * lever_derivative,
        }
    }

    pub(super) fn area(&self) -> f64 {
        let needs_caps = self.trial::<false>(0.0).utilization > 1.0;
        if !needs_caps {
            return 0.0;
        }
        let maximum = self
            .section
            .spars
            .iter()
            .map(|spar| 0.1 * self.chord * spar.h[self.station])
            .fold(0.0_f64, f64::max);
        let mut lower = 0.0;
        let mut upper = (self.minimum_gauge * self.chord).max(1.0e-12).min(maximum);
        let mut installed_upper = upper;
        for _ in 0..64 {
            installed_upper = upper;
            if self.trial::<false>(upper).utilization <= 1.0 || upper >= maximum {
                break;
            }
            lower = upper;
            upper = (2.0 * upper).min(maximum);
        }
        let bracketed = self.trial::<false>(installed_upper).utilization <= 1.0;
        if !bracketed {
            // Exhausting the doubling budget retains the last evaluated
            // section, as did the original in-place cap construction.
            return installed_upper;
        }
        let original_lower = lower;
        let original_upper = upper;
        let mut point = upper;
        // Newton proposes a tighter bracket only. No gauge tolerance, search
        // budget or feasible-endpoint convention is changed by the proposal.
        for _ in 0..8 {
            let trial = self.trial::<true>(point);
            if trial.utilization > 1.0 {
                lower = point;
            } else {
                upper = point;
            }
            let proposed = point - trial.residual / trial.derivative;
            if (proposed - point).abs() <= 8.0 * f64::EPSILON * point.abs() {
                point = proposed.clamp(original_lower, original_upper);
                break;
            }
            point = if proposed.is_finite() && proposed > lower && proposed < upper {
                proposed
            } else {
                0.5 * (lower + upper)
            };
        }
        let radius = 16.0 * f64::EPSILON * point.abs();
        let probe_lower = (point - radius).max(original_lower);
        let probe_upper = (point + radius).min(original_upper);
        if self.trial::<false>(probe_lower).utilization > 1.0 {
            lower = lower.max(probe_lower);
        }
        if self.trial::<false>(probe_upper).utilization <= 1.0 {
            upper = upper.min(probe_upper);
        }
        let mut replay_lower = original_lower;
        let mut replay_upper = original_upper;
        // Evaluate nearby midpoints explicitly: rounded cap dimensions can
        // make the floating-point predicate vary by an ulp at capacity.
        let rounding_guard = 128.0 * f64::EPSILON * original_upper;
        for _ in 0..56 {
            let middle = 0.5 * (replay_lower + replay_upper);
            // Cap stiffness grows monotonically over the fabrication law's
            // h/3 gauge bound, while the clear web lever arm decreases. Thus
            // the narrowed bracket proves these original predicate outcomes.
            let infeasible = if middle < lower - rounding_guard {
                true
            } else if middle > upper + rounding_guard {
                false
            } else {
                self.trial::<false>(middle).utilization > 1.0
            };
            if infeasible {
                replay_lower = middle;
            } else {
                replay_upper = middle;
            }
        }
        replay_upper
    }
}

/// The derivatives guide Newton; cap dimensions and acceptance still use the
/// unchanged fabrication function and unchanged utilization arithmetic.
fn dimension_slopes(area: f64, scale: f64, chord: f64, height: f64, minimum: f64) -> (f64, f64) {
    if height <= 0.0 {
        return (0.0, 0.0);
    }
    let maximum_width = (0.5 * chord).max(1.0e-6);
    let maximum_thickness = 0.2 * height;
    let base_width = maximum_width.min(0.6 * height).max(1.0e-6);
    let widened = area / base_width > maximum_thickness;
    let width = if widened {
        (area / maximum_thickness)
            .min(maximum_width)
            .max(base_width)
    } else {
        base_width
    };
    let mut width_slope = if widened
        && area / maximum_thickness < maximum_width
        && area / maximum_thickness > base_width
    {
        scale / maximum_thickness
    } else {
        0.0
    };
    let mut thickness_slope = if area / width < maximum_thickness {
        (scale * width - area * width_slope) / width.powi(2)
    } else {
        0.0
    };
    let floor = minimum.min(height / 3.0).max(0.0);
    if (area / width).min(maximum_thickness) < floor {
        if width < floor {
            width_slope = 0.0;
        }
        thickness_slope = 0.0;
    }
    (width_slope, thickness_slope)
}
