// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/solver_presets.py

//! Named speed-against-thoroughness settings for the design search.
//!
//! Every field these presets set is a stage budget on [`SolverSettings`]:
//! screening and refinement wall-clock limits, which trade wall-clock time
//! for how far the search refines, and evaluation ceilings set well above
//! what each limit affords (see the stage defaults), so the limits end the
//! stages and the refinement plans its budget from the measured throughput.
//! Bundling them under four names makes that trade a single choice.
//!
//! Nothing here touches [`crate::ObjectiveWeights`], deliberately. A preset
//! that changed what the search was looking for as well as how hard it looked
//! would make two runs incomparable while appearing to differ only in effort.

use std::sync::OnceLock;

use crate::{SolverSettings, StageBudget};

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

/// A stage budget of `max_evaluations` evaluations and `time_limit_s` s.
fn stage(max_evaluations: i64, time_limit_s: f64) -> StageBudget {
    StageBudget {
        max_evaluations,
        time_limit_s,
        replay_evaluations: None,
        replay_planned_evaluations: None,
        replay_restoration_evaluations: None,
        max_pregate_rejects: None,
    }
}

fn build() -> Vec<SolverPreset> {
    vec![
        SolverPreset {
            name: "quick_draft",
            display_name: "Quick Draft",
            description: "Fast, rough pass: 15 s of screening and at most 60 s of refinement. \
                          Good for iterating on requirements/geometry before committing to a \
                          full run.",
            settings: SolverSettings {
                screening: stage(10_000, 15.0),
                refinement: stage(10_000, 60.0),
                ..SolverSettings::default()
            },
        },
        SolverPreset {
            name: "balanced",
            display_name: "Balanced (Recommended)",
            description: "The default tradeoff: 30 s of screening and at most 2 min of \
                          refinement.",
            // Whatever `SolverSettings` itself defaults to, so the recommended
            // preset and an unconfigured run are the same run.
            settings: SolverSettings::default(),
        },
        SolverPreset {
            name: "thorough",
            display_name: "Thorough",
            description: "A larger refinement budget, at most 4 min, for a closer local \
                          optimum on a final design.",
            settings: SolverSettings {
                screening: stage(30_000, 60.0),
                refinement: stage(30_000, 240.0),
                ..SolverSettings::default()
            },
        },
        SolverPreset {
            name: "exhaustive",
            display_name: "Exhaustive",
            description: "The largest budgets the stages accept: 2 min of screening and 5 min \
                          of refinement. Slowest option; use for a final high-confidence \
                          optimization.",
            settings: SolverSettings {
                screening: stage(40_000, 120.0),
                refinement: stage(40_000, crate::optimizer::MAXIMUM_STAGE_TIME_LIMIT_S),
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
        // The dropdown reads as a scale: each step down buys more screening
        // time, more refinement time and a larger refinement budget, and
        // every one of them is a budget the optimizer accepts.
        let settings: Vec<&SolverSettings> =
            registry().iter().map(|preset| &preset.settings).collect();
        for pair in settings.windows(2) {
            assert!(pair[0].screening.time_limit_s < pair[1].screening.time_limit_s);
            assert!(pair[0].refinement.time_limit_s < pair[1].refinement.time_limit_s);
            assert!(pair[0].refinement.max_evaluations < pair[1].refinement.max_evaluations);
        }
        for preset in registry() {
            assert_eq!(
                preset.settings.validate_budgets(),
                Ok(()),
                "{}",
                preset.name
            );
            assert_eq!(preset.settings.workers, 0, "{}", preset.name);
        }
    }

    #[test]
    fn the_recommended_preset_is_what_an_unconfigured_run_already_does() {
        assert_eq!(get("balanced").unwrap().settings, SolverSettings::default());
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
