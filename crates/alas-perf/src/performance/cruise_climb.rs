// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise-altitude thrust requirements, written on the installed
//! sea-level-static thrust-to-weight axis with the thrust the engine deck
//! delivers at the cruise condition instead of one flat lapse constant.
//!
//! Two requirements, both flown at the design cruise Mach and altitude with
//! the engines at the maximum-climb rating:
//!
//! - **Level flight** at the mid-cruise weight, the weight the design cruise
//!   point is flown at: `T_MCL >= D`, i.e. `(T/W)_SLS >= f (D/W') / r` with
//!   `r = F_MCL(h, M) / F_SLS`, `f` the mid-cruise over take-off mass ratio
//!   and `D/W'` the drag over weight at the mid-cruise mass. The design
//!   altitude is the representative (mid-cruise) level of a step-climbing
//!   aircraft, not its initial level, so pairing it with the take-off weight
//!   (the initial-cruise convention of Torenbeek 2013, Eq. 9.3) overstates the
//!   thrust demand by the burned fuel.
//! - **Top of climb**, the initial-cruise-altitude capability: a residual
//!   climb rate of 300 ft/min at the top-of-climb mass, held at constant true
//!   airspeed, `(T - D) V / W >= ROC`, i.e.
//!   `(T/W)_SLS >= f (D/W' + ROC / V) / r` with `f` the top-of-climb to
//!   take-off mass ratio and `D/W'` the drag over the weight at that mass.
//!
//! The 300 ft/min criterion and the 0.985 mass ratio are the values the
//! mission model's initial-cruise-level search already uses
//! (`mission_model::cruise_levels`), which now takes them from here, so the
//! constraint and the flown level cannot disagree. Torenbeek (Advanced
//! Aircraft Design, 2013, Sec. 9.2) sizes the cruise engines the same way:
//! the initial cruise altitude capability is attained when a specified rate
//! of climb is achievable at maximum climb rating (200 ft/min is about 0.4 %
//! gradient, 8 % of the cruise thrust).

/// Residual rate of climb at maximum-climb thrust that defines the highest
/// usable cruise altitude, m/s: 300 ft/min (ICAO-style service-ceiling
/// convention; Airbus "Getting to Grips with Aircraft Performance", 2002,
/// "Maximum altitude"; Boeing FCOM performance data use the same residual).
pub const TOC_RESIDUAL_CLIMB_M_S: f64 = 300.0 * 0.3048 / 60.0;

/// Top-of-climb mass over brake-release mass: Raymer, "Aircraft Design: A
/// Conceptual Approach", 6th ed., Sec. 6.3, climb and accelerate fraction.
pub const TOC_MASS_FRACTION: f64 = 0.985;

fn usable(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// Required installed sea-level static `T/W` (over the take-off weight) to
/// hold level flight at the maximum-climb rating: `f (D/W') / r`.
///
/// `drag_over_weight` is `CD / CL` at the level-flight mass, `mass_fraction`
/// that mass over the take-off mass (1 at the take-off weight) and
/// `thrust_ratio` the deck's `F_MCL / F_SLS`. `None` for non-positive inputs.
pub fn required_sls_tw_level(
    drag_over_weight: f64,
    mass_fraction: f64,
    thrust_ratio: f64,
) -> Option<f64> {
    (usable(drag_over_weight) && usable(mass_fraction) && usable(thrust_ratio))
        .then(|| mass_fraction * drag_over_weight / thrust_ratio)
}

/// Required installed sea-level static `T/W` for the top-of-climb rate:
/// `f (D/W' + ROC / V) / r`.
///
/// `drag_over_weight_toc` is `CD / CL` at the top-of-climb mass,
/// `climb_rate_m_s` the residual rate, `tas_m_s` the true airspeed (held
/// constant, so the acceleration term of a constant-Mach climb is zero),
/// `mass_fraction` the top-of-climb over take-off mass and `thrust_ratio`
/// the deck's `F_MCL / F_SLS` at the condition. `None` for non-physical
/// inputs.
pub fn required_sls_tw_top_of_climb(
    drag_over_weight_toc: f64,
    climb_rate_m_s: f64,
    tas_m_s: f64,
    mass_fraction: f64,
    thrust_ratio: f64,
) -> Option<f64> {
    (usable(drag_over_weight_toc)
        && climb_rate_m_s.is_finite()
        && climb_rate_m_s >= 0.0
        && usable(tas_m_s)
        && usable(mass_fraction)
        && usable(thrust_ratio))
    .then(|| mass_fraction * (drag_over_weight_toc + climb_rate_m_s / tas_m_s) / thrust_ratio)
}

#[cfg(test)]
mod tests {
    // A test asserts on values it built, so a failed unwrap is the assertion failing.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_level_requirement_is_the_drag_over_the_deck_ratio() {
        // CD/CL 0.0575 (L/D 17.4) over a deck ratio of 0.235: 0.2447.
        let tw = required_sls_tw_level(0.0575, 1.0, 0.235).unwrap();
        assert!((tw - 0.244_680_851).abs() < 1e-8, "{tw}");
        assert!(required_sls_tw_level(0.0575, 1.0, 0.0).is_none());
        assert!(required_sls_tw_level(f64::NAN, 1.0, 0.2).is_none());
        assert!(required_sls_tw_level(0.0575, 0.0, 0.2).is_none());
    }

    /// At the mid-cruise weight (0.85 of take-off) the requirement on the
    /// take-off-weight axis scales by the mass fraction: 0.85 x 0.2447 =
    /// 0.20798, a 15 % lower requirement for the same D/W.
    #[test]
    fn the_level_requirement_scales_with_the_mid_cruise_mass_fraction() {
        let tw = required_sls_tw_level(0.0575, 0.85, 0.235).unwrap();
        assert!((tw - 0.85 * 0.0575 / 0.235).abs() < 1e-15, "{tw}");
        assert!((tw - 0.207_978).abs() < 1e-6, "{tw}");
    }

    /// Hand case, A320 class at FL350, M 0.78: V = 230.4 m/s, 300 ft/min =
    /// 1.524 m/s, ROC/V = 0.006615; D/W' 0.0575 at the top-of-climb mass;
    /// f = 0.985; r = 0.235:  0.985 (0.0575 + 0.006615) / 0.235 = 0.26874.
    #[test]
    fn the_top_of_climb_requirement_adds_the_gradient_and_scales_with_the_mass_fraction() {
        let v = 230.4;
        let tw =
            required_sls_tw_top_of_climb(0.0575, TOC_RESIDUAL_CLIMB_M_S, v, 0.985, 0.235).unwrap();
        let hand = 0.985 * (0.0575 + 1.524 / 230.4) / 0.235;
        assert!((tw - hand).abs() < 1e-12);
        assert!((tw - 0.268_74).abs() < 2e-5, "{tw}");
        // With no residual climb it reduces to the level requirement at the
        // top-of-climb weight.
        let level = required_sls_tw_top_of_climb(0.0575, 0.0, v, 1.0, 0.235).unwrap();
        assert!((level - required_sls_tw_level(0.0575, 1.0, 0.235).unwrap()).abs() < 1e-15);
        // Monotone: more climb rate, more thrust.
        let more = required_sls_tw_top_of_climb(0.0575, 2.5, v, 0.985, 0.235).unwrap();
        assert!(more > tw);
    }

    #[test]
    fn the_criterion_is_three_hundred_feet_per_minute() {
        assert!((TOC_RESIDUAL_CLIMB_M_S - 1.524).abs() < 1e-12);
    }
}
