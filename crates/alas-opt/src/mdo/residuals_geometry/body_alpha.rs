// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The geometric body-attitude window residual of the Geometry family.

use crate::mdo::types::ConstraintFamily::Geometry;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// Scale floor of a degree-valued window residual: one degree is the
/// resolution of the design variables and the order of the attitude change
/// the window is meant to separate.
const SCALE_FLOOR_DEG: f64 = 1.0;

/// Relative slack of the body-attitude window residual: the value of
/// `types::NUMERICAL_SLACK`, which the scaled constructor applied before this
/// residual needed an explicit scale.
const BODY_ALPHA_NUMERICAL_SLACK: f64 = 1.0e-5;

/// A two-sided residual for the configured geometric body-attitude window.
///
/// The raw value is positive only outside the interval; inside, the margin
/// to the nearer bound is reported as a negative value.  The residual uses a
/// degree unit and the nearest violated bound so the optimizer receives a
/// useful direction without treating the interval as a local-section solver
/// validity claim.
pub(super) fn body_alpha_window_residual(
    actual_deg: f64,
    min_deg: f64,
    max_deg: f64,
    role: ResidualRole,
) -> ConstraintResidual {
    let (limit_deg, raw_residual) = if actual_deg < min_deg {
        (min_deg, min_deg - actual_deg)
    } else if actual_deg > max_deg {
        (max_deg, actual_deg - max_deg)
    } else {
        let slack_to_min = actual_deg - min_deg;
        let slack_to_max = max_deg - actual_deg;
        if slack_to_min < slack_to_max {
            (min_deg, -slack_to_min)
        } else {
            (max_deg, -slack_to_max)
        }
    };
    // The window may start at zero (D8: the lower bound is the level
    // fuselage), where a scale of the bound's own magnitude is zero.  The
    // scale is the magnitude of the nearest bound with a one-degree floor,
    // so the upper bound (4 deg) normalizes exactly as before and a miss of
    // the zero lower bound counts one unit per degree.
    let scale_deg = limit_deg.abs().max(SCALE_FLOOR_DEG);
    let normalized = if [actual_deg, limit_deg, raw_residual]
        .iter()
        .all(|value| value.is_finite())
    {
        (raw_residual / scale_deg - BODY_ALPHA_NUMERICAL_SLACK).max(0.0)
    } else {
        f64::INFINITY
    };
    ConstraintResidual::direct(
        "geometric_body_alpha",
        Geometry,
        actual_deg,
        limit_deg,
        "deg",
        raw_residual,
        normalized,
        role,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn residual(actual: f64) -> ConstraintResidual {
        body_alpha_window_residual(actual, 0.0, 4.0, ResidualRole::Constraint)
    }

    #[test]
    fn a_window_that_starts_at_zero_has_a_finite_scale() {
        // One degree nose-down is one unit, not 1e9 (a zero-magnitude bound).
        let below = residual(-1.0);
        assert!((below.normalized_violation - 1.0).abs() < 1e-4);
        assert_eq!(below.limit, 0.0);
        assert!((below.raw_residual - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_upper_bound_keeps_its_relative_scale_and_the_inside_is_feasible() {
        let above = residual(5.0);
        assert!((above.normalized_violation - 0.25).abs() < 1e-4);
        for inside in [0.0, 0.7, 2.0, 4.0] {
            assert_eq!(residual(inside).normalized_violation, 0.0, "{inside}");
        }
        assert!(residual(f64::NAN).normalized_violation.is_infinite());
    }
}
