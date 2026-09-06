// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py
// Reference: alas @ rust-port-baseline.

//! What the design search is looking for, and how hard it looks.
//!
//! The halves are deliberately separate. [`ObjectiveConfig`] says what a
//! good aircraft is -- the mission quantity minimised and the policy of every
//! requirement family that bounds it -- and changing one of those changes
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

mod objective;
mod solver;
mod weights;

pub use objective::{ConstraintPolicy, MtowSizing, ObjectiveConfig, ObjectiveKind};
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
        help = "The mission-sized objective -- block fuel, takeoff mass, empty mass or fuel per seat-kilometre over the design range under the fuel policy -- the takeoff-mass closure, and the hard, soft or diagnostic policy of every requirement family that bounds it."
    )]
    pub objective: ObjectiveConfig,
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
        // has to be visible. The mission-sized objective is a third question
        // -- what is being minimised at all -- and gets its own group.
        let schema = OptimizerConfig::default().schema();
        let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(names, vec!["weights", "solver", "objective"]);
        for field in &schema.fields {
            assert!(matches!(field.entry, Entry::Node(_)), "{}", field.name);
        }
    }

    #[test]
    fn the_composed_default_is_the_two_halves_defaults() {
        let config = OptimizerConfig::default();
        assert_eq!(config.weights, ObjectiveWeights::default());
        assert_eq!(config.solver, SolverSettings::default());
    }
}
