// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The web-gauge bracket of one station's strength search
//! ([`super::product_strength`]): from the manufacturing minimum by
//! doubling, or grown geometrically around the gauge the station took at a
//! nearby cover gauge. Either bracket holds a failing and a passing gauge
//! for the same narrowing.

use super::WingboxSizing;

/// First relative step of the bracket grown around a web-gauge hint; each
/// further step is four times the last.
pub(super) const HINT_STEP: f64 = 2.0e-3;

/// A station's web-gauge bracket, `(lower, its web utilization, upper, its
/// web utilization)` with `lower` failing and `upper` passing; or, when
/// there is nothing to narrow, the gauge whose section is left installed.
pub(super) type WebBracket = Result<(f64, f64, f64, f64), f64>;

/// The cold bracket: the minimum, then doubling.
pub(super) fn from_minimum(
    sizing: &mut WingboxSizing,
    minimum: f64,
    ceiling: f64,
    feasible: f64,
    trial: &impl Fn(&mut WingboxSizing, f64) -> (f64, f64),
) -> WebBracket {
    let (peak, mut lower_web) = trial(sizing, minimum);
    if peak <= feasible || peak.is_nan() {
        return Err(minimum);
    }
    let mut lower = minimum;
    let mut upper = 2.0 * lower.max(1.0e-12);
    let mut upper_web = f64::INFINITY;
    let mut upper_peak = f64::INFINITY;
    for _ in 0..64 {
        (upper_peak, upper_web) = trial(sizing, upper);
        if upper_peak <= feasible || upper_peak.is_nan() || upper >= ceiling {
            break;
        }
        lower = upper;
        lower_web = upper_web;
        upper = (2.0 * upper).min(ceiling);
    }
    if upper_peak > feasible || upper_peak.is_nan() {
        return Err(upper);
    }
    Ok((lower, lower_web, upper, upper_web))
}

/// The warm bracket: the hint, then steps growing fourfold away from it,
/// down to the minimum while it passes or up to the ceiling while it fails.
/// An undefined utilization falls back to the cold bracket, which reports
/// it as without a hint.
pub(super) fn from_hint(
    sizing: &mut WingboxSizing,
    hint: f64,
    minimum: f64,
    ceiling: f64,
    feasible: f64,
    trial: &impl Fn(&mut WingboxSizing, f64) -> (f64, f64),
) -> WebBracket {
    let (peak, web_at_hint) = trial(sizing, hint);
    if peak.is_nan() {
        return from_minimum(sizing, minimum, ceiling, feasible, trial);
    }
    let mut step = HINT_STEP;
    if peak <= feasible {
        let (mut upper, mut upper_web) = (hint, web_at_hint);
        for _ in 0..64 {
            let lower = (upper / (1.0 + step)).max(minimum);
            let (peak, lower_web) = trial(sizing, lower);
            if peak.is_nan() {
                return from_minimum(sizing, minimum, ceiling, feasible, trial);
            }
            if peak > feasible {
                return Ok((lower, lower_web, upper, upper_web));
            }
            if lower <= minimum {
                return Err(minimum);
            }
            (upper, upper_web) = (lower, lower_web);
            step *= 4.0;
        }
        return from_minimum(sizing, minimum, ceiling, feasible, trial);
    }
    let (mut lower, mut lower_web) = (hint, web_at_hint);
    for _ in 0..64 {
        let upper = (lower * (1.0 + step)).min(ceiling);
        let (peak, upper_web) = trial(sizing, upper);
        if peak.is_nan() {
            return from_minimum(sizing, minimum, ceiling, feasible, trial);
        }
        if peak <= feasible {
            return Ok((lower, lower_web, upper, upper_web));
        }
        if upper >= ceiling {
            return Err(upper);
        }
        (lower, lower_web) = (upper, upper_web);
        step *= 4.0;
    }
    from_minimum(sizing, minimum, ceiling, feasible, trial)
}
