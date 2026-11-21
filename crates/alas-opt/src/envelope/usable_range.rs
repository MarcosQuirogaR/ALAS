// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimum usable CG range at a loading state's mass.
//!
//! A registered preset whose primary source publishes its CG envelope is held
//! to that envelope's width at the same mass (compared in metres, so a model
//! with another chord asks for the same length of CG travel); every other run
//! uses `requirements.cg_range_pct_mac`, the clean-sheet default.

use alas_config::{AlasConfig, UsableCgRangeSource};

/// Where one run's minimum usable CG range comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum UsableRangeRequirement {
    /// The registered preset's published envelope, with the configured value
    /// kept for a mass at which the envelope gives no width.
    Published {
        source: UsableCgRangeSource,
        configured_pct_mac: f64,
    },
    /// `requirements.cg_range_pct_mac`.
    Configured(f64),
}

impl UsableRangeRequirement {
    /// The requirement of `config`: its registered preset's published
    /// envelope when there is one, otherwise the configured value.
    pub(crate) fn of(config: &AlasConfig) -> Self {
        let configured_pct_mac = config.requirements.cg_range_pct_mac;
        alas_config::presets::get(&config.preset)
            .ok()
            .and_then(|preset| preset.reference.usable_cg_range)
            .map_or(Self::Configured(configured_pct_mac), |source| {
                Self::Published {
                    source,
                    configured_pct_mac,
                }
            })
    }

    /// Requirement at `mass_kg`, in percent of the model chord `model_mac_m`.
    pub(crate) fn pct_of_model_mac(&self, mass_kg: f64, model_mac_m: f64) -> f64 {
        match *self {
            Self::Configured(pct) => pct,
            Self::Published {
                source,
                configured_pct_mac,
            } => source
                .width_m(mass_kg)
                .filter(|_| model_mac_m.is_finite() && model_mac_m > 0.0)
                .map_or(configured_pct_mac, |width_m| 100.0 * width_m / model_mac_m),
        }
    }

    /// Requirement at `mass_kg` as a length along the fuselage, in metres.
    pub(crate) fn length_m(&self, mass_kg: f64, model_mac_m: f64) -> f64 {
        self.pct_of_model_mac(mass_kg, model_mac_m) / 100.0 * model_mac_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_for(preset: &str) -> AlasConfig {
        AlasConfig {
            preset: preset.to_owned(),
            ..AlasConfig::default()
        }
    }

    #[test]
    fn a_preset_without_a_published_envelope_keeps_the_configured_range() {
        let requirement = UsableRangeRequirement::of(&config_for("A320-200"));
        assert_eq!(requirement, UsableRangeRequirement::Configured(30.0));
        assert_eq!(requirement.pct_of_model_mac(70_000.0, 4.19), 30.0);
    }

    #[test]
    fn the_a220_is_held_to_its_published_width_in_metres_at_each_mass() {
        let requirement = UsableRangeRequirement::of(&config_for("A220-300"));
        let source_mac_m = 148.86 * 0.0254;
        // MTOW: ground envelope 12.2 % of the source chord.
        let at_mtow = requirement.pct_of_model_mac(67_585.0, source_mac_m);
        assert!((at_mtow - 12.2).abs() < 1.0e-9, "{at_mtow}");
        // A model chord 10 % longer needs the same metres: fewer percent.
        let longer = requirement.pct_of_model_mac(67_585.0, 1.1 * source_mac_m);
        assert!((longer - 12.2 / 1.1).abs() < 1.0e-9, "{longer}");
        // Mid weight: far wider than the heavy corner.
        assert!(requirement.pct_of_model_mac(58_967.0, source_mac_m) > 20.0);
    }

    #[test]
    fn a_degenerate_model_chord_falls_back_to_the_configured_value() {
        let requirement = UsableRangeRequirement::of(&config_for("A220-300"));
        assert_eq!(requirement.pct_of_model_mac(67_585.0, 0.0), 30.0);
    }
}
