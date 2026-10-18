// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The error a segment reports when it cannot be set up.

use alas_math::ChebyshevError;

/// Why a segment could not be set up.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SegmentError {
    /// The pseudospectral operators could not be built.
    #[error("the discretization operators could not be built: {0}")]
    Discretization(#[from] ChebyshevError),
    /// The segment's starting altitude was left to the previous segment and
    /// there is no previous segment. Upstream raises an `AttributeError`
    /// here; a library here reports rather than panicking.
    #[error("segment {tag} takes its starting altitude from a predecessor it does not have")]
    NoStartingAltitude {
        /// The segment that could not start.
        tag: String,
    },
    /// A calibrated airspeed and a control point's real ambient state implied
    /// an invalid or supersonic condition.
    #[error("segment {tag} calibrated airspeed at control point {point}: {source}")]
    InvalidCalibratedAirspeed {
        /// The segment that could not resolve its airspeed.
        tag: String,
        /// Which control point failed.
        point: usize,
        /// Why.
        source: alas_atmo::airspeed::AirspeedError,
    },
    /// A resolved airspeed at a control point did not exceed the segment's
    /// vertical rate, so no real horizontal velocity exists there. Checked
    /// per control point in calibrated-airspeed mode because true airspeed
    /// varies along the ramp; a true-airspeed segment is checked once, before
    /// construction, by its caller.
    #[error(
        "segment {tag} vertical rate {vertical_velocity_m_s} m/s at or above the {resolved_speed_m_s} m/s airspeed resolved at control point {point}"
    )]
    VerticalRateExceedsAirspeed {
        /// The segment that could not fly its ramp.
        tag: String,
        /// Which control point failed.
        point: usize,
        /// The configured vertical rate, m/s.
        vertical_velocity_m_s: f64,
        /// The true airspeed resolved at that control point, m/s.
        resolved_speed_m_s: f64,
    },
    /// A climb or descent rate was zero, negative or not finite, so the ramp
    /// would take an infinite, negative or undefined time.
    #[error("segment {tag} {kind} rate {rate_m_s} m/s must be finite and greater than zero")]
    NonPositiveVerticalRate {
        /// The segment that was given the rate.
        tag: String,
        /// `"climb"` or `"descent"`.
        kind: &'static str,
        /// The configured rate magnitude, m/s.
        rate_m_s: f64,
    },
}

/// Reject a climb or descent rate magnitude that is not finite and positive.
/// The ramp time is `dz / vz`, so zero gives an infinite or NaN time and a
/// negative rate a negative one.
pub(super) fn check_vertical_rate(
    tag: &str,
    kind: &'static str,
    rate_m_s: f64,
) -> Result<(), SegmentError> {
    if rate_m_s.is_finite() && rate_m_s > 0.0 {
        Ok(())
    } else {
        Err(SegmentError::NonPositiveVerticalRate {
            tag: tag.to_owned(),
            kind,
            rate_m_s,
        })
    }
}
