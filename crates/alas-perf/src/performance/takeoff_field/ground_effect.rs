// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ground effect on the lift-dependent drag of the take-off wing.
//!
//! A continued take-off is flown, and demonstrated, in ground effect up to a
//! wing height of one span: CS 25.111(d)(3)-(4) lets a segmental path ignore
//! the effect only because the path "must be checked by continuous
//! demonstrated take-offs up to the point at which the aeroplane is out of
//! ground effect", which "is considered to be out of the ground effect when
//! it reaches a height equal to its wing span". The 35 ft screen of
//! CS 25.113 lies inside that region, and Torenbeek (*Advanced Aircraft
//! Design*, 2013, Sec. 9.3.2, p. 266) states that the first segment "benefits
//! from ground effect".
//!
//! Source, read from the page (Torenbeek, *Synthesis of Subsonic Airplane
//! Design*, 1982, Appendix G-7.2, p. 553, and Fig. G-26, p. 554):
//!
//! ```text
//! C_Dv = (1 - sigma) / (1 - beta C_L / (4 pi h / c_g)) * (C_Dv)_inf      (G-77)
//! sigma = exp(-2.48 (2 h_eff / b)^0.768)
//! beta  = sqrt(1 + (2 h_eff / b)^2) - 2 h_eff / b                       (Fig. G-26)
//! ```
//!
//! with `h` the height of the wing aerodynamic centre above the ground, `c_g`
//! the mean geometric chord, `b` the span and `2 h_eff = h_ac + h_TE` (the
//! sum of the aerodynamic-centre and trailing-edge heights): the same two
//! heights, and the same `2 h_eff`, as the ground-effect lift ratio of G-76
//! that sets the lift-off attitude, so the drag and the lift of the rotation
//! see one wing over one ground plane. In the rotation the trailing edge,
//! aft of the main wheels, drops as the nose rises while the aerodynamic
//! centre, ahead of them, rises ([`WingRise`]).
//! `(1 - sigma)` is the Wieselsberger reduction of the induced angle
//! (G-72); the denominator is the loss of effective dynamic pressure at the
//! wing from the image of the bound vortex (G-68).
//!
//! Cross-check on the page of the sixth edition of Raymer, *Aircraft Design:
//! A Conceptual Approach*, Eq. (12.60), p. 451: `K_eff / K = 33 (h/b)^1.5 /
//! (1 + 33 (h/b)^1.5)`. The two agree within 0.08 over `0.05 <= h/b <= 0.5`
//! (the unit test below).
//!
//! The wing profile drag is not reduced (G-78 does reduce it, in proportion
//! to the wing's profile share, which the aircraft drag build does not
//! split): conservative, bounded by `beta C_L / (4 pi h / c_g)` times the
//! wing profile drag, at most 0.002 of CD at lift-off.

use std::f64::consts::PI;

/// Coefficient of `sigma` in Fig. G-26 (Torenbeek 1982, p. 554).
const SIGMA_COEFFICIENT: f64 = 2.48;
/// Exponent of `sigma` in Fig. G-26.
const SIGMA_EXPONENT: f64 = 0.768;
/// Largest `beta C_L / (4 pi h / c_g)` accepted: the ground-effect form
/// (G-77) is a small-correction expansion in the image-vortex velocity.
const MAX_VELOCITY_LOSS: f64 = 0.5;

/// The wing of the aircraft over the ground, for [`Self::induced_drag_ratio`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundEffect {
    /// Wing span `b`, m.
    pub span_m: f64,
    /// Mean geometric chord `c_g = S / b`, m.
    pub mean_chord_m: f64,
    /// Height of the wing aerodynamic centre above the ground with the
    /// aircraft on its gear at the take-off weight (level attitude), m.
    pub wing_height_m: f64,
    /// Height of the wing trailing edge (MAC) above the ground in the same
    /// position, m: with [`Self::wing_height_m`] the `2 h_eff` of G-77.
    pub trailing_edge_height_m: f64,
}

/// Rise of the wing's two reference points over their static, level heights,
/// m: the aerodynamic centre and the trailing edge (MAC). Pitching about the
/// main-wheel contact raises a point ahead of the wheels and lowers one aft
/// of them, and the shock-absorber extension raises both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingRise {
    /// Rise of the aerodynamic centre, m.
    pub aerodynamic_centre_m: f64,
    /// Rise of the trailing edge, m (negative when it drops).
    pub trailing_edge_m: f64,
}

impl WingRise {
    /// Both points `height_m` above their static, level heights (a level
    /// translation: the climb above the lift-off position).
    #[must_use]
    pub const fn level(height_m: f64) -> Self {
        Self {
            aerodynamic_centre_m: height_m,
            trailing_edge_m: height_m,
        }
    }

    /// This rise with both points a further `height_m` up.
    #[must_use]
    pub fn raised(self, height_m: f64) -> Self {
        Self {
            aerodynamic_centre_m: self.aerodynamic_centre_m + height_m,
            trailing_edge_m: self.trailing_edge_m + height_m,
        }
    }
}

/// Torenbeek's `sigma` (Fig. G-26) at `2 h_eff / b`.
pub fn ground_effect_sigma(two_h_over_b: f64) -> f64 {
    (-SIGMA_COEFFICIENT * two_h_over_b.max(0.0).powf(SIGMA_EXPONENT)).exp()
}

/// Torenbeek's `beta` (Fig. G-26) at `2 h_eff / b`.
pub fn ground_effect_beta(two_h_over_b: f64) -> f64 {
    (1.0 + two_h_over_b * two_h_over_b).sqrt() - two_h_over_b
}

impl GroundEffect {
    /// Validated geometry: span, mean geometric chord, and the static, level
    /// heights of the aerodynamic centre and of the trailing edge; `None` for
    /// non-finite or non-positive values.
    pub fn new(
        span_m: f64,
        mean_chord_m: f64,
        wing_height_m: f64,
        trailing_edge_height_m: f64,
    ) -> Option<Self> {
        [span_m, mean_chord_m, wing_height_m, trailing_edge_height_m]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            .then_some(Self {
                span_m,
                mean_chord_m,
                wing_height_m,
                trailing_edge_height_m,
            })
    }

    /// Ratio of the vortex-induced drag in ground effect to that in free air
    /// at the same lift coefficient `cl`, with the wing's two points `rise`
    /// above their static, level heights: (G-77), `sigma` and `beta` at
    /// `2 h_eff = h_ac + h_TE`, the velocity loss at `h_ac`. In `(0, 1.x]`;
    /// the denominator can lift it above `1 - sigma` at low height and high
    /// `cl`. No point is taken below the ground.
    pub fn induced_drag_ratio(&self, cl: f64, rise: WingRise) -> f64 {
        let h_ac = (self.wing_height_m + rise.aerodynamic_centre_m).max(1e-6);
        let h_te = (self.trailing_edge_height_m + rise.trailing_edge_m).max(0.0);
        let two_h_over_b = (h_ac + h_te) / self.span_m;
        let sigma = ground_effect_sigma(two_h_over_b);
        let beta = ground_effect_beta(two_h_over_b);
        let loss = (beta * cl.max(0.0) / (4.0 * PI * h_ac / self.mean_chord_m))
            .clamp(0.0, MAX_VELOCITY_LOSS);
        (1.0 - sigma) / (1.0 - loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raymer (6th ed.) Eq. (12.60), p. 451.
    fn raymer_ratio(height_over_span: f64) -> f64 {
        let x = 33.0 * height_over_span.powf(1.5);
        x / (1.0 + x)
    }

    #[test]
    fn torenbeek_and_raymer_agree_over_the_take_off_heights() {
        for h_over_b in [0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5] {
            let torenbeek = 1.0 - ground_effect_sigma(2.0 * h_over_b);
            let raymer = raymer_ratio(h_over_b);
            assert!(
                (torenbeek - raymer).abs() < 0.08,
                "h/b {h_over_b}: Torenbeek {torenbeek:.3} Raymer {raymer:.3}"
            );
        }
    }

    #[test]
    fn the_limits_are_no_effect_far_away_and_a_strong_effect_near_the_ground() {
        let wing = GroundEffect::new(34.1, 3.6, 3.0, 3.0).unwrap_or_else(|| panic!("valid wing"));
        let at = |cl: f64, height_m: f64| wing.induced_drag_ratio(cl, WingRise::level(height_m));
        // A wing span above the ground: the free-air value to within 8 percent.
        let high = at(1.0, 34.1);
        assert!(high > 0.92 && high < 1.02, "{high}");
        // On the ground the induced drag is about half.
        let low = at(1.0, 0.0);
        assert!(low > 0.35 && low < 0.7, "{low}");
        // Monotone with height at fixed lift, and with lift at fixed height
        // (the image-vortex velocity loss of the denominator).
        let heights = [0.0, 3.0, 6.0, 11.0, 20.0];
        assert!(heights.windows(2).all(|w| at(1.5, w[0]) < at(1.5, w[1])));
        assert!(at(2.0, 0.0) > at(0.5, 0.0));
        assert!(GroundEffect::new(34.1, 3.6, -1.0, 3.0).is_none());
        assert!(GroundEffect::new(34.1, 3.6, 3.0, 0.0).is_none());
        assert!(GroundEffect::new(f64::NAN, 3.6, 3.0, 3.0).is_none());
    }

    /// `2 h_eff = h_ac + h_TE`: a trailing edge below the aerodynamic centre
    /// (a pitched-up wing) gains more from the ground than the level wing at
    /// the aerodynamic-centre height, and dropping the trailing edge of the
    /// level wing by the same amount reads the same ratio.
    #[test]
    fn the_effective_height_is_the_mean_of_the_two_points() {
        let level = GroundEffect::new(34.1, 3.6, 3.0, 3.0).unwrap_or_else(|| panic!("valid"));
        let pitched = GroundEffect::new(34.1, 3.6, 3.0, 2.0).unwrap_or_else(|| panic!("valid"));
        let still = WingRise::level(0.0);
        assert!(pitched.induced_drag_ratio(1.0, still) < level.induced_drag_ratio(1.0, still));
        let drop = WingRise {
            aerodynamic_centre_m: 0.0,
            trailing_edge_m: -1.0,
        };
        let (a, b) = (
            level.induced_drag_ratio(1.0, drop),
            pitched.induced_drag_ratio(1.0, still),
        );
        assert!((a - b).abs() < 1e-12, "{a} {b}");
    }
}
