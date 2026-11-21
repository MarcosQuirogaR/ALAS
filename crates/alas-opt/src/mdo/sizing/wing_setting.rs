// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing setting angle of a clean-sheet transport.
//!
//! A clean-sheet candidate keeps the registered root incidence of the preset it
//! starts from, which is the incidence of another wing. The setting angle is a
//! consequence of the wing, not an input: it is the angle between the fuselage
//! reference line and the wing root chord that puts the fuselage at its intended
//! attitude at the design cruise lift coefficient, Raymer, *Aircraft Design: A
//! Conceptual Approach* (6th ed.), Ch. 4 "Wing incidence", pp. 87-88 ("the incidence
//! angle is chosen such that when the wing is at the correct angle of attack for the
//! selected design condition, the fuselage is at the angle of attack for minimum total
//! drag"); Jenkinson, *Civil Jet Aircraft Design* (1999), "Wing/body setting" ("the
//! fuselage is at its minimum drag attitude at the mid-cruise condition. This
//! condition dictates the wing/body setting angle"); Torenbeek, *Synthesis of Subsonic
//! Airplane Design* (1982), Ch. 3, p. 73 (the cabin floor level in the normal cruise
//! attitude).
//!
//! The candidate is trimmed once with the incidences it was built with. The wing is
//! then rotated rigidly about its root (root, break and tip incidence move together,
//! so the washout is unchanged) by the difference between that trimmed body attitude
//! and the target one, and trimmed again. A rigid rotation of the wing leaves the lift
//! coefficient, the lift-to-drag ratio and the tail load of the trim unchanged to first
//! order and moves the body attitude by the opposite angle.
//!
//! Target attitude. Two plausibility windows meet in the setting angle: the body
//! attitude `[a_min, a_max]` and the root incidence `[i_min, i_max]`. With `c` the sum of
//! the built root incidence and the trimmed body attitude (which a rigid rotation keeps),
//! a target `u` leaves the root at `c - u`; the target that keeps the aircraft farthest
//! inside both windows is the maximin of the four slacks, `u = (A + B) / 2` with
//! `A = max(a_min, c - i_max)` and `B = min(a_max, c - i_min)`.

use alas_config::AlasConfig;

/// Rotations smaller than this, degrees, are not applied: far below the resolution of
/// the trim (a tenth of a degree is the resolution of the design variables).
pub(super) const ROTATION_TOLERANCE_DEG: f64 = 1.0e-3;

/// Whether the setting angle is derived for this configuration: a clean-sheet
/// passenger transport with the planform constraints on, the same gate as the
/// body-attitude window residual.
pub(super) fn applies(config: &AlasConfig) -> bool {
    config.optimizer.design_space.mode == alas_config::DesignMode::CleanSheet
        && config.requirements.aircraft_type == "passenger"
        && config
            .optimizer
            .weights
            .transport_planform_constraints_enabled
}

/// The target body attitude, degrees, for a wing built with root incidence
/// `root_deg` that trims at body attitude `alpha_deg`: the maximin of the slacks of the
/// body-attitude window and the root-incidence window.
pub(super) fn target_body_alpha_deg(
    root_deg: f64,
    alpha_deg: f64,
    alpha_window_deg: (f64, f64),
    incidence_window_deg: (f64, f64),
) -> f64 {
    let c = root_deg + alpha_deg;
    let lower = alpha_window_deg.0.max(c - incidence_window_deg.1);
    let upper = alpha_window_deg.1.min(c - incidence_window_deg.0);
    0.5 * (lower + upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_the_maximin_of_the_two_windows() {
        let alpha = (0.0, 4.0);
        let incidence = (0.0, 6.5);
        // E195-E2 clean sheet: root 3.0 deg, trimmed body attitude -0.96 deg: the
        // incidence floor is the binding side, c = 2.04, so the target is c / 2.
        let target = target_body_alpha_deg(3.0, -0.96, alpha, incidence);
        assert!((target - 1.02).abs() < 1e-12, "{target}");
        let root_after = 3.0 + (-0.96 - target);
        assert!((target - root_after).abs() < 1e-12);
        // A wing that trims high keeps the attitude ceiling and the incidence ceiling
        // symmetric: c = 9.5.
        let target = target_body_alpha_deg(5.5, 4.0, alpha, incidence);
        let root_after = 5.5 + (4.0 - target);
        assert!(((4.0 - target) - (6.5 - root_after)).abs() < 1e-12);
        // Both windows are met by the result whenever they can be.
        for (root, a) in [(3.0, 0.5), (2.0, 3.0), (4.5, -1.0), (1.0, 2.0)] {
            let u = target_body_alpha_deg(root, a, alpha, incidence);
            let i = root + a - u;
            assert!(
                (0.0..=4.0).contains(&u) && (0.0..=6.5).contains(&i),
                "{root} {a}"
            );
        }
    }
}
