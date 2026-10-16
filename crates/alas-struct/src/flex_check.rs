// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The 1 g stiffness check: how far a sized box flexes in level flight and at
//! ultimate load, beside the published flex of real wings.
//!
//! This is a diagnostic, not a gate. The sizing is strength-governed; the
//! stiffness it produces is a consequence, and whether that consequence is
//! physical is a validation question. The check reports the tip rise of the
//! 1 g flight shape and of the pull-up ultimate case relative to the semispan,
//! and the two sweep factors that bound how much of a published-flex gap a
//! swept wing can explain (see [`swept_axis_deflection_factors`]).
//!
//! # Published references (validation only, never used to tune)
//!
//! * Airbus A380: "During take-off the wing will flex upwards by over 4 m"
//!   (A380 Facts and Figures, 2022, p.4): ground droop plus the 1 g rise.
//! * Boeing 787-8 static test wing: about 7.6 m (25 ft) upward at ultimate
//!   load (Boeing news release, 28 March 2010).
//!
//! # Sweep (derived, not literature-verified in this lane)
//!
//! The beam model integrates `M/EI` in the streamwise frame. A uniform
//! cantilever swept by `Lambda` whose section properties are those of the
//! streamwise cut deflects, under a vertical load on its elastic axis,
//! `p l^4 / (8 E I_perp)` with `l = L / cos(Lambda)` and
//! `I_perp = I_s cos(Lambda)`, against `q L^4 / (8 E I_s)` for the straight
//! beam with the same total load: a ratio of `1 / cos^4(Lambda)`. If the box is
//! instead re-sized for the swept-frame stress, which is `1 / cos^2(Lambda)`
//! higher, the ratio is `1 / cos^2(Lambda)`. Root-clamp, torsion and
//! bending-twist coupling are not in either number.

use crate::analytical::StructuralAnalysisReport;
use crate::feasibility::FLIGHT_SHAPE_CASE;

/// A380 take-off wing flex, m, ground to 1 g (Airbus A380 Facts and Figures,
/// 2022, p.4: "over 4 m"). A lower bound of the published value.
pub const A380_TAKEOFF_FLEX_M: f64 = 4.0;

/// Boeing 787-8 static-test wing tip rise at ultimate load, m (about 25 ft;
/// Boeing, 28 March 2010).
pub const B787_ULTIMATE_TIP_RISE_M: f64 = 7.6;

/// Name of the ultimate manoeuvre case the check reads.
const ULTIMATE_CASE: &str = "pull-up";

/// Tip rise of the sized box and the sweep bounds on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexibilityCheck {
    /// Semispan, m.
    pub semi_span_m: f64,
    /// Tip rise in 1 g level flight (straight beam), m.
    pub level_tip_rise_m: f64,
    /// Tip rise at the pull-up ultimate case (straight beam, linear), m.
    pub ultimate_tip_rise_m: f64,
    /// `level_tip_rise_m / semi_span_m`.
    pub level_tip_over_semi_span: f64,
    /// `ultimate_tip_rise_m / semi_span_m`.
    pub ultimate_tip_over_semi_span: f64,
    /// Swept-axis deflection factor for the box as sized, `1 / cos^4`.
    pub swept_axis_factor_as_sized: f64,
    /// Swept-axis factor if the box were re-sized for the swept stress,
    /// `1 / cos^2`.
    pub swept_axis_factor_resized: f64,
}

/// The two swept-axis deflection factors `(1 / cos^4, 1 / cos^2)` for an
/// elastic-axis sweep in radians, or `None` when it is not a finite angle with
/// `|Lambda| < 90 deg`.
pub fn swept_axis_deflection_factors(sweep_rad: f64) -> Option<(f64, f64)> {
    if !sweep_rad.is_finite() || sweep_rad.abs() >= std::f64::consts::FRAC_PI_2 {
        return None;
    }
    let c2 = sweep_rad.cos().powi(2);
    Some((1.0 / (c2 * c2), 1.0 / c2))
}

/// The 1 g and ultimate flexibility of an analysed box, `None` when the report
/// lacks the level or pull-up case or holds a non-finite tip rise.
pub fn flexibility_check(
    report: &StructuralAnalysisReport,
    elastic_axis_sweep_rad: f64,
) -> Option<FlexibilityCheck> {
    let semi_span_m = *report.y.last()?;
    let tip = |name: &str| {
        report
            .load_cases
            .iter()
            .find(|case| case.name == name)
            .map(|case| case.tip_deflection_m)
            .filter(|value| value.is_finite())
    };
    let level = tip(FLIGHT_SHAPE_CASE)?;
    let ultimate = tip(ULTIMATE_CASE)?;
    let (as_sized, resized) = swept_axis_deflection_factors(elastic_axis_sweep_rad)?;
    (semi_span_m.is_finite() && semi_span_m > 0.0).then_some(FlexibilityCheck {
        semi_span_m,
        level_tip_rise_m: level,
        ultimate_tip_rise_m: ultimate,
        level_tip_over_semi_span: level / semi_span_m,
        ultimate_tip_over_semi_span: ultimate / semi_span_m,
        swept_axis_factor_as_sized: as_sized,
        swept_axis_factor_resized: resized,
    })
}

/// Computed over published flex: below 1 means the model is stiffer than the
/// published wing. `None` for a non-positive or non-finite published value.
pub fn computed_over_published(computed_m: f64, published_m: f64) -> Option<f64> {
    (published_m.is_finite() && published_m > 0.0 && computed_m.is_finite())
        .then(|| computed_m / published_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytical::{LoadCaseResult, ModalResult};

    fn case(name: &'static str, tip: f64) -> LoadCaseResult {
        LoadCaseResult {
            name,
            load_factor: 1.0,
            y: vec![0.0, 10.0],
            q_net: vec![0.0; 2],
            shear_n: vec![0.0; 2],
            moment_nm: vec![0.0; 2],
            deflection_m: vec![0.0, tip],
            tip_deflection_m: tip,
            spar_stress: Vec::new(),
        }
    }

    fn report(cases: Vec<LoadCaseResult>) -> StructuralAnalysisReport {
        StructuralAnalysisReport {
            y: vec![0.0, 10.0],
            ei_nm2: vec![1.0; 2],
            load_cases: cases,
            modal: ModalResult {
                frequencies_hz: Vec::new(),
                mode_shapes: Vec::new(),
            },
        }
    }

    #[test]
    fn sweep_factors_are_the_inverse_cosine_powers() {
        assert_eq!(swept_axis_deflection_factors(0.0), Some((1.0, 1.0)));
        let (a, b) = swept_axis_deflection_factors(33.5_f64.to_radians()).expect("finite");
        assert!((a - 1.0 / 33.5_f64.to_radians().cos().powi(4)).abs() < 1.0e-12);
        assert!((b - 1.0 / 33.5_f64.to_radians().cos().powi(2)).abs() < 1.0e-12);
        assert!(a > b && b > 1.0);
        // Symmetric in the sign of the sweep.
        assert_eq!(
            swept_axis_deflection_factors(-0.5),
            swept_axis_deflection_factors(0.5)
        );
        assert!(swept_axis_deflection_factors(f64::NAN).is_none());
        assert!(swept_axis_deflection_factors(std::f64::consts::FRAC_PI_2).is_none());
    }

    #[test]
    fn the_swept_uniform_cantilever_matches_the_closed_form_ratio() {
        // Straight-frame beam: q L^4 / (8 E I_s). Swept beam of axis length
        // l = L / cos, load per axis length p = q cos... total load fixed
        // (p l = q L), I_perp = I_s cos: p l^4 / (8 E I_perp).
        let lambda = 30.0_f64.to_radians();
        let (q, length, eis) = (1.0e4_f64, 20.0_f64, 5.0e9_f64);
        let straight = q * length.powi(4) / (8.0 * eis);
        let axis_length = length / lambda.cos();
        let p = q * length / axis_length;
        let swept = p * axis_length.powi(4) / (8.0 * eis * lambda.cos());
        let (as_sized, _) = swept_axis_deflection_factors(lambda).expect("finite");
        assert!((swept / straight - as_sized).abs() < 1.0e-12);
    }

    #[test]
    fn the_check_reads_the_level_and_ultimate_tip_rise() {
        let check = flexibility_check(&report(vec![case("pull-up", 4.0), case("level", 1.0)]), 0.5)
            .expect("both cases present");
        assert_eq!(check.level_tip_rise_m, 1.0);
        assert_eq!(check.ultimate_tip_rise_m, 4.0);
        assert!((check.level_tip_over_semi_span - 0.1).abs() < 1.0e-15);
        assert!((check.ultimate_tip_over_semi_span - 0.4).abs() < 1.0e-15);
    }

    #[test]
    fn a_missing_case_or_bad_value_gives_no_check() {
        assert!(flexibility_check(&report(vec![case("level", 1.0)]), 0.5).is_none());
        assert!(flexibility_check(
            &report(vec![case("pull-up", f64::NAN), case("level", 1.0)]),
            0.5
        )
        .is_none());
        assert!(flexibility_check(
            &report(vec![case("pull-up", 4.0), case("level", 1.0)]),
            f64::NAN
        )
        .is_none());
    }

    #[test]
    fn the_published_ratio_is_defined_only_for_a_positive_reference() {
        assert_eq!(computed_over_published(2.0, 4.0), Some(0.5));
        assert!(computed_over_published(2.0, 0.0).is_none());
        assert!(computed_over_published(f64::NAN, 4.0).is_none());
    }
}
