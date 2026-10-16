// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evaluations of the legacy laws and the quantities derived from them.

use alas_config::loft::{FairnessBudget, InflectionBudget};
use alas_config::FuselageConfig;

/// Distance from a tip within which the outline error is relaxed, m.
pub(super) const POLE_ZONE_M: f64 = 0.6;
/// Factor by which the error bound is relaxed in the pole zone.
pub(super) const POLE_RELAXATION: f64 = 2.5;
/// Extra inflections per line the stored budget allows over the law's own: a C2
/// rounding of a curvature step (radome arc to windshield) overshoots to the
/// other sign once, by a curvature radius of tens of metres.
const ROUNDING_ALLOWANCE: u32 = 2;

/// Law values on a station: half-width, half-height, centre height, exponent.
pub(super) fn law_at(config: &FuselageConfig, x: f64, length: f64) -> (f64, f64, f64, f64) {
    let st = config.body_station(x, length);
    let (z, height) = config.raise_crown(x, st.z_m, st.height_m);
    (0.5 * st.width_m, 0.5 * height, z, st.shape)
}

/// Plan and side tip radii of the law: the paraboloids `a^2 = 2 R_a x` and
/// `e^2 = 2 R_e x` that fit the law best in the first half metre (least
/// squares), clamped to a sane range and to a ratio of at most 8.
pub(super) fn estimate_tip_radii(config: &FuselageConfig, length: f64) -> (f64, f64) {
    let (mut na, mut ne, mut den) = (0.0, 0.0, 0.0);
    for i in 1..=40 {
        let x = 0.5 * f64::from(i) / 40.0;
        let (a, e, _, _) = law_at(config, x, length);
        na += x * a * a;
        ne += x * e * e;
        den += x * x;
    }
    let (mut ra, mut re) = (
        (na / (2.0 * den)).clamp(0.02, 3.0),
        (ne / (2.0 * den)).clamp(0.02, 3.0),
    );
    if ra > 8.0 * re {
        ra = 8.0 * re;
    }
    if re > 8.0 * ra {
        re = 8.0 * ra;
    }
    (ra, re)
}

/// Crown, keel and plan lines of the law convolved with a Gaussian of standard
/// deviation `sigma` (13 taps over plus or minus 3 sigma, ends clamped).
pub(super) fn smoothed_lines(
    config: &FuselageConfig,
    length: f64,
    x: f64,
    sigma: f64,
) -> (f64, f64, f64) {
    let (mut c, mut k, mut p, mut wsum) = (0.0, 0.0, 0.0, 0.0);
    for i in -6..=6 {
        let delta = 0.5 * sigma * f64::from(i);
        let w = (-0.5 * (0.5 * f64::from(i)).powi(2)).exp();
        let (a, e, z, _) = law_at(config, (x + delta).clamp(0.0, length), length);
        c += w * (z + e);
        k += w * (z - e);
        p += w * a;
        wsum += w;
    }
    (c / wsum, k / wsum, p / wsum)
}

/// The absolute budget of a legacy fit: the inflections of the law's own
/// crown, keel and plan lines (same sign rule as the fairness report) plus the
/// rounding allowance, and a curvature total-variation ratio limit of 3.
pub(super) fn law_budget(config: &FuselageConfig, length: f64) -> FairnessBudget {
    let m = 2000usize;
    let xs: Vec<f64> = (0..=m)
        .map(|i| 0.5 * length * (1.0 - (std::f64::consts::PI * i as f64 / m as f64).cos()))
        .collect();
    let (mut crown, mut keel, mut plan, mut scale) = (vec![], vec![], vec![], vec![]);
    for &x in &xs {
        let (a, e, z, _) = law_at(config, x, length);
        crown.push(z + e);
        keel.push(z - e);
        plan.push(a);
        scale.push((a * e).sqrt());
    }
    let count = |f: &[f64]| crate::loft::fairness::sampled_inflections(&xs, f, &scale) as u32;
    FairnessBudget {
        inflections: InflectionBudget {
            crown: count(&crown) + ROUNDING_ALLOWANCE,
            keel: count(&keel) + ROUNDING_ALLOWANCE,
            plan: count(&plan) + ROUNDING_ALLOWANCE,
        },
        tv_ratio_max: 3.0,
    }
}
