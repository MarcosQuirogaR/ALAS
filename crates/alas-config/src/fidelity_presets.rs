// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/fidelity_presets.py
// Reference: alas @ rust-port-baseline.

//! Named resolutions for the aerodynamic analysis.
//!
//! Each preset sets exactly three fields of [`AnalysisConfig`]: how many
//! points the polar sweep takes, and how finely the vortex lattice is
//! panelled spanwise and chordwise. Those are the knobs that buy accuracy with
//! time and nothing else.
//!
//! # What a preset carries, and what applying one should mean
//!
//! [`AnalysisConfig`] also holds physical and empirical assumptions -- tail
//! efficiency, the lift-coefficient window the drag polar is fitted over, the
//! probe angles the trim search starts from -- and every preset leaves those
//! at their defaults. Upstream states that the registry is scoped to the three
//! resolution fields precisely so it cannot clobber an assumption the user has
//! tuned, and then hands its consumer the whole configuration, which clobbers
//! them. This port reproduces what the registry holds and does not decide the
//! question; a `deviation-candidate` in docs/PORTING.md records it for
//! whoever writes the consumer.
//!
//! This is unrelated to [`crate::performance_presets`], which is about what
//! the field-performance model assumes rather than how finely anything is
//! resolved.

use std::sync::OnceLock;

use crate::AnalysisConfig;

/// A fidelity preset that was asked for and is not registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown fidelity preset '{name}'; available: {}", available.join(", "))]
pub struct UnknownFidelityPreset {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// A named analysis resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct FidelityPreset {
    /// The key the configuration selects it by.
    pub name: &'static str,
    /// What the interface calls it.
    pub display_name: &'static str,
    /// What choosing it means, in one or two sentences.
    pub description: &'static str,
    /// The analysis configuration it describes. Only the three resolution
    /// fields differ between presets; see the module documentation.
    pub analysis: AnalysisConfig,
}

/// Every registered preset, in the order the interface lists them.
pub fn registry() -> &'static [FidelityPreset] {
    static REGISTRY: OnceLock<Vec<FidelityPreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// Look one preset up by name.
///
/// # Errors
///
/// [`UnknownFidelityPreset`], carrying what is available. Upstream raises for
/// the same input.
pub fn get(name: &str) -> Result<&'static FidelityPreset, UnknownFidelityPreset> {
    registry()
        .iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| UnknownFidelityPreset {
            name: name.to_owned(),
            available: sorted_names(),
        })
}

/// Every preset's name, in registration order.
pub fn available() -> Vec<&'static str> {
    registry().iter().map(|preset| preset.name).collect()
}

/// Every preset's name paired with what to call it, in registration order.
pub fn display_names() -> Vec<(&'static str, &'static str)> {
    registry()
        .iter()
        .map(|preset| (preset.name, preset.display_name))
        .collect()
}

fn sorted_names() -> Vec<String> {
    let mut names: Vec<String> = registry()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    names.sort();
    names
}

fn build() -> Vec<FidelityPreset> {
    vec![
        FidelityPreset {
            name: "draft",
            display_name: "Draft (fast)",
            description: "Coarse polar sweep and panel resolution -- fastest feedback while \
                          iterating on requirements/geometry.",
            analysis: AnalysisConfig {
                sweep_n_points: 7,
                spanwise_resolution: 1,
                chordwise_resolution: 1,
                ..AnalysisConfig::default()
            },
        },
        FidelityPreset {
            name: "standard",
            display_name: "Standard (Recommended)",
            description:
                "The default resolution -- a good balance of speed and accuracy for most runs.",
            // Whatever `AnalysisConfig` itself defaults to, so the
            // recommended preset and an unconfigured run are the same run.
            analysis: AnalysisConfig::default(),
        },
        FidelityPreset {
            name: "high_fidelity",
            display_name: "High Fidelity (slow)",
            description: "Fine polar sweep and panel resolution for a final, high-confidence \
                          analysis. Slowest option.",
            analysis: AnalysisConfig {
                sweep_n_points: 30,
                spanwise_resolution: 3,
                chordwise_resolution: 3,
                ..AnalysisConfig::default()
            },
        },
    ]
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_presets_are_ordered_from_coarsest_to_finest() {
        let points: Vec<i64> = registry()
            .iter()
            .map(|preset| preset.analysis.sweep_n_points)
            .collect();
        assert!(
            points.windows(2).all(|pair| pair[0] < pair[1]),
            "{points:?}"
        );
    }

    #[test]
    fn the_presets_differ_from_each_other_in_nothing_but_resolution() {
        // This is the registry's stated scope, and the property that decides
        // whether handing a consumer the whole configuration is harmless: it
        // is, exactly as long as nothing else in it varies.
        for preset in registry() {
            let restored = AnalysisConfig {
                sweep_n_points: AnalysisConfig::default().sweep_n_points,
                spanwise_resolution: AnalysisConfig::default().spanwise_resolution,
                chordwise_resolution: AnalysisConfig::default().chordwise_resolution,
                ..preset.analysis.clone()
            };
            assert_eq!(restored, AnalysisConfig::default(), "{}", preset.name);
        }
    }

    #[test]
    fn the_recommended_preset_is_what_an_unconfigured_run_already_does() {
        assert_eq!(get("standard").unwrap().analysis, AnalysisConfig::default());
    }

    #[test]
    fn an_unknown_preset_is_an_error_that_says_what_there_is() {
        let error = get("coarse").unwrap_err();
        assert_eq!(error.available, vec!["draft", "high_fidelity", "standard"]);
        assert!(error.to_string().contains("standard"));
    }

    #[test]
    fn the_dropdown_lists_the_presets_in_registration_order() {
        assert_eq!(available(), vec!["draft", "standard", "high_fidelity"]);
    }
}
