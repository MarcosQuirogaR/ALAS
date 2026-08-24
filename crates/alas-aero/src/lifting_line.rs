// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent low-order finite-wing aerodynamics for model comparison.
//!
//! This is the classical, linear Prandtl lifting-line reduction for a nearly
//! elliptic, high-aspect-ratio, low-sweep wing in steady incompressible flow.
//! It deliberately does not reuse the vortex-lattice solver. MIT Unified
//! Engineering notes derive the finite-wing lift slope as
//! \(a=a_0/(1+a_0/(pi e AR))\), and MIT 16.100 derives
//! \(C_{D_i}=C_L^2/(pi e AR)\).
//!
//! References:
//! - MIT Unified Engineering, F9, "General Wings" (2004).
//! - MIT 16.100, Lectures 17-19, "Prandtl's Lifting Line" (2005).
//! - NASA/TP-2002-209032, the Helmbold finite-span lift-slope relation.
//!
//! The caller remains responsible for applicability. This model does not
//! represent sweep coupling, compressibility, stall, separation, wave drag,
//! control surfaces, fuselage interference, or propulsion-airframe effects.

use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

/// Inputs to the classical finite-wing lifting-line reduction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftingLineModel {
    /// Wing aspect ratio, \(b^2/S\).
    pub aspect_ratio: f64,
    /// Span efficiency in the induced-drag relation.
    pub span_efficiency: f64,
    /// Two-dimensional section lift-curve slope, per radian.
    pub section_lift_slope_per_rad: f64,
    /// Section zero-lift angle, radians.
    pub zero_lift_angle_rad: f64,
    /// Non-induced drag coefficient on the aircraft reference area.
    pub parasite_drag_coefficient: f64,
    /// Constant pitching-moment coefficient supplied by the caller.
    pub moment_coefficient: f64,
}

/// One point from the independent lifting-line model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftingLinePoint {
    /// Geometric angle of attack, radians.
    pub alpha_rad: f64,
    /// Whole-wing lift coefficient.
    pub lift_coefficient: f64,
    /// Induced drag coefficient from the finite-span wake.
    pub induced_drag_coefficient: f64,
    /// Parasite plus induced drag coefficient.
    pub drag_coefficient: f64,
    /// Lift-to-drag ratio, absent only when total drag is numerically zero.
    pub lift_to_drag: Option<f64>,
    /// Caller-supplied constant pitching-moment coefficient.
    pub moment_coefficient: f64,
}

/// Invalid inputs are rejected instead of being converted into plausible data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftingLineError {
    /// At least one input was NaN or infinite.
    NonFiniteInput,
    /// Aspect ratio was zero or negative.
    NonPositiveAspectRatio,
    /// Span efficiency was zero or negative.
    NonPositiveSpanEfficiency,
    /// The section lift-curve slope was zero or negative.
    NonPositiveSectionLiftSlope,
    /// The non-induced drag coefficient was negative.
    NegativeParasiteDrag,
}

impl Display for LiftingLineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NonFiniteInput => "all lifting-line inputs must be finite",
            Self::NonPositiveAspectRatio => "aspect ratio must be positive",
            Self::NonPositiveSpanEfficiency => "span efficiency must be positive",
            Self::NonPositiveSectionLiftSlope => "section lift-curve slope must be positive",
            Self::NegativeParasiteDrag => "parasite drag coefficient cannot be negative",
        };
        formatter.write_str(message)
    }
}

impl Error for LiftingLineError {}

/// Helmbold's incompressible finite-span lift-curve slope, per radian.
///
/// NASA/TP-2002-209032 gives this approximation for unswept rectangular
/// wings as \(C_{L_alpha}=2 pi AR/(2+sqrt(AR^2+4))\). It approaches the
/// two-dimensional \(2 pi\) slope at high aspect ratio and the Jones
/// \(pi AR/2\) limit at very low aspect ratio.
pub fn helmbold_lift_slope_per_rad(aspect_ratio: f64) -> Result<f64, LiftingLineError> {
    if !aspect_ratio.is_finite() {
        return Err(LiftingLineError::NonFiniteInput);
    }
    if aspect_ratio <= 0.0 {
        return Err(LiftingLineError::NonPositiveAspectRatio);
    }
    Ok(2.0 * PI * aspect_ratio / (2.0 + (aspect_ratio.powi(2) + 4.0).sqrt()))
}

impl LiftingLineModel {
    /// Validate all parameters before the model can be evaluated.
    pub fn validate(self) -> Result<Self, LiftingLineError> {
        let values = [
            self.aspect_ratio,
            self.span_efficiency,
            self.section_lift_slope_per_rad,
            self.zero_lift_angle_rad,
            self.parasite_drag_coefficient,
            self.moment_coefficient,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(LiftingLineError::NonFiniteInput);
        }
        if self.aspect_ratio <= 0.0 {
            return Err(LiftingLineError::NonPositiveAspectRatio);
        }
        if self.span_efficiency <= 0.0 {
            return Err(LiftingLineError::NonPositiveSpanEfficiency);
        }
        if self.section_lift_slope_per_rad <= 0.0 {
            return Err(LiftingLineError::NonPositiveSectionLiftSlope);
        }
        if self.parasite_drag_coefficient < 0.0 {
            return Err(LiftingLineError::NegativeParasiteDrag);
        }
        Ok(self)
    }

    /// Three-dimensional lift-curve slope, per radian.
    pub fn finite_wing_lift_slope_per_rad(self) -> Result<f64, LiftingLineError> {
        let model = self.validate()?;
        Ok(model.section_lift_slope_per_rad
            / (1.0
                + model.section_lift_slope_per_rad
                    / (PI * model.span_efficiency * model.aspect_ratio)))
    }

    /// Evaluate one attached-flow operating point.
    pub fn solve(self, alpha_rad: f64) -> Result<LiftingLinePoint, LiftingLineError> {
        if !alpha_rad.is_finite() {
            return Err(LiftingLineError::NonFiniteInput);
        }
        let model = self.validate()?;
        let slope = model.finite_wing_lift_slope_per_rad()?;
        let lift_coefficient = slope * (alpha_rad - model.zero_lift_angle_rad);
        let induced_drag_coefficient =
            lift_coefficient.powi(2) / (PI * model.span_efficiency * model.aspect_ratio);
        let drag_coefficient = model.parasite_drag_coefficient + induced_drag_coefficient;
        let lift_to_drag =
            (drag_coefficient > f64::EPSILON).then_some(lift_coefficient / drag_coefficient);
        Ok(LiftingLinePoint {
            alpha_rad,
            lift_coefficient,
            induced_drag_coefficient,
            drag_coefficient,
            lift_to_drag,
            moment_coefficient: model.moment_coefficient,
        })
    }

    /// Evaluate an angle sweep without changing the caller's ordering.
    pub fn sweep(self, alpha_rad: &[f64]) -> Result<Vec<LiftingLinePoint>, LiftingLineError> {
        alpha_rad.iter().map(|&alpha| self.solve(alpha)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classical_model() -> LiftingLineModel {
        LiftingLineModel {
            aspect_ratio: 8.0,
            span_efficiency: 1.0,
            section_lift_slope_per_rad: 2.0 * PI,
            zero_lift_angle_rad: -2.0_f64.to_radians(),
            parasite_drag_coefficient: 0.02,
            moment_coefficient: -0.05,
        }
    }

    #[test]
    fn finite_wing_slope_matches_the_classical_closed_form() {
        let Ok(slope) = classical_model().finite_wing_lift_slope_per_rad() else {
            panic!("classical model must be valid");
        };
        let expected = 2.0 * PI / (1.0 + 2.0 / 8.0);
        assert!((slope - expected).abs() <= 1e-14);
    }

    #[test]
    fn zero_lift_angle_produces_no_lift_or_induced_drag() {
        let model = classical_model();
        let Ok(point) = model.solve(model.zero_lift_angle_rad) else {
            panic!("zero-lift point must be valid");
        };
        assert_eq!(point.lift_coefficient, 0.0);
        assert_eq!(point.induced_drag_coefficient, 0.0);
        assert_eq!(point.drag_coefficient, model.parasite_drag_coefficient);
    }

    #[test]
    fn induced_drag_obeys_the_lifting_line_relation() {
        let model = classical_model();
        let Ok(point) = model.solve(5.0_f64.to_radians()) else {
            panic!("attached-flow point must be valid");
        };
        let expected =
            point.lift_coefficient.powi(2) / (PI * model.span_efficiency * model.aspect_ratio);
        assert!((point.induced_drag_coefficient - expected).abs() <= 1e-15);
        assert!(point.drag_coefficient >= point.induced_drag_coefficient);
    }

    #[test]
    fn helmbold_slope_has_the_correct_high_and_low_aspect_ratio_limits() {
        let Ok(high_aspect_ratio) = helmbold_lift_slope_per_rad(1.0e6) else {
            panic!("positive finite aspect ratio must be valid");
        };
        assert!((high_aspect_ratio - 2.0 * PI).abs() < 2.0e-5);

        let low_aspect_ratio = 1.0e-6;
        let Ok(low_slope) = helmbold_lift_slope_per_rad(low_aspect_ratio) else {
            panic!("positive finite aspect ratio must be valid");
        };
        assert!((low_slope / low_aspect_ratio - PI / 2.0).abs() < 1.0e-6);
    }

    #[test]
    fn helmbold_rejects_nonphysical_aspect_ratio() {
        assert_eq!(
            helmbold_lift_slope_per_rad(0.0),
            Err(LiftingLineError::NonPositiveAspectRatio)
        );
        assert_eq!(
            helmbold_lift_slope_per_rad(f64::NAN),
            Err(LiftingLineError::NonFiniteInput)
        );
    }

    #[test]
    fn invalid_inputs_are_not_silently_coerced() {
        let mut model = classical_model();
        model.aspect_ratio = 0.0;
        assert_eq!(
            model.validate(),
            Err(LiftingLineError::NonPositiveAspectRatio)
        );
        model = classical_model();
        model.parasite_drag_coefficient = -0.01;
        assert_eq!(
            model.validate(),
            Err(LiftingLineError::NegativeParasiteDrag)
        );
        assert_eq!(
            classical_model().solve(f64::NAN),
            Err(LiftingLineError::NonFiniteInput)
        );
    }
}
