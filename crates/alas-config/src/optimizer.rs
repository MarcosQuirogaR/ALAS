// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py

//! What the design search is looking for, and how hard it looks.
//!
//! The halves are deliberately separate. [`ObjectiveConfig`] says what a
//! good aircraft is: the mission quantity minimised and every hard
//! requirement family that bounds it, and changing one of those changes
//! which design the search converges on. [`SolverSettings`] says how the
//! search is run, and changing one of those changes how thoroughly the same
//! target is approached, not what the target is. Two runs that differ only in
//! solver settings are answering the same question; two that differ in the
//! objective are not, and a comparison between them means nothing.
//!
//! [`ObjectiveWeights`] holds the transport-planform thresholds and the failure
//! cost the mission-sized search reads.

mod aerodrome_code;
pub mod design_space;
pub mod mtow_plan;
mod objective;
pub mod plausibility;
mod solver;
mod weights;

pub use aerodrome_code::{AerodromeReferenceCode, SPAN_CODE_MARGIN_M};
pub use design_space::{DesignMode, DesignSpaceConfig, VariableEnvelope};
pub use mtow_plan::{
    DesignMission, DesignPayloadSource, DesignRange, MtowPlan, StructuralBasis,
    UNBOUNDED_DISPATCH_MTOW_KG,
};
pub use objective::{MtowSizing, ObjectiveConfig, ObjectiveKind, DEFAULT_MTOW_BAND_FRACTION};
pub use plausibility::PlausibilityLimits;
pub use solver::{
    SeedOutOfRange, SolverSettings, StageBudget, LEGACY_METHOD_TOKENS, MAXIMUM_STAGE_TIME_LIMIT_S,
    PRODUCT_DE_METHOD,
};
pub use weights::ObjectiveWeights;

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// The composed optimizer configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct OptimizerConfig {
    /// What the search rewards and what it penalizes.
    #[config(
        nested,
        help = "Transport-planform thresholds and the failure cost the mission-sized search reads."
    )]
    pub weights: ObjectiveWeights,

    /// How the search is run.
    #[config(
        nested,
        help = "Which complete optimization profile runs, how long and wide its population is, and where in the design space it starts."
    )]
    pub solver: SolverSettings,

    /// What the mission-sized product search minimises, and which requirements bound it.
    #[serde(default)]
    #[config(
        nested,
        help = "Used by the mission-sized differential_evolution profile: select block fuel, takeoff mass, empty mass or fuel per seat-kilometre over the design range, plus the takeoff-mass closure and hard requirements."
    )]
    pub objective: ObjectiveConfig,

    /// Which aircraft fields are allowed to change during a product run.
    #[serde(default, skip_serializing_if = "DesignSpaceConfig::is_default")]
    #[config(
        nested,
        help = "Design boundary for clean-sheet studies, reference-aircraft adaptation, and the fixed baseline sandbox. The resolved mutable/fixed envelope is recorded with each run and is enforced by the evaluator as well as the search bounds."
    )]
    pub design_space: DesignSpaceConfig,

    /// The validity domain an optimized design must stay inside.
    ///
    /// See [`plausibility::PlausibilityLimits`] for what each window means
    /// and why it is a statement about the model rather than a requirement.
    #[serde(default, skip_serializing_if = "PlausibilityLimits::is_default")]
    #[config(
        nested,
        label = "Model validity domain",
        help = "The window of shapes this program's own mass, drag and stability correlations were fitted for. These are not performance requirements: a design outside one of them is a result the correlations cannot be trusted to have computed, which is why every window is set wider than every registered aircraft. They reach the search as hard Geometry residuals."
    )]
    pub plausibility: PlausibilityLimits,
}

impl OptimizerConfig {
    /// Blocking issues in the objective and model-validity windows.
    pub(crate) fn validation_issues(&self) -> Vec<crate::validation::ValidationIssue> {
        [
            ("optimizer.plausibility", self.plausibility.validate()),
            ("optimizer.objective", self.objective.validate()),
        ]
        .into_iter()
        .filter_map(|(path, result)| {
            result
                .err()
                .map(|message| crate::validation::ValidationIssue {
                    field_path: path.to_owned(),
                    message,
                    severity: crate::validation::Severity::Error,
                })
        })
        .collect()
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entry;

    #[test]
    fn what_is_searched_for_and_how_it_is_searched_reach_the_form_as_separate_groups() {
        // A run that changed a weight and a run that changed a generation
        // count are not comparable, and the form is where that distinction
        // has to be visible. The mission-sized objective is a third question:
        // what is being minimised at all, and gets its own group. The
        // validity domain defines where the model stops being trustworthy.
        let schema = OptimizerConfig::default().schema();
        let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(
            names,
            vec![
                "weights",
                "solver",
                "objective",
                "design_space",
                "plausibility",
            ]
        );
        for field in &schema.fields {
            assert!(matches!(field.entry, Entry::Node(_)), "{}", field.name);
        }
    }

    #[test]
    fn the_composed_default_is_the_two_halves_defaults() {
        let config = OptimizerConfig::default();
        assert_eq!(config.weights, ObjectiveWeights::default());
        assert_eq!(config.solver, SolverSettings::default());
        assert_eq!(config.design_space, DesignSpaceConfig::default());
    }

    #[test]
    fn registered_preset_preserves_explicit_mission_sizing_on_save() {
        let mut config = crate::AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200"
        }))
        .unwrap();
        assert_eq!(
            config.optimizer.objective.mtow_sizing,
            MtowSizing::FixedRequirement
        );
        config.optimizer.objective = ObjectiveConfig::default();
        let document = serde_json::to_value(&config).unwrap();
        assert_eq!(
            document["optimizer"]["objective"]["mtow_sizing"],
            "sized_by_mission"
        );
        let loaded = crate::AlasConfig::from_value(&document).unwrap();
        assert_eq!(loaded.optimizer.objective, config.optimizer.objective);
    }
}
