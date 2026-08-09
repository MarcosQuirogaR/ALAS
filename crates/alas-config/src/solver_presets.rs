// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/solver_presets.py
// Reference: alas @ rust-port-baseline.

//! Named speed-against-thoroughness settings for the design search.
//!
//! Every field these presets set is on [`SolverSettings`], and every one of
//! them trades wall-clock time for how well the search converges. Bundling
//! them under four names is what makes that trade a single choice: a
//! population size raised without a matching generation budget explores widely
//! and converges on nothing, and picking the two independently is how that
//! happens.
//!
//! Nothing here touches [`crate::ObjectiveWeights`], deliberately. A preset
//! that changed what the search was looking for as well as how hard it looked
//! would make two runs incomparable while appearing to differ only in effort.

use std::sync::OnceLock;

use crate::SolverSettings;

/// A solver preset that was asked for and is not registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown solver preset '{name}'; available: {}", available.join(", "))]
pub struct UnknownSolverPreset {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// A named solver configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverPreset {
    /// The key the configuration selects it by.
    pub name: &'static str,
    /// What the interface calls it.
    pub display_name: &'static str,
    /// What choosing it means, in one or two sentences.
    pub description: &'static str,
    /// The settings it applies in full.
    pub settings: SolverSettings,
}

/// Every registered preset, in the order the interface lists them.
///
/// Registration order is the dropdown's order, and the first entry is what a
/// user who does not choose ends up running, so it is preserved rather than
/// sorted.
pub fn registry() -> &'static [SolverPreset] {
    static REGISTRY: OnceLock<Vec<SolverPreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// Look one preset up by name.
///
/// # Errors
///
/// [`UnknownSolverPreset`], carrying what is available. Upstream raises for
/// the same input.
pub fn get(name: &str) -> Result<&'static SolverPreset, UnknownSolverPreset> {
    registry()
        .iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| UnknownSolverPreset {
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

fn build() -> Vec<SolverPreset> {
    vec![
        SolverPreset {
            name: "quick_draft",
            display_name: "Quick Draft",
            description: "Fast, rough pass -- small population and few generations. Good for \
                          iterating on requirements/geometry before committing to a full run.",
            settings: SolverSettings {
                strategy: "best1bin".to_owned(),
                max_iterations: 8,
                population_size: 4,
                tolerance: 0.02,
                workers: 4,
                display_progress: true,
                ..SolverSettings::default()
            },
        },
        SolverPreset {
            name: "balanced",
            display_name: "Balanced (Recommended)",
            description:
                "The default tradeoff -- good convergence in a reasonable wall-clock time.",
            // Whatever `SolverSettings` itself defaults to, so the recommended
            // preset and an unconfigured run are the same run.
            settings: SolverSettings::default(),
        },
        SolverPreset {
            name: "thorough",
            display_name: "Thorough",
            description: "Larger population and more generations for tighter convergence on a \
                          final design.",
            settings: SolverSettings {
                strategy: "best1bin".to_owned(),
                max_iterations: 30,
                population_size: 10,
                tolerance: 0.005,
                workers: 4,
                display_progress: true,
                ..SolverSettings::default()
            },
        },
        SolverPreset {
            name: "exhaustive",
            display_name: "Exhaustive",
            description: "Widest search -- large population, many generations, tight tolerance. \
                          Slowest option; use for a final high-confidence optimization.",
            settings: SolverSettings {
                strategy: "best1bin".to_owned(),
                max_iterations: 60,
                population_size: 15,
                tolerance: 0.002,
                workers: 4,
                display_progress: true,
                ..SolverSettings::default()
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
    fn the_presets_are_ordered_from_quickest_to_most_thorough() {
        // The dropdown reads as a scale, so a user picking the next one down
        // gets more search rather than a different kind of search.
        let budgets: Vec<i64> = registry()
            .iter()
            .map(|preset| preset.settings.max_iterations * preset.settings.population_size)
            .collect();
        assert!(
            budgets.windows(2).all(|pair| pair[0] < pair[1]),
            "{budgets:?}"
        );
    }

    #[test]
    fn a_longer_search_is_also_asked_to_converge_more_tightly() {
        // More generations spent against a loose tolerance would stop early
        // and waste the budget the user just paid for.
        let tolerances: Vec<f64> = registry()
            .iter()
            .map(|preset| preset.settings.tolerance)
            .collect();
        assert!(
            tolerances.windows(2).all(|pair| pair[0] > pair[1]),
            "{tolerances:?}"
        );
    }

    #[test]
    fn the_recommended_preset_is_what_an_unconfigured_run_already_does() {
        assert_eq!(get("balanced").unwrap().settings, SolverSettings::default());
    }

    #[test]
    fn every_preset_names_a_strategy_the_solver_accepts() {
        let accepted = crate::OptionSource::Strategy.options().unwrap();
        for preset in registry() {
            assert!(
                accepted.contains(&preset.settings.strategy.as_str()),
                "{}: {}",
                preset.name,
                preset.settings.strategy
            );
        }
    }

    #[test]
    fn an_unknown_preset_is_an_error_that_says_what_there_is() {
        let error = get("fastest").unwrap_err();
        assert_eq!(
            error.available,
            vec!["balanced", "exhaustive", "quick_draft", "thorough"]
        );
        assert!(error.to_string().contains("balanced"));
    }

    #[test]
    fn the_dropdown_lists_the_presets_in_registration_order() {
        assert_eq!(
            available(),
            vec!["quick_draft", "balanced", "thorough", "exhaustive"]
        );
        assert_eq!(display_names()[1], ("balanced", "Balanced (Recommended)"));
    }
}
