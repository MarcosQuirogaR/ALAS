// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closed-form lifting-surface estimates used by the simple neutral-point
//! method: lift-curve slope, downwash, control-surface effectiveness.
//!
//! Sources (page and equation numbers refer to the cited editions):
//!
//! * Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Eq. 12.6-12.9,
//!   p. 400 (wing lift-curve slope, fuselage lift factor).
//! * Sadraey, Aircraft Design: A Systems Engineering Approach, Eq. 6.54-6.56,
//!   p. 335 (downwash at zero angle of attack and its slope).
//! * Sadraey, Fig. 12.12, p. 659 (control-surface angle-of-attack
//!   effectiveness `tau` against chord ratio), digitised by eye (about
//!   +/-0.03); the text of Example 12.4 states `tau = 0.41` at `c_f/c = 0.2`.

use super::AirframeError;

/// Raymer Eq. 12.6 without the exposed-area and fuselage factors:
///
/// `CLa = 2 pi A / (2 + sqrt(4 + A^2 beta^2 / eta^2 (1 + tan^2(L_maxt) / beta^2)))`
///
/// per radian, with `beta^2 = 1 - M^2`, `eta` the airfoil efficiency (Raymer:
/// about 0.95 when the section slope is unknown) and `L_maxt` the sweep of the
/// line of maximum thickness.
///
/// # Errors
///
/// Non-positive aspect ratio or efficiency, or `M >= 1`.
pub fn lift_curve_slope(
    aspect_ratio: f64,
    mach: f64,
    sweep_max_t_rad: f64,
    airfoil_efficiency: f64,
) -> Result<f64, AirframeError> {
    let positive = |x: f64| x.is_finite() && x > 0.0;
    if !positive(aspect_ratio) || !positive(airfoil_efficiency) || !(0.0..1.0).contains(&mach) {
        return Err(AirframeError::Invalid(
            "lift_curve_slope needs AR > 0, eta > 0 and 0 <= M < 1".into(),
        ));
    }
    let beta2 = 1.0 - mach * mach;
    let t = sweep_max_t_rad.tan();
    let root = (4.0
        + aspect_ratio * aspect_ratio * beta2 / (airfoil_efficiency * airfoil_efficiency)
            * (1.0 + t * t / beta2))
        .sqrt();
    Ok(2.0 * std::f64::consts::PI * aspect_ratio / (2.0 + root))
}

/// Raymer Eq. 12.9: fuselage lift factor `F = 1.07 (1 + d/b)^2`.
#[must_use]
pub fn fuselage_lift_factor(diameter_over_span: f64) -> f64 {
    1.07 * (1.0 + diameter_over_span).powi(2)
}

/// Downwash gradient `d eps / d alpha = 2 CLa_w / (pi AR_w)` (Sadraey Eq. 6.56,
/// an elliptic-loading far-field approximation: it over-predicts for a tail
/// within a few chords of the wing; Raymer Fig. 16.12 gives 0.38 for the DR-1
/// where this gives 0.52).
#[must_use]
pub fn downwash_gradient(cl_alpha_wing: f64, aspect_ratio_wing: f64) -> f64 {
    2.0 * cl_alpha_wing / (std::f64::consts::PI * aspect_ratio_wing)
}

/// Downwash angle at the tail for a wing lift coefficient `cl_w`,
/// `eps = 2 CL_w / (pi AR_w)` rad (Sadraey Eq. 6.55).
#[must_use]
pub fn downwash_angle(cl_wing: f64, aspect_ratio_wing: f64) -> f64 {
    2.0 * cl_wing / (std::f64::consts::PI * aspect_ratio_wing)
}

/// `(c_f / c, tau)` read from Sadraey Fig. 12.12 (angle-of-attack
/// effectiveness of a plain control surface).  ESTIMATE: eyeballed curve.
const TAU_TABLE: [(f64, f64); 8] = [
    (0.0, 0.0),
    (0.1, 0.27),
    (0.2, 0.41),
    (0.3, 0.51),
    (0.4, 0.60),
    (0.5, 0.66),
    (0.6, 0.73),
    (0.7, 0.80),
];

/// Control-surface effectiveness `tau = d alpha_eff / d delta` from
/// [`TAU_TABLE`] (piecewise linear), for a chord ratio in `[0, 0.7]`.
///
/// # Errors
///
/// A chord ratio outside `[0, 0.7]` (the figure's range).
pub fn control_effectiveness(chord_ratio: f64) -> Result<f64, AirframeError> {
    if !(0.0..=0.7).contains(&chord_ratio) {
        return Err(AirframeError::Invalid(format!(
            "control chord ratio {chord_ratio} is outside the 0..0.7 range of the effectiveness chart"
        )));
    }
    for w in TAU_TABLE.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if chord_ratio <= x1 {
            return Ok(y0 + (y1 - y0) * (chord_ratio - x0) / (x1 - x0));
        }
    }
    Ok(TAU_TABLE[TAU_TABLE.len() - 1].1)
}
