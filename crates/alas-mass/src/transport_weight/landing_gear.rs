// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Common.landing_gear.landing_gear.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Main- and nose-gear masses, a fixed fraction of takeoff weight.

/// The landing gear as a fraction of takeoff weight -- upstream's
/// `landing_gear_wt_factor` default, never overridden by
/// [`super::empty_weight`].
const GEAR_MASS_FRACTION: f64 = 0.04;

/// The main and nose landing-gear masses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LandingGear {
    pub main_kg: f64,
    pub nose_kg: f64,
}

/// The landing-gear mass split -- `landing_gear`. The total is 4% of takeoff
/// weight, split 90% main / 10% nose.
pub(crate) fn landing_gear(mtow_kg: f64) -> LandingGear {
    let weight_kg = GEAR_MASS_FRACTION * mtow_kg;
    LandingGear {
        main_kg: weight_kg * 0.9,
        nose_kg: weight_kg * 0.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_is_four_percent_of_takeoff_weight_split_ninety_ten() {
        let gear = landing_gear(100_000.0);
        assert!((gear.main_kg - 3600.0).abs() < 1e-9);
        assert!((gear.nose_kg - 400.0).abs() < 1e-9);
    }
}
