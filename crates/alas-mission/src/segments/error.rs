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
    /// A cruise distance or its mean speed was negative or not finite, the
    /// mean speed was zero, or the distance was zero on a leg that changes
    /// speed: the leg has no time span its discretization is defined on.
    #[error(
        "segment {tag} cruise distance {distance_m} m at a mean speed of {mean_speed_m_s} m/s has no usable time span: both must be finite, the speed greater than zero, and the distance greater than zero where the speed changes along the leg"
    )]
    NonPositiveCruiseSpan {
        /// The segment that was given the distance.
        tag: String,
        /// The configured cruise distance, m.
        distance_m: f64,
        /// The mean true airspeed of the leg's speed profile, m/s.
        mean_speed_m_s: f64,
    },
    /// A load factor other than one on a leg that is not level, or one that is
    /// not finite or below one. The lift balance `L = n W` stands in for the
    /// whole gravity vector, which is the balance of a level turn only.
    #[error("segment {tag} load factor {load_factor}: a sustained load factor of at least 1 is only defined on a level leg")]
    InvalidLoadFactor {
        /// The segment that was given the load factor.
        tag: String,
        /// The configured load factor.
        load_factor: f64,
    },
}

/// Reject a load factor that is not finite, below one, or on a path that is not level.
pub(super) fn check_load_factor(
    tag: &str,
    level: bool,
    load_factor: f64,
) -> Result<(), SegmentError> {
    if load_factor.is_finite() && load_factor >= 1.0 && (level || load_factor == 1.0) {
        Ok(())
    } else {
        Err(SegmentError::InvalidLoadFactor {
            tag: tag.to_owned(),
            load_factor,
        })
    }
}

/// [`check_load_factor`] for the load factor of `spec` on the kind of leg it is.
pub(super) fn check_spec_load_factor(spec: &super::SegmentSpec) -> Result<(), SegmentError> {
    let level = matches!(spec.kind, super::SegmentKind::Cruise { .. });
    check_load_factor(&spec.tag, level, spec.load_factor)
}

/// Reject a cruise whose time span `distance / mean speed` is not defined.
///
/// The mean speed must be finite and positive and the distance finite and
/// not negative. A leg that changes speed (`differentiated`) takes its
/// acceleration from the time-derivative operator, which does not exist on a
/// zero span, so its distance must be positive. A constant-speed leg never
/// reads that operator: at zero distance it is a leg of zero duration that
/// burns no fuel and hands its start state on unchanged.
pub(super) fn check_cruise_span(
    tag: &str,
    distance_m: f64,
    mean_speed_m_s: f64,
    differentiated: bool,
) -> Result<(), SegmentError> {
    let speed_usable = mean_speed_m_s.is_finite() && mean_speed_m_s > 0.0;
    let span_usable = speed_usable && distance_m.is_finite() && {
        let span_s = distance_m / mean_speed_m_s;
        span_s.is_finite() && (span_s > 0.0 || (distance_m == 0.0 && !differentiated))
    };
    if span_usable {
        Ok(())
    } else {
        Err(SegmentError::NonPositiveCruiseSpan {
            tag: tag.to_owned(),
            distance_m,
            mean_speed_m_s,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cruise_needs_a_defined_time_span() {
        assert!(check_cruise_span("cruise", 1.0e6, 230.0, false).is_ok());
        assert!(check_cruise_span("cruise", 1.0e6, 230.0, true).is_ok());
        // A constant-speed leg of zero distance is a leg of zero duration.
        assert!(check_cruise_span("cruise", 0.0, 230.0, false).is_ok());
        for (distance_m, mean_speed_m_s, differentiated) in [
            (0.0, 230.0, true),
            (-1.0e3, 230.0, false),
            (f64::NAN, 230.0, false),
            (0.0, 0.0, false),
            (1.0e6, 0.0, false),
            (1.0e6, f64::NAN, false),
            (1.0e6, f64::INFINITY, false),
        ] {
            assert!(
                matches!(
                    check_cruise_span("cruise", distance_m, mean_speed_m_s, differentiated),
                    Err(SegmentError::NonPositiveCruiseSpan { .. })
                ),
                "{distance_m} m at {mean_speed_m_s} m/s"
            );
        }
    }

    #[test]
    fn a_load_factor_above_one_is_defined_on_a_level_leg_only() {
        assert!(check_load_factor("turn", true, 1.3).is_ok());
        assert!(check_load_factor("cruise", true, 1.0).is_ok());
        assert!(check_load_factor("climb", false, 1.0).is_ok());
        assert!(matches!(
            check_load_factor("climb", false, 1.3),
            Err(SegmentError::InvalidLoadFactor { .. })
        ));
        for bad in [0.5, f64::NAN, f64::INFINITY] {
            assert!(check_load_factor("turn", true, bad).is_err(), "{bad}");
        }
    }
}
