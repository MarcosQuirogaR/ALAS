// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ground effect for take-off and landing (UAV-PLAN 3.1 item 7).
//!
//! `h` is the height of the wing quarter-chord above the ground plane, `b`
//! the span, both in metres; `h/b >= 0`.  The induced-drag reduction factor
//! `phi = K_ground / K` (so `CDi_ground = phi CDi_free` at the same `CL`) has
//! three published forms:
//!
//! * [`GroundModel::Wieselsberger`]: `phi = 1 - sigma`, `sigma = (1 - 1.32 h/b)
//!   / (1.05 + 7.4 h/b)`.  Source: C. Wieselsberger, "Wing Resistance Near the
//!   Ground", NACA TM-77 (1922), NTRS 19930081293, read 2026-10-12: the
//!   image-vortex derivation, `Delta CD = -sigma CL^2 / (pi A)` (Eq. 3/5) and
//!   Prandtl's approximation of the influence coefficient `sigma`, Eq. 4,
//!   quoted there for the wing-to-image distance `h' = 2h` between `b/15` and
//!   `b/2`, i.e. `h/b` from 1/30 to 1/4 in the wing height used here.  The
//!   scan's OCR of Eq. 4 is degraded (the constants 1.05 and 3.7 are legible,
//!   the 0.66 is inferred); the formula in `h' = 2h` form reproduces the
//!   report's worked value `sigma = 0.452` at `h'/b = 0.242` to 4.5 % and the
//!   image-system VLM test below.  Clamped to `[0, 1]` outside the fit range.
//! * [`GroundModel::McCormick`]: `phi = (16 h/b)^2 / (1 + (16 h/b)^2)`
//!   (McCormick, after Wieselsberger 1922).  NOT verified on a page in this
//!   project: the formula comes from the plan and from memory of McCormick
//!   (the book is not in the "ALAS Handbooks" notebook).
//! * [`GroundModel::Raymer`]: `phi = 33 (h/b)^1.5 / (1 + 33 (h/b)^1.5)`,
//!   Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Eq. (12.60)
//!   (PDF page 485, read on the page 2026-10-12; the book's p. 810 remark
//!   that phi ~ 0.27 at h/b = 0.05 is reproduced by this form).
//!
//! Lift: at a fixed angle of attack the reduced downwash raises the lift
//! slope.  With lifting-line slope `1/a = 1/a0 + 1/(pi A e)` the ground slope
//! is `1/a_g = 1/a0 + phi/(pi A e) = 1/a - (1 - phi)/(pi A e)`; the zero-lift
//! angle is unchanged, so `CL_g = CL_free a_g / a`.  This is a lifting-line
//! derivation, not a published correlation; the image-system VLM test in
//! `tests/uav_aero_ground.rs` is its verification.
//!
//! # Verification against the exact image system (rectangular AR 8, c = 0.25 m)
//!
//! The ground replaced by the mirrored wing in the production VLM (8x4 mesh,
//! symmetric section, `tests/uav_aero_ground.rs`) gives the K-factor ratio
//! `phi` (equal `CL`) of 0.443 / 0.594 / 0.690 / 0.805 / 0.921 / 0.975 at
//! `h/b` = 0.05 / 0.1 / 0.15 / 0.25 / 0.5 / 1.0.  Raymer Eq. 12.60 is within
//! 5 % from `h/b` = 0.15 up (0.657, 0.805, 0.921, 0.971); Wieselsberger-Prandtl
//! is 9 % low at 0.15 and within 5 % from 0.25; McCormick `(16 h/b)^2` form
//! is 21 % high at 0.1 (it under-predicts the ground effect on drag) and is
//! therefore NOT the default.  Below `h/b` ~ 0.1 (`h/c` < 1) all three are low
//! (the VLM chord is finite; the image bound vortex also adds lift) and the
//! lift increment is the largest miss: at `h/b` = 0.05 the VLM raises `CL` by
//! 38 % at fixed angle, this lifting-line correction predicts 19 %.  Use the
//! image-system VLM, not these formulas, for rotation at `h/c` < 1.
//! Recommended model: [`GroundModel::Raymer`].
//!
//! Not modelled: ground effect on `CLmax` (none is applied), on the pitching
//! moment, or on a tail separately (call the same functions with the tail
//! geometry).  Valid for `h/b` roughly 0.05..1; below 0.05 the wing is on the
//! ground and the formulas only tend to `phi -> 0`.

use super::AeroError;

/// Which induced-drag factor form to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundModel {
    /// `1 - (1 - 1.32 h/b)/(1.05 + 7.4 h/b)`, Wieselsberger 1922 / Prandtl.
    Wieselsberger,
    /// `(16 h/b)^2 / (1 + (16 h/b)^2)`, McCormick.
    McCormick,
    /// `33 (h/b)^1.5 / (1 + 33 (h/b)^1.5)`, Raymer Eq. 12.60.
    Raymer,
}

/// Induced-drag factor `phi` in `[0, 1)` at relative height `h_over_b`.
///
/// # Errors
///
/// [`AeroError::InvalidInput`] for a negative or non-finite height.
pub fn induced_drag_factor(model: GroundModel, h_over_b: f64) -> Result<f64, AeroError> {
    if !(h_over_b.is_finite() && h_over_b >= 0.0) {
        return Err(AeroError::InvalidInput(format!(
            "h/b must be finite and non-negative, got {h_over_b}"
        )));
    }
    Ok(match model {
        GroundModel::Wieselsberger => {
            let sigma = (1.0 - 1.32 * h_over_b) / (1.05 + 7.4 * h_over_b);
            (1.0 - sigma).clamp(0.0, 1.0)
        }
        GroundModel::McCormick => {
            let x = (16.0 * h_over_b).powi(2);
            x / (1.0 + x)
        }
        GroundModel::Raymer => {
            let x = 33.0 * h_over_b.powf(1.5);
            x / (1.0 + x)
        }
    })
}

/// A wing near the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundEffect {
    /// Correlation.
    pub model: GroundModel,
    /// Quarter-chord height above the ground, m.
    pub height_m: f64,
    /// Span, m.
    pub span_m: f64,
}

/// Free-air state that the ground correction acts on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeAir {
    /// Lift coefficient at the angle of attack of interest.
    pub cl: f64,
    /// Induced-drag coefficient (Trefftz) at that angle.
    pub cd_induced: f64,
    /// Lift slope of the wing, 1/rad.
    pub cl_alpha_per_rad: f64,
    /// Aspect ratio `b^2/S`.
    pub aspect_ratio: f64,
    /// Span efficiency `CL^2/(pi A CDi)`.
    pub span_efficiency: f64,
}

/// Result of the correction at a fixed angle of attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPoint {
    /// Induced-drag factor used.
    pub phi: f64,
    /// Lift slope in ground effect, 1/rad.
    pub cl_alpha_per_rad: f64,
    /// Lift coefficient at the same angle of attack.
    pub cl: f64,
    /// Induced-drag coefficient at the same angle of attack.
    pub cd_induced: f64,
}

impl GroundEffect {
    /// `h/b`.
    pub fn h_over_b(&self) -> f64 {
        self.height_m / self.span_m
    }

    /// Induced-drag factor `phi`.
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] for a bad height or span.
    pub fn phi(&self) -> Result<f64, AeroError> {
        if !(self.span_m.is_finite() && self.span_m > 0.0) {
            return Err(AeroError::InvalidInput("span must be positive".into()));
        }
        induced_drag_factor(self.model, self.h_over_b())
    }

    /// Induced drag at the same `CL` as free air: `phi CDi`.
    ///
    /// # Errors
    ///
    /// See [`Self::phi`].
    pub fn cd_induced_at_fixed_cl(&self, cd_induced_free: f64) -> Result<f64, AeroError> {
        Ok(self.phi()? * cd_induced_free)
    }

    /// Lift and induced drag at the same angle of attack as `free`.
    ///
    /// `CDi_g = phi CL_g^2/(pi A e)`: with `CL_g = CL a_g/a` this is
    /// `phi (a_g/a)^2 CDi_free`.
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] when the slope is not below the
    /// lifting-line bound `pi A e` (the wing would have no induced
    /// downwash to reduce), or for a bad geometry.
    pub fn at_fixed_alpha(&self, free: &FreeAir) -> Result<GroundPoint, AeroError> {
        let phi = self.phi()?;
        let pi_a_e = std::f64::consts::PI * free.aspect_ratio * free.span_efficiency;
        let a = free.cl_alpha_per_rad;
        if !(a.is_finite() && a > 0.0 && pi_a_e.is_finite() && a < pi_a_e) {
            return Err(AeroError::InvalidInput(format!(
                "lift slope {a} must be positive and below pi A e = {pi_a_e}"
            )));
        }
        let a_g = 1.0 / (1.0 / a - (1.0 - phi) / pi_a_e);
        let ratio = a_g / a;
        Ok(GroundPoint {
            phi,
            cl_alpha_per_rad: a_g,
            cl: free.cl * ratio,
            cd_induced: phi * ratio * ratio * free.cd_induced,
        })
    }
}
