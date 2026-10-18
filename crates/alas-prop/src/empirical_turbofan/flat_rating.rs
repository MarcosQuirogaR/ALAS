// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Bartel-Young/OpenAP take-off correlation (the previous take-off lapse,
//! now the comparison source of the take-off thrust band) and the engine
//! flat-rating temperature break.

use super::*;

/// Highest Mach number at which the take-off rating is read from the take-off
/// charts (Jenkinson Figs. 9.18(a)-9.21(a) end at Mach 0.4, except BPR 3.0 at
/// 0.6; the Bartel-Young/OpenAP fit is a low-Mach take-off fit too). Above this
/// Mach the rating is not extrapolated: it is held at its value at this Mach
/// and floored at the maximum-climb rating (see
/// `EmpiricalTurbofanModel::takeoff_available_n`).
pub(super) const TAKEOFF_CORRELATION_MAXIMUM_MACH: f64 = 0.4;

/// ISA sea-level static temperature defining the throttle ratio, K.
const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;

/// Bartel-Young/OpenAP take-off thrust ratio `F / F_SLS` at pressure ratio
/// `delta` and Mach, for bypass ratio `bpr`.
pub(super) fn bartel_young_takeoff_ratio(bpr: f64, delta: f64, mach: f64) -> f64 {
    let g0 = 0.0606 * bpr + 0.6337;
    let a = -0.4327 * delta.powi(2) + 1.3855 * delta + 0.0472;
    let z = 0.9106 * delta.powi(3) - 1.7736 * delta.powi(2) + 1.8697 * delta;
    let x = 0.1377 * delta.powi(3) - 0.4374 * delta.powi(2) + 1.3003 * delta;
    a - 0.377 * (1.0 + bpr) / ((1.0 + 0.82 * bpr) * g0).sqrt() * z * mach
        + (0.23 + 0.19 * bpr.sqrt()) * x * mach.powi(2)
}

/// Multiplier on every rated thrust for the flat-rating throttle break.
///
/// A flat-rated engine holds its rated thrust up to the throttle-ratio
/// temperature `TR` and loses thrust above it (the turbine inlet temperature
/// limit is reached). Mattingly, *Aircraft Engine Design* (2nd ed.), the
/// high-bypass turbofan thrust lapse: with `theta0` the freestream total
/// temperature over 288.15 K,
/// `alpha = delta0 (1 - 0.49 sqrt(M0))` for `theta0 <= TR` and
/// `alpha = delta0 (1 - 0.49 sqrt(M0) - 3 (theta0 - TR) / (1.5 + M0))` above.
/// The returned factor is the ratio of the second to the first, so it is 1 at
/// and below `TR` (continuous at the break) and independent of `delta0`.
/// **Flag: the coefficients are as recalled in the audit plan and were not
/// re-read from the book.**
///
/// `flat_rating_temperature_k` is the ambient static temperature at sea-level
/// static up to which the rating is flat (a TCDS datum, per engine). `None`
/// means no flat-rating datum is known: the rating is then treated as flat at
/// every temperature, which is the deck's behaviour before this break existed.
pub(super) fn flat_rating_thrust_factor(
    flat_rating_temperature_k: Option<f64>,
    flight: FlightCondition,
) -> f64 {
    let Some(flat_temperature_k) = flat_rating_temperature_k else {
        return 1.0;
    };
    let throttle_ratio = flat_temperature_k / SEA_LEVEL_TEMPERATURE_K;
    let theta0 = flight.temperature_k * (1.0 + 0.5 * (flight.gamma - 1.0) * flight.mach.powi(2))
        / SEA_LEVEL_TEMPERATURE_K;
    if theta0 <= throttle_ratio {
        return 1.0;
    }
    let mach = flight.mach;
    let unbroken = 1.0 - 0.49 * mach.sqrt();
    (1.0 - 3.0 * (theta0 - throttle_ratio) / ((1.5 + mach) * unbroken)).max(0.0)
}

/// Whether a flat-rating temperature is usable: at or above ISA sea level
/// (a throttle ratio below 1 would derate the standard day) and below 340 K.
pub(super) fn flat_rating_temperature_is_valid(flat_rating_temperature_k: Option<f64>) -> bool {
    flat_rating_temperature_k
        .is_none_or(|value| value.is_finite() && (SEA_LEVEL_TEMPERATURE_K..=340.0).contains(&value))
}
