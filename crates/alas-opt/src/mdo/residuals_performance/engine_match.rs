// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a fixed engine rating still matches the closed take-off mass.
//!
//! The catalogue engine is rated for one aircraft. When the take-off mass
//! closes far from the mass that engine was matched to, the fixed thrust no
//! longer belongs to the sized airframe: a 20 percent mass mismatch is about
//! +1.4 t (+1.8 percent TOW) and, amplified by the closure factor
//! `1/(1 - s)` with `s` of 0.45 to 0.6, +3 to +4 percent TOW. The check only
//! warns; it never changes the result.
//!
//! Data: there is no engine design take-off mass in the engine catalogue yet
//! (lane B1 adds the optional field). The proxy is the declared MTOW of the
//! registered aircraft, `requirements.mtow_kg`, which is meaningful only when
//! the engine is matched to that aircraft, i.e. outside the clean-sheet design
//! mode where the declared MTOW is merely a target.

use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;

/// The closed-to-design take-off mass ratio and the accepted band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineMassMatch {
    /// Closed take-off mass over the engine design take-off mass.
    pub ratio: f64,
    /// Half-width of the accepted band, `engine_match_tolerance`.
    pub tolerance: f64,
    /// The engine design take-off mass used, kg.
    pub engine_design_takeoff_mass_kg: f64,
}

impl EngineMassMatch {
    /// Whether the ratio lies inside `[1 - tolerance, 1 + tolerance]`.
    pub fn within_band(&self) -> bool {
        // The tolerance admits the exact band edge despite rounding of the ratio.
        (self.ratio - 1.0).abs() <= self.tolerance + 1e-12
    }

    /// Distance outside the band, as a fraction of the tolerance (0 inside).
    pub fn normalized_excess(&self) -> f64 {
        ((self.ratio - 1.0).abs() - self.tolerance).max(0.0) / self.tolerance.max(1e-9)
    }
}

/// The engine/mass match of `closed_takeoff_mass_kg`, or `None` when no
/// engine design mass datum exists (clean-sheet mode) or an input is unusable.
pub fn engine_mass_match(
    config: &AlasConfig,
    closed_takeoff_mass_kg: f64,
) -> Option<EngineMassMatch> {
    if config.optimizer.design_space.mode == DesignMode::CleanSheet {
        return None;
    }
    let design_mass_kg = config.requirements.mtow_kg;
    let tolerance = config.performance.engine_match_tolerance;
    let usable = |value: f64| value.is_finite() && value > 0.0;
    if !(usable(closed_takeoff_mass_kg) && usable(design_mass_kg) && usable(tolerance)) {
        return None;
    }
    Some(EngineMassMatch {
        ratio: closed_takeoff_mass_kg / design_mass_kg,
        tolerance,
        engine_design_takeoff_mass_kg: design_mass_kg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mode: DesignMode) -> AlasConfig {
        let mut config = AlasConfig::default();
        config.optimizer.design_space.mode = mode;
        config.requirements.mtow_kg = 78_000.0;
        config
    }

    #[test]
    fn the_band_boundaries_are_inclusive_at_0_85_and_1_15() {
        let matched = |ratio: f64| EngineMassMatch {
            ratio,
            tolerance: 0.15,
            engine_design_takeoff_mass_kg: 78_000.0,
        };
        for (ratio, inside) in [
            (0.85, true),
            (1.15, true),
            (0.8499, false),
            (1.1501, false),
            (1.0, true),
        ] {
            assert_eq!(matched(ratio).within_band(), inside, "ratio {ratio}");
        }
        // The ratio is closed mass over design mass.
        let config = config(DesignMode::ReferenceAdaptation);
        let from_masses =
            engine_mass_match(&config, 78_000.0 * 0.9).unwrap_or_else(|| panic!("match"));
        assert!((from_masses.ratio - 0.9).abs() < 1e-12);
        assert!(from_masses.within_band());
    }

    #[test]
    fn the_excess_is_zero_inside_and_scales_with_the_tolerance_outside() {
        let config = config(DesignMode::BaselineSandbox);
        let inside = engine_mass_match(&config, 78_000.0 * 1.10).unwrap_or_else(|| panic!("match"));
        assert_eq!(inside.normalized_excess(), 0.0);
        let outside =
            engine_mass_match(&config, 78_000.0 * 1.30).unwrap_or_else(|| panic!("match"));
        // 0.30 beyond 1, tolerance 0.15: (0.30 - 0.15) / 0.15 = 1.
        assert!((outside.normalized_excess() - 1.0).abs() < 1e-12);
        assert!(!outside.within_band());
    }

    #[test]
    fn a_clean_sheet_design_has_no_engine_design_mass_datum() {
        assert!(engine_mass_match(&config(DesignMode::CleanSheet), 80_000.0).is_none());
    }

    #[test]
    fn unusable_inputs_give_no_verdict() {
        let config = config(DesignMode::ReferenceAdaptation);
        assert!(engine_mass_match(&config, f64::NAN).is_none());
        assert!(engine_mass_match(&config, 0.0).is_none());
    }
}
