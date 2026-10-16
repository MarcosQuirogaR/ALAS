// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lift range over which a VSPAERO comparison is requested.
//!
//! VSPAERO is an inviscid vortex-lattice solver with a relaxed free wake. At
//! the cruise Mach number the lift range in which an aircraft may be flown is
//! bounded by buffet onset, and the loaded-wing wake of an inviscid lattice
//! stops settling long before that range ends: on the 2026-10 A320-200 run the
//! free-wake history was a clean plateau at CL = -0.08, 0.41 and 0.77 and an
//! unsettled oscillation at CL = 1.26 and 1.63 (the wake gate, in
//! `vspaero_wake`, rejects the latter). Requesting points the method cannot
//! settle only produces `completed_not_comparable` for a cell whose valid
//! range is fine, and states nothing about the valid range.
//!
//! The requested sweep is therefore limited to `|CL| <= 1.3 CL_cruise`, taken
//! on the ALAS lattice's own lift curve: operating practice keeps a 1.3 g load
//! factor available before buffet onset at the cruise point (CS 25.251(e),
//! EASA CS-25, "Vibration and buffeting"; Airbus, *Getting to Grips with
//! Aircraft Performance*, 2002; the same criterion as the buffet margin in
//! `alas-opt`). Angles of attack beyond it are not requested and are listed on
//! the result as outside the validity domain; they are never reported as
//! passes. The limit is an operating criterion, not a stall or buffet
//! prediction.

use crate::full_analysis::AnalysisReport;

/// Load factor on the cruise lift coefficient that bounds the compared range.
pub const VALIDITY_LOAD_FACTOR: f64 = 1.3;

/// Fewest points a restricted sweep may keep; below this the full report
/// schedule is requested instead, since two points cannot show a trend.
const MIN_DOMAIN_POINTS: usize = 3;

/// The cruise lift coefficient of the report: trimmed when a trim exists.
fn design_cl(report: &AnalysisReport) -> f64 {
    report
        .trimmed_design_point
        .map_or(report.design_point.cl, |trim| trim.cl)
}

/// Split the report's reporting-axis angle schedule into the angles inside the
/// validity domain and those outside it, as `(inside, outside)`.
///
/// Membership is `|CL| <= VALIDITY_LOAD_FACTOR * |CL_cruise|` on the report's
/// own polar. A non-finite or empty lift curve, or fewer than
/// [`MIN_DOMAIN_POINTS`] angles inside, leaves the whole schedule inside.
pub fn validity_schedule(report: &AnalysisReport) -> (Vec<f64>, Vec<f64>) {
    split_schedule(&report.polar.alpha_deg, &report.polar.cl, design_cl(report))
}

fn split_schedule(alpha_deg: &[f64], cl: &[f64], cruise_cl: f64) -> (Vec<f64>, Vec<f64>) {
    let limit = VALIDITY_LOAD_FACTOR * cruise_cl.abs();
    if alpha_deg.len() != cl.len()
        || !limit.is_finite()
        || limit <= 0.0
        || cl.iter().any(|value| !value.is_finite())
    {
        return (alpha_deg.to_vec(), Vec::new());
    }
    let (inside, outside): (Vec<_>, Vec<_>) = alpha_deg
        .iter()
        .zip(cl)
        .partition(|(_, value)| value.abs() <= limit);
    if inside.len() < MIN_DOMAIN_POINTS {
        return (alpha_deg.to_vec(), Vec::new());
    }
    (
        inside.into_iter().map(|(alpha, _)| *alpha).collect(),
        outside.into_iter().map(|(alpha, _)| *alpha).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lattice-like lift curve, CL = 0.1 per degree + 0.4 at alpha 0.
    fn curve() -> (Vec<f64>, Vec<f64>) {
        let alpha: Vec<f64> = (-4..=10).map(f64::from).collect();
        let cl = alpha.iter().map(|a| 0.4 + 0.1 * a).collect();
        (alpha, cl)
    }

    #[test]
    fn the_sweep_stops_at_one_point_three_times_the_cruise_lift() {
        let (alpha, cl) = curve();
        // Cruise CL 0.5: limit 0.65, so alpha <= 2.5 deg is inside.
        let (inside, outside) = split_schedule(&alpha, &cl, 0.5);
        assert_eq!(inside.first(), Some(&-4.0));
        assert_eq!(inside.last(), Some(&2.0));
        assert_eq!(outside.first(), Some(&3.0));
        assert_eq!(outside.last(), Some(&10.0));
        assert_eq!(inside.len() + outside.len(), alpha.len());
    }

    #[test]
    fn a_degenerate_curve_or_cruise_lift_requests_the_whole_schedule() {
        let (alpha, cl) = curve();
        for cruise in [0.0, f64::NAN, -0.0] {
            assert!(split_schedule(&alpha, &cl, cruise).1.is_empty());
        }
        let mut broken = cl.clone();
        broken[3] = f64::NAN;
        assert!(split_schedule(&alpha, &broken, 0.5).1.is_empty());
        // A tiny cruise lift would leave under three points inside.
        assert!(split_schedule(&alpha, &cl, 0.01).1.is_empty());
        assert!(split_schedule(&alpha, &cl[..3], 0.5).1.is_empty());
    }
}
