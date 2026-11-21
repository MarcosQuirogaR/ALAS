// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compressible lift-curve slope of a finite swept wing: the one closed-form
//! rule used wherever the incompressible lattice slope of the main wing has to
//! be carried to a flight Mach number without a compressible solve.
//!
//! USAF DATCOM 4.1.3.2 Method 1, as printed in E. Torenbeek, *Advanced
//! Aircraft Design* (2013), Eq. 11.10, for a straight-tapered wing with a
//! section slope of `2 pi`:
//!
//! `dCL/dalpha = 2 pi A / (2 + sqrt(A^2 (1 - M^2 + tan^2 Lambda_1/2) + 4))`
//!
//! per radian, `A` the aspect ratio and `Lambda_1/2` the sweep of the
//! half-chord line. It tends to the two-dimensional Prandtl-Glauert rule
//! `2 pi / sqrt(1 - M^2)` only for an unswept wing of infinite aspect ratio;
//! on a transport wing (A = 9, `tan Lambda_1/2` = 0.5) the slope at M 0.85 is
//! 1.39 times the incompressible one, not the 1.90 of the two-dimensional
//! rule.
//!
//! # Validity
//!
//! Subsonic attached flow, `0 <= M < 1`. DATCOM lists the method for M < 0.8
//! ([`SUBSTANTIATED_MACH_MAX`]); it has no shock or critical-Mach physics, so
//! between that Mach number and 1 the value is the linear-theory
//! extrapolation that the analysis reports through its linear-model domain
//! finding. At `M >= 1`, or for a non-finite or non-positive input, no value
//! is returned.
//!
//! The direct compressible solve (the Goethert-stretched lattice of the
//! report polar and of the cruise trim) does not go through this rule: it
//! solves the same linearised problem on the actual planform.

use super::{swept_pg_beta, AeroAnalysis};

/// Highest Mach number DATCOM lists the compressible lift-curve-slope method
/// for (4.1.3.2 Method 1: M < 0.8).
pub const SUBSTANTIATED_MACH_MAX: f64 = 0.80;

/// Name of the main wing among the airplane's surfaces.
const MAIN_WING_NAME: &str = "Main Wing";

/// DATCOM lift-curve slope of a straight-tapered wing, 1/rad (module doc), at
/// aspect ratio `aspect_ratio`, half-chord sweep tangent
/// `tan_half_chord_sweep` and Mach number `mach`.
///
/// `None` outside `0 <= mach < 1` or for a non-finite or non-positive input.
pub fn finite_wing_lift_curve_slope_per_rad(
    aspect_ratio: f64,
    tan_half_chord_sweep: f64,
    mach: f64,
) -> Option<f64> {
    let valid = aspect_ratio.is_finite()
        && aspect_ratio > 0.0
        && tan_half_chord_sweep.is_finite()
        && mach.is_finite()
        && (0.0..1.0).contains(&mach);
    valid.then(|| {
        let root = ((tan_half_chord_sweep.powi(2) + 1.0 - mach * mach) * aspect_ratio.powi(2)
            + 4.0)
            .sqrt();
        2.0 * std::f64::consts::PI * aspect_ratio / (root + 2.0)
    })
}

/// Ratio of the compressible to the incompressible lift-curve slope of the
/// same wing, `a(M) / a(0)` of [`finite_wing_lift_curve_slope_per_rad`]:
/// dimensionless, 1 at `mach` = 0 and rising with it. `None` outside the
/// rule's domain.
pub fn compressible_slope_ratio(
    aspect_ratio: f64,
    tan_half_chord_sweep: f64,
    mach: f64,
) -> Option<f64> {
    let compressible =
        finite_wing_lift_curve_slope_per_rad(aspect_ratio, tan_half_chord_sweep, mach)?;
    let incompressible =
        finite_wing_lift_curve_slope_per_rad(aspect_ratio, tan_half_chord_sweep, 0.0)?;
    Some(compressible / incompressible)
}

impl AeroAnalysis<'_> {
    /// [`compressible_slope_ratio`] of the analysed main wing (the first wing
    /// when none carries the main wing's name) at `mach`: its geometric
    /// aspect ratio and the root-to-tip sweep of its half-chord line. `None`
    /// without a wing or outside the rule's domain.
    pub(super) fn main_wing_compressible_slope_ratio(&self, mach: f64) -> Option<f64> {
        let wings = &self.plane.wings;
        let wing = wings
            .iter()
            .find(|wing| wing.name == MAIN_WING_NAME)
            .or_else(|| wings.first())?;
        if wing.xsecs.len() < 2 {
            return None;
        }
        let tan_half_chord_sweep = wing.mean_sweep_angle(0.5).to_radians().tan();
        compressible_slope_ratio(wing.aspect_ratio(), tan_half_chord_sweep, mach.abs())
    }

    /// Factor that carries an angle of attack measured from the zero-lift
    /// angle on the incompressible lattice to `mach`:
    /// `alpha(M) - alpha_0 = factor (alpha(0) - alpha_0)` at the same lift
    /// coefficient, i.e. the inverse of the slope ratio.
    ///
    /// The frozen reference path keeps upstream's swept two-dimensional factor
    /// ([`swept_pg_beta`]). Outside the rule's domain the factor is `NaN`, so
    /// the angle it would have relabelled is reported as unavailable instead
    /// of being extrapolated.
    pub(super) fn incompressible_angle_factor(&self, mach: f64) -> f64 {
        if self.reference_compatibility {
            return swept_pg_beta(mach, self.sweep_deg);
        }
        self.main_wing_compressible_slope_ratio(mach)
            .map_or(f64::NAN, |ratio| 1.0 / ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slope_is_the_lifting_line_value_for_a_straight_incompressible_wing() {
        // 2 pi A / (2 + sqrt(A^2 + 4)).
        let a = 9.0_f64;
        let expected = 2.0 * std::f64::consts::PI * a / (2.0 + (a * a + 4.0).sqrt());
        let slope = finite_wing_lift_curve_slope_per_rad(a, 0.0, 0.0);
        assert!(slope.is_some_and(|s| (s - expected).abs() < 1e-12));
    }

    #[test]
    fn a_transport_wing_steepens_far_less_than_the_two_dimensional_rule() {
        // A = 9, tan(half-chord sweep) = 0.5.
        // M 0.78: sqrt(81 (0.3916 + 0.25) + 4) = 7.4812, ratio
        // (2 + sqrt(105.25)) / (2 + 7.4812) = 12.2591 / 9.4812 = 1.2930.
        // M 0.85: sqrt(81 (0.2775 + 0.25) + 4) = 6.8356, ratio 1.3875.
        let at = |mach: f64| compressible_slope_ratio(9.0, 0.5, mach);
        assert!(at(0.0).is_some_and(|r| (r - 1.0).abs() < 1e-15));
        assert!(at(0.78).is_some_and(|r| (r - 1.2930).abs() < 1e-4));
        assert!(at(0.85).is_some_and(|r| (r - 1.3875).abs() < 1e-4));
        // The two-dimensional rule at the same Mach numbers: 1.598 and 1.898.
        assert!(at(0.85).is_some_and(|r| r < 1.0 / (1.0 - 0.85_f64 * 0.85).sqrt() - 0.5));
    }

    #[test]
    fn the_unswept_infinite_wing_recovers_prandtl_glauert() {
        let ratio = compressible_slope_ratio(1.0e9, 0.0, 0.6);
        assert!(ratio.is_some_and(|r| (r - 1.25).abs() < 1e-6));
    }

    #[test]
    fn no_value_is_returned_outside_the_subsonic_domain() {
        assert!(compressible_slope_ratio(9.0, 0.5, 1.0).is_none());
        assert!(compressible_slope_ratio(9.0, 0.5, 1.2).is_none());
        assert!(compressible_slope_ratio(9.0, 0.5, -0.1).is_none());
        assert!(compressible_slope_ratio(9.0, 0.5, f64::NAN).is_none());
        assert!(compressible_slope_ratio(0.0, 0.5, 0.5).is_none());
        assert!(compressible_slope_ratio(9.0, f64::INFINITY, 0.5).is_none());
    }
}
