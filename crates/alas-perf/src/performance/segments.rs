// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CS 25 climb-gradient requirements as one parametrised closed form.
//!
//! Every certification climb check in this module family is the same
//! steady-flight balance, `T/W = N/(N-1) (gamma + CD/CL)` when one engine is
//! inoperative and `T/W = gamma + CD/CL` with all engines operating, evaluated
//! at the segment's own speed, weight, configuration and thrust rating. The
//! pure-math side takes the segment's thrust ratio (available per-engine
//! thrust at the condition over the rated sea-level-static per-engine thrust)
//! as an argument: it comes from the engine deck in the callers, never from a
//! lapse constant here.
//!
//! Gradients (CS-25 Amendment 27 and 28, identical values; EASA Easy Access
//! Rules for Large Aeroplanes):
//!
//! | clause | condition | 2 engines | 3 engines | 4 engines |
//! |---|---|---|---|---|
//! | 25.121(a) | gear extended, at VLOF | positive | 0.3 % | 0.5 % |
//! | 25.121(b) | gear retracted, V2 | 2.4 % | 2.7 % | 3.0 % |
//! | 25.121(c) | final take-off, max continuous, >= 1.18 VSR | 1.2 % | 1.5 % | 1.7 % |
//! | 25.121(d) | approach, go-around thrust, <= 1.4 VSR | 2.1 % | 2.4 % | 2.7 % |
//! | 25.115(b) | net path = gross - | 0.8 % | 0.9 % | 1.0 % |
//! | 25.119 | landing climb, all engines, VREF | 3.2 % | 3.2 % | 3.2 % |
//!
//! Frames and units: dimensionless gradients (rise over distance), lift and
//! drag coefficients on the reference wing area, weights as fractions of the
//! take-off weight so a result can be compared with the installed
//! sea-level-static thrust over the take-off weight.

use super::{far25_engine_count_supported, far25_oei_gradient};

/// CS 25.119 landing-climb gradient, all engines operating.
pub const CS25_119_LANDING_CLIMB_GRADIENT: f64 = 0.032;
/// CS 25.107(g): the final take-off speed is not less than `1.18 VSR`.
pub const FINAL_TAKEOFF_MIN_OVER_VSR: f64 = 1.18;
/// CS 25.121(d): the approach-climb speed is not more than `1.4 VSR`.
pub const APPROACH_CLIMB_MAX_OVER_VSR: f64 = 1.4;
/// CS 25.121(d): the approach configuration is any configuration "in which
/// `VSR` for this configuration does not exceed 110 percent of the `VSR` for
/// the related all-engines-operating landing configuration" (14 CFR 25.121(d)
/// lead-in; CS-25 identical).
pub const APPROACH_CONFIG_MAX_VSR_OVER_LANDING: f64 = 1.10;
/// CS 25.125(b)(2)(i): `VREF` is not less than `1.23 VSR0`.
pub const VREF_MIN_OVER_VSR0: f64 = 1.23;

/// CS 25.121(a) gear-extended gradient at `VLOF`: positive (0), 0.3 %, 0.5 %.
pub fn cs25_121a_gradient(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.0),
        3 => Some(0.003),
        4 => Some(0.005),
        _ => None,
    }
}

/// CS 25.121(c) final take-off gradient: 1.2 %, 1.5 %, 1.7 %.
pub fn cs25_121c_final_gradient(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.012),
        3 => Some(0.015),
        4 => Some(0.017),
        _ => None,
    }
}

/// CS 25.121(d) approach-climb gradient: 2.1 %, 2.4 %, 2.7 %.
pub fn cs25_121d_approach_gradient(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.021),
        3 => Some(0.024),
        4 => Some(0.027),
        _ => None,
    }
}

/// CS 25.115(b) gradient by which the gross take-off flight path is reduced to
/// the net path: 0.8 %, 0.9 %, 1.0 %.
pub fn cs25_115_net_path_reduction(n_engines: i64) -> Option<f64> {
    match n_engines {
        2 => Some(0.008),
        3 => Some(0.009),
        4 => Some(0.010),
        _ => None,
    }
}

/// The certification climb checks that are evaluated as a thrust requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimbSegment {
    /// CS 25.121(b): take-off flaps, gear retracted, `V2`, one engine out.
    SecondSegment,
    /// CS 25.121(c): flaps retracted, maximum continuous thrust, one engine
    /// out, speed at least `1.18 VSR`.
    FinalSegment,
    /// CS 25.121(d): approach configuration, go-around thrust, one engine
    /// out, maximum landing weight, speed at most `1.4 VSR`, gear retracted.
    ApproachClimb,
    /// CS 25.119: landing configuration, gear extended, all engines at the
    /// thrust available 8 s after the levers leave minimum flight idle,
    /// speed `VREF`.
    LandingClimb,
}

impl ClimbSegment {
    /// Residual identifier used by the optimizer and the findings.
    pub const fn id(self) -> &'static str {
        match self {
            Self::SecondSegment => "oei_second_segment",
            Self::FinalSegment => "oei_final_segment",
            Self::ApproachClimb => "oei_approach_climb",
            Self::LandingClimb => "landing_climb",
        }
    }

    /// Whether one engine is inoperative in this check.
    pub const fn engine_out(self) -> bool {
        !matches!(self, Self::LandingClimb)
    }

    /// The clause gradient for `n_engines`, or `None` outside the 2/3/4-engine
    /// schedule.
    pub fn gradient(self, n_engines: i64) -> Option<f64> {
        match self {
            Self::SecondSegment => far25_oei_gradient(n_engines),
            Self::FinalSegment => cs25_121c_final_gradient(n_engines),
            Self::ApproachClimb => cs25_121d_approach_gradient(n_engines),
            Self::LandingClimb => {
                far25_engine_count_supported(n_engines).then_some(CS25_119_LANDING_CLIMB_GRADIENT)
            }
        }
    }
}

/// The operating point of one climb check, with every drag term resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimbPoint {
    /// Which clause is evaluated.
    pub segment: ClimbSegment,
    /// Installed engine count.
    pub n_engines: i64,
    /// Lift coefficient of the steady climb.
    pub cl: f64,
    /// Total drag coefficient at that lift coefficient: clean point drag plus
    /// the configuration, gear, windmilling and trim increments that belong
    /// to the clause.
    pub cd: f64,
    /// Segment weight divided by the take-off weight (1 at take-off, the
    /// landing-mass fraction at the approach and landing checks).
    pub weight_over_takeoff: f64,
    /// Available per-engine thrust at the segment condition and rating,
    /// divided by the rated sea-level-static per-engine thrust. Include the
    /// go-around spool-up fraction here for the landing climb.
    pub thrust_ratio: f64,
}

/// Required sea-level-static installed thrust over take-off weight [-] for
/// one climb check: `N/(N-1) (gamma + CD/CL) (W/W_TO) / r` with one engine out,
/// without `N/(N-1)` with all engines operating.
///
/// Returns `None` for an engine count outside the 2/3/4 schedule, for a
/// non-finite or non-positive lift coefficient, thrust ratio or weight
/// fraction, or a negative drag coefficient.
pub fn required_sls_thrust_to_weight(point: ClimbPoint) -> Option<f64> {
    let gradient = point.segment.gradient(point.n_engines)?;
    let valid = point.cl.is_finite()
        && point.cl > 0.0
        && point.cd.is_finite()
        && point.cd >= 0.0
        && point.weight_over_takeoff.is_finite()
        && point.weight_over_takeoff > 0.0
        && point.thrust_ratio.is_finite()
        && point.thrust_ratio > 0.0;
    if !valid {
        return None;
    }
    let factor = if point.segment.engine_out() {
        point.n_engines as f64 / (point.n_engines - 1) as f64
    } else {
        1.0
    };
    let required =
        factor * (gradient + point.cd / point.cl) * point.weight_over_takeoff / point.thrust_ratio;
    required.is_finite().then_some(required)
}

/// Lift coefficient at a speed that is `speed_over_vsr` times the reference
/// stall speed of the same configuration: `CL = CLmax / (V/VSR)^2`, with the
/// configured `CLmax` read as the lift coefficient at `VSR` on a 1-g basis.
pub fn cl_at_speed_ratio(cl_max: f64, speed_over_vsr: f64) -> Option<f64> {
    (cl_max.is_finite() && cl_max > 0.0 && speed_over_vsr.is_finite() && speed_over_vsr > 0.0)
        .then(|| cl_max / (speed_over_vsr * speed_over_vsr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cs25_gradient_tables_are_the_published_values() {
        // CS-25 Amdt 27/28: 25.121(a) positive/0.3/0.5, (b) 2.4/2.7/3.0,
        // (c) 1.2/1.5/1.7, (d) 2.1/2.4/2.7; 25.115(b) 0.8/0.9/1.0; 25.119 3.2.
        for (n, a, b, c, d, net) in [
            (2, 0.0, 0.024, 0.012, 0.021, 0.008),
            (3, 0.003, 0.027, 0.015, 0.024, 0.009),
            (4, 0.005, 0.030, 0.017, 0.027, 0.010),
        ] {
            assert_eq!(cs25_121a_gradient(n), Some(a));
            assert_eq!(ClimbSegment::SecondSegment.gradient(n), Some(b));
            assert_eq!(ClimbSegment::FinalSegment.gradient(n), Some(c));
            assert_eq!(ClimbSegment::ApproachClimb.gradient(n), Some(d));
            assert_eq!(cs25_115_net_path_reduction(n), Some(net));
            assert_eq!(ClimbSegment::LandingClimb.gradient(n), Some(0.032));
        }
        for n in [0, 1, 5, 6] {
            for segment in [
                ClimbSegment::SecondSegment,
                ClimbSegment::FinalSegment,
                ClimbSegment::ApproachClimb,
                ClimbSegment::LandingClimb,
            ] {
                assert_eq!(segment.gradient(n), None);
            }
            assert_eq!(cs25_121a_gradient(n), None);
            assert_eq!(cs25_115_net_path_reduction(n), None);
        }
    }

    #[test]
    fn the_gradients_order_as_the_regulation_does() {
        // The second segment is the steepest engine-out requirement and the
        // final segment the shallowest; the approach climb sits between.
        for n in 2..=4 {
            let second = ClimbSegment::SecondSegment.gradient(n).unwrap_or(f64::NAN);
            let approach = ClimbSegment::ApproachClimb.gradient(n).unwrap_or(f64::NAN);
            let final_segment = ClimbSegment::FinalSegment.gradient(n).unwrap_or(f64::NAN);
            assert!(final_segment < approach && approach < second);
            // More engines never relax a requirement.
            if n > 2 {
                let fewer = ClimbSegment::SecondSegment
                    .gradient(n - 1)
                    .unwrap_or(f64::NAN);
                assert!(second > fewer);
            }
        }
    }

    fn point(segment: ClimbSegment, n: i64) -> ClimbPoint {
        ClimbPoint {
            segment,
            n_engines: n,
            cl: 1.2,
            cd: 0.12,
            weight_over_takeoff: 1.0,
            thrust_ratio: 0.8,
        }
    }

    #[test]
    fn the_engine_out_closed_form_matches_by_hand() {
        // N = 2, gamma 0.024, CD/CL = 0.10, r = 0.8: 2 * 0.124 / 0.8 = 0.31.
        let required = required_sls_thrust_to_weight(point(ClimbSegment::SecondSegment, 2));
        assert!((required.unwrap_or(f64::NAN) - 0.31).abs() < 1e-12);
        // N = 4: 4/3 * (0.030 + 0.10) / 0.8.
        let quad = required_sls_thrust_to_weight(point(ClimbSegment::SecondSegment, 4));
        assert!((quad.unwrap_or(f64::NAN) - 4.0 / 3.0 * 0.130 / 0.8).abs() < 1e-12);
    }

    #[test]
    fn the_all_engine_landing_climb_has_no_engine_out_factor() {
        let required = required_sls_thrust_to_weight(point(ClimbSegment::LandingClimb, 2));
        assert!((required.unwrap_or(f64::NAN) - (0.032 + 0.10) / 0.8).abs() < 1e-12);
    }

    #[test]
    fn a_heavier_or_draggier_segment_needs_more_thrust() {
        let base = point(ClimbSegment::ApproachClimb, 2);
        let reference = required_sls_thrust_to_weight(base).unwrap_or(f64::NAN);
        let heavier = ClimbPoint {
            weight_over_takeoff: 1.1,
            ..base
        };
        let draggier = ClimbPoint { cd: 0.14, ..base };
        let weaker = ClimbPoint {
            thrust_ratio: 0.7,
            ..base
        };
        for variant in [heavier, draggier, weaker] {
            assert!(required_sls_thrust_to_weight(variant).unwrap_or(f64::NAN) > reference);
        }
    }

    #[test]
    fn invalid_points_are_rejected_not_defaulted() {
        let base = point(ClimbSegment::SecondSegment, 2);
        assert!(required_sls_thrust_to_weight(ClimbPoint { cl: 0.0, ..base }).is_none());
        assert!(required_sls_thrust_to_weight(ClimbPoint { cd: -0.01, ..base }).is_none());
        assert!(required_sls_thrust_to_weight(ClimbPoint {
            thrust_ratio: f64::NAN,
            ..base
        })
        .is_none());
        assert!(required_sls_thrust_to_weight(ClimbPoint {
            n_engines: 5,
            ..base
        })
        .is_none());
    }

    #[test]
    fn the_speed_ratio_lift_schedule_is_the_inverse_square() {
        assert_eq!(cl_at_speed_ratio(2.0, 1.0), Some(2.0));
        assert!((cl_at_speed_ratio(2.0, 1.4).unwrap_or(f64::NAN) - 2.0 / 1.96).abs() < 1e-12);
        assert!(cl_at_speed_ratio(0.0, 1.2).is_none());
        assert!(cl_at_speed_ratio(1.8, f64::NAN).is_none());
    }
}
