// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Term-by-term decomposition of the CS 25.121(b) second-segment gradient.
//!
//! The steady one-engine-inoperative climb at `V2` balances (small angle,
//! lift equals weight)
//!
//! ```text
//! gamma = (N-1)/N * r * (T/W)_SLS  -  CD / CL
//! CD    = CD0_clean + dCD_flap + dCD_windmill + dCD_trim + k CL^2
//! ```
//!
//! with `N` the installed engine count, `r` the thrust of an operating
//! engine at `V2` over its rated sea-level-static thrust, `(T/W)_SLS` the
//! installed all-engine static thrust over the take-off weight, and `CL`
//! the lift coefficient at `V2`. This module reports every term so a
//! shortfall against the 2.4 / 2.7 / 3.0 % requirement can be assigned to
//! thrust, to one of the drag increments, or to the lift coefficient the
//! speed rule produces. Dimensionless; gradients are rise over distance.

/// Inputs of one second-segment point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientInputs {
    /// Installed engine count (2, 3 or 4).
    pub n_engines: i64,
    /// Installed all-engine static thrust over take-off weight.
    pub installed_tw_sls: f64,
    /// Operating-engine thrust at `V2` over rated static thrust.
    pub thrust_ratio: f64,
    /// Lift coefficient at `V2`.
    pub cl: f64,
    /// Zero-lift drag of the clean airframe at the `V2` Mach and Reynolds.
    pub cd0_clean: f64,
    /// Lift-dependent drag factor `k` of the clean polar at `CL` (`CD - CD0 = k CL^2`).
    pub induced_factor: f64,
    /// Take-off flap and slat increment, gear up.
    pub flap_delta_cd: f64,
    /// Failed-engine (windmilling or feathered) increment.
    pub windmilling_cd: f64,
    /// Asymmetric trim increment.
    pub trim_cd: f64,
}

/// Every term of the gradient balance, plus the verdict against a requirement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientDecomposition {
    /// `(N-1)/N * r * (T/W)_SLS`: the thrust available per unit weight.
    pub thrust_term: f64,
    /// Lift-dependent drag `k CL^2`.
    pub induced_cd: f64,
    /// Total drag coefficient.
    pub cd_total: f64,
    /// Lift-to-drag ratio at `V2`, one engine out.
    pub l_over_d: f64,
    /// Drag per unit lift of each contribution, `dCD/CL`: clean zero-lift,
    /// flaps, failed engine, trim, induced.
    pub drag_over_lift: [f64; 5],
    /// Available gradient `thrust_term - CD/CL`.
    pub gradient: f64,
    /// Effective Oswald factor `CL^2 / (pi A k CL^2)` is not formed here
    /// (no aspect ratio); see [`oswald_factor`].
    pub k: f64,
}

/// Decompose the second-segment balance. `None` for non-finite or
/// non-physical inputs (fewer than two engines, non-positive `CL`).
pub fn decompose_gradient(inputs: &GradientInputs) -> Option<GradientDecomposition> {
    let values = [
        inputs.installed_tw_sls,
        inputs.thrust_ratio,
        inputs.cl,
        inputs.cd0_clean,
        inputs.induced_factor,
        inputs.flap_delta_cd,
        inputs.windmilling_cd,
        inputs.trim_cd,
    ];
    if inputs.n_engines < 2 || values.iter().any(|v| !v.is_finite() || *v < 0.0) || inputs.cl <= 0.0
    {
        return None;
    }
    let n = inputs.n_engines as f64;
    let thrust_term = (n - 1.0) / n * inputs.thrust_ratio * inputs.installed_tw_sls;
    let induced_cd = inputs.induced_factor * inputs.cl * inputs.cl;
    let cd_total = inputs.cd0_clean
        + inputs.flap_delta_cd
        + inputs.windmilling_cd
        + inputs.trim_cd
        + induced_cd;
    let per_lift = |cd: f64| cd / inputs.cl;
    Some(GradientDecomposition {
        thrust_term,
        induced_cd,
        cd_total,
        l_over_d: inputs.cl / cd_total,
        drag_over_lift: [
            per_lift(inputs.cd0_clean),
            per_lift(inputs.flap_delta_cd),
            per_lift(inputs.windmilling_cd),
            per_lift(inputs.trim_cd),
            per_lift(induced_cd),
        ],
        gradient: thrust_term - cd_total / inputs.cl,
        k: inputs.induced_factor,
    })
}

/// Oswald factor `e = 1 / (pi A k)` of a lift-dependent drag factor `k` on a
/// wing of aspect ratio `A`.
pub fn oswald_factor(aspect_ratio: f64, induced_factor: f64) -> Option<f64> {
    let e = 1.0 / (std::f64::consts::PI * aspect_ratio * induced_factor);
    (aspect_ratio > 0.0 && induced_factor > 0.0 && e.is_finite()).then_some(e)
}

#[cfg(test)]
mod tests {
    // A test asserts on values it built, so a failed unwrap is the assertion failing.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Hand calculation, A320-200 class (twin): installed T/W 0.3248,
    /// r = 0.744, CL 1.30, CD0 0.0210, flap 0.015, windmilling 0.0043,
    /// trim 0.0071, k 0.0446 (A 9.5, e 0.75).
    ///   thrust = 0.5 * 0.744 * 0.3248              = 0.120826
    ///   induced = 0.0446 * 1.69                    = 0.075374
    ///   CD = 0.0210+0.015+0.0043+0.0071+0.075374   = 0.122774
    ///   CD/CL = 0.122774/1.30                      = 0.094442
    ///   gradient = 0.120826 - 0.094442             = 0.026384
    #[test]
    fn the_a320_class_balance_matches_the_hand_calculation() {
        let d = decompose_gradient(&GradientInputs {
            n_engines: 2,
            installed_tw_sls: 0.3248,
            thrust_ratio: 0.744,
            cl: 1.30,
            cd0_clean: 0.0210,
            induced_factor: 0.0446,
            flap_delta_cd: 0.015,
            windmilling_cd: 0.0043,
            trim_cd: 0.0071,
        })
        .unwrap();
        assert!((d.thrust_term - 0.120_826).abs() < 1e-6, "{d:?}");
        assert!((d.induced_cd - 0.075_374).abs() < 1e-6);
        assert!((d.cd_total - 0.122_774).abs() < 1e-6);
        assert!((d.gradient - 0.026_384).abs() < 1e-6);
        assert!((d.l_over_d - 1.30 / 0.122_774).abs() < 1e-9);
        let sum: f64 = d.drag_over_lift.iter().sum();
        assert!((sum - d.cd_total / 1.30).abs() < 1e-12);
    }

    /// Hand calculation, B787-9 class (twin): installed T/W 0.2715,
    /// r = 0.730, CL 1.46, CD0 0.0190, flap 0.015, windmilling 0.0039,
    /// trim 0.0057, k 0.0386 (A 9.6, e 0.86).
    ///   thrust = 0.5 * 0.730 * 0.2715               = 0.0991
    ///   induced = 0.0386 * 2.1316                   = 0.082280
    ///   CD = 0.0190+0.015+0.0039+0.0057+0.082280    = 0.125880
    ///   CD/CL = 0.125880/1.46                       = 0.086219
    ///   gradient = 0.0991 - 0.086219 = 0.012881 (below the 2.4 % requirement)
    #[test]
    fn the_b787_class_balance_shows_the_shortfall_against_two_point_four_percent() {
        let d = decompose_gradient(&GradientInputs {
            n_engines: 2,
            installed_tw_sls: 0.2715,
            thrust_ratio: 0.730,
            cl: 1.46,
            cd0_clean: 0.0190,
            induced_factor: 0.0386,
            flap_delta_cd: 0.015,
            windmilling_cd: 0.0039,
            trim_cd: 0.0057,
        })
        .unwrap();
        assert!((d.thrust_term - 0.099_097_5).abs() < 1e-6, "{d:?}");
        assert!((d.cd_total - 0.125_880).abs() < 5e-6);
        assert!((d.gradient - 0.012_88).abs() < 5e-5, "{d:?}");
        assert!(d.gradient < 0.024);
    }

    #[test]
    fn a_quad_loses_a_quarter_of_the_thrust_and_a_single_engine_is_rejected() {
        let mut inputs = GradientInputs {
            n_engines: 4,
            installed_tw_sls: 0.25,
            thrust_ratio: 0.8,
            cl: 1.3,
            cd0_clean: 0.02,
            induced_factor: 0.04,
            flap_delta_cd: 0.015,
            windmilling_cd: 0.002,
            trim_cd: 0.008,
        };
        let d = decompose_gradient(&inputs).unwrap();
        assert!((d.thrust_term - 0.75 * 0.8 * 0.25).abs() < 1e-15);
        inputs.n_engines = 1;
        assert!(decompose_gradient(&inputs).is_none());
        inputs.n_engines = 2;
        inputs.cl = f64::NAN;
        assert!(decompose_gradient(&inputs).is_none());
    }

    #[test]
    fn the_oswald_factor_inverts_the_induced_factor() {
        let e = oswald_factor(9.5, 0.0446).unwrap();
        assert!((e - 1.0 / (std::f64::consts::PI * 9.5 * 0.0446)).abs() < 1e-15);
        assert!(oswald_factor(0.0, 0.04).is_none());
    }
}
