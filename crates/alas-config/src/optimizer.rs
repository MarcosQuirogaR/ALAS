// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py
// Reference: alas @ rust-port-baseline.

//! What the design search is looking for, and how hard it looks.
//!
//! The halves are deliberately separate. [`ObjectiveConfig`] says what a
//! good aircraft is: the mission quantity minimised and the policy of every
//! requirement family that bounds it, and changing one of those changes
//! which design the search converges on. [`SolverSettings`] says how the
//! search is run, and changing one of those changes how thoroughly the same
//! target is approached, not what the target is. Two runs that differ only in
//! solver settings are answering the same question; two that differ in the
//! objective are not, and a comparison between them means nothing.
//!
//! [`ObjectiveWeights`] is the penalty table of the frozen Python objective.
//! The product search reads only its failure cost and tail-volume window;
//! the rest is replayed by the parity fixtures and kept so a saved
//! configuration still round-trips.

pub mod design_space;
mod objective;
pub mod plausibility;
pub mod policy_review;
pub mod relaxation;
mod solver;
mod weights;

pub use design_space::{DesignMode, DesignSpaceConfig, VariableEnvelope};
pub use objective::{ConstraintPolicy, MtowSizing, ObjectiveConfig, ObjectiveKind};
pub use plausibility::PlausibilityLimits;
pub use policy_review::{review_for, RelaxationReview, ReviewedLimit, REVIEWED_LIMITS};
pub use relaxation::{ConstraintRelaxation, RelaxableLimit, NON_RELAXABLE_RESIDUAL_IDS};
pub use solver::SolverSettings;
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
        help = "Penalty table of the frozen reference objective, replayed by the parity fixtures. The mission-sized search reads only the failure cost and the tail-volume window from this group; every other weight is inert for product runs."
    )]
    pub weights: ObjectiveWeights,

    /// How the search is run.
    #[config(
        nested,
        help = "Which optimization method runs, how long and wide its population is, and where in the design space it starts."
    )]
    pub solver: SolverSettings,

    /// What the mission-sized search minimises, and which requirements bound it.
    #[serde(default, skip_serializing_if = "ObjectiveConfig::is_default")]
    #[config(
        nested,
        help = "The mission-sized objective (block fuel, takeoff mass, empty mass or fuel per seat-kilometre over the design range under the fuel policy) the takeoff-mass closure, and the hard, soft or diagnostic policy of every requirement family that bounds it."
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
        help = "The window of shapes this program's own mass, drag and stability correlations were fitted for. These are not performance requirements: a design outside one of them is a result the correlations cannot be trusted to have computed, which is why every window is set wider than every registered aircraft. They reach the search as named Geometry residuals and follow that family's policy."
    )]
    pub plausibility: PlausibilityLimits,

    /// Whether, and how far, an overconstrained problem may miss a limit.
    ///
    /// Strict as shipped; see [`relaxation::ConstraintRelaxation`] for the
    /// D01-D03 rules and [`policy_review`] for the review that decides which
    /// limits may ever be listed.
    #[serde(default, skip_serializing_if = "ConstraintRelaxation::is_default")]
    #[config(
        nested,
        label = "Controlled constraint relaxation",
        help = "Off by default, so every Mass, Balance, Performance and Geometry limit is hard. The engineering review behind it currently admits no limit at all, so switching it on changes nothing until a reviewed tolerance with a primary source exists."
    )]
    pub relaxation: ConstraintRelaxation,
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
        // validity domain and the relaxation policy are two more: where the
        // model stops being trustworthy, and whether a limit may be missed.
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
                "relaxation"
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
        assert_eq!(config.relaxation, ConstraintRelaxation::default());
    }

    #[test]
    fn the_shipped_optimizer_relaxes_nothing() {
        // A user who never opens the relaxation form must get exactly the
        // strict behaviour every measurement in this product was taken with.
        let config = OptimizerConfig::default();
        assert!(!config.relaxation.is_active());
    }
}
