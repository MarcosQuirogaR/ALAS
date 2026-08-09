// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py (`CargoDeckConfig`)
// Reference: alas @ rust-port-baseline.

//! Which decks carry freight, in what containers, and how it is distributed.
//!
//! Where the load goes matters more than how much of it there is. A hold
//! filled front to back puts the centre of gravity outside the envelope with
//! exactly the same payload that would have been fine spread differently, so
//! the loading strategy is a first-class choice here rather than an
//! implementation detail of the loader.
//!
//! Zero means "work it out" for every position field: a door position of zero
//! is not a door at the nose, it is a door the layout places. That convention
//! is upstream's and is reproduced, including its one sharp edge -- a target
//! centre of gravity at or below zero means the centre of the envelope, so
//! there is no way to ask for a trim point at the datum itself.
//!
//! None of these fields declares an explanation upstream; the ones here are
//! this port's, as CONTRIBUTING.md requires, and they change no value.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Cargo loading configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct CargoDeckConfig {
    /// Whether freight is carried on the main deck as well as below.
    #[config(
        help = "Carry freight on the main deck, as a dedicated freighter does, rather than only in the lower holds. A passenger aircraft has no main-deck cargo, so this distinguishes the two configurations."
    )]
    pub use_main_deck: bool,

    /// Which container the main deck is loaded with.
    #[config(
        help = "Unit load device code for the main deck -- the pallets and boxes a freighter's main deck takes, which are far larger than anything that fits a lower hold."
    )]
    pub main_deck_uld: String,

    /// Which container the lower holds are loaded with.
    #[config(
        help = "Unit load device code for the lower holds. Degrades automatically to a shorter container and then to bulk loading where the hold's cross-section cannot take the requested one, which is what narrowbody holds usually require."
    )]
    pub lower_deck_uld: String,

    /// How the load is distributed among the available positions.
    #[config(
        help = "How to place the load: 'target_cg' spreads it and trims to the target centre of gravity, 'min_pallets' concentrates full containers near that point, 'door_proximity' favours the positions nearest the doors for a fast turnaround, and 'uniform' spreads it evenly regardless."
    )]
    pub loading_strategy: String,

    /// Where the loaded centre of gravity is trimmed to.
    #[config(
        help = "Centre of gravity the load is trimmed toward, as a percentage of mean aerodynamic chord. Zero or below means the centre of the CG envelope, so the loader targets a valid point without being told where one is."
    )]
    pub target_cg_pct_mac: f64,

    /// Where the main-deck door is, or zero to place it.
    #[config(
        help = "Longitudinal position of the main-deck cargo door. 0 places it at mid-cabin, which is what the door-proximity strategy measures distance from."
    )]
    pub main_door_x_m: f64,

    /// Where the forward hold door is, or zero to place it.
    #[config(
        help = "Longitudinal position of the forward lower-hold door. 0 places it against the forward hold."
    )]
    pub fwd_door_x_m: f64,

    /// Where the aft hold door is, or zero to place it.
    #[config(
        help = "Longitudinal position of the aft lower-hold door. 0 places it against the aft hold."
    )]
    pub aft_door_x_m: f64,

    /// How much load the trim solver moves per iteration.
    #[config(
        help = "How much payload the centre-of-gravity trim moves between positions per iteration. A smaller step trims more precisely and needs more iterations to get there."
    )]
    pub cg_trim_step_kg: f64,

    /// How many iterations the trim solver may take.
    #[config(
        help = "Iteration cap for the centre-of-gravity trim. A very large aircraft moving a small step per iteration can need more than the default; raise this rather than the step if the trim is not converging."
    )]
    pub cg_trim_max_iterations: i64,
}

impl Default for CargoDeckConfig {
    fn default() -> Self {
        Self {
            use_main_deck: true,
            main_deck_uld: "PMC".to_owned(),
            lower_deck_uld: "LD3".to_owned(),
            loading_strategy: "target_cg".to_owned(),
            target_cg_pct_mac: 25.0,
            main_door_x_m: 0.0,
            fwd_door_x_m: 0.0,
            aft_door_x_m: 0.0,
            cg_trim_step_kg: 50.0,
            cg_trim_max_iterations: 2000,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_door_position_defaults_to_being_placed_rather_than_stated() {
        let config = CargoDeckConfig::default();
        assert_eq!(config.main_door_x_m, 0.0);
        assert_eq!(config.fwd_door_x_m, 0.0);
        assert_eq!(config.aft_door_x_m, 0.0);
    }

    #[test]
    fn the_trim_solver_can_move_a_meaningful_share_of_a_load() {
        // Step times iterations bounds how much load the trim can move in
        // total; a cap below a full container would leave the solver unable
        // to reach a valid centre of gravity on a large aircraft.
        let config = CargoDeckConfig::default();
        let reachable = config.cg_trim_step_kg * config.cg_trim_max_iterations as f64;
        assert!(
            reachable > 50_000.0,
            "the trim can only move {reachable} kg"
        );
    }

    #[test]
    fn no_string_field_here_claims_an_option_list() {
        // The container codes and the loading strategy are validated by
        // whatever resolves them, not by the form: upstream's schema offers
        // no list for any of the three, and offering one here would be this
        // port inventing a constraint.
        let schema = CargoDeckConfig::default().schema();
        for name in ["main_deck_uld", "lower_deck_uld", "loading_strategy"] {
            match &schema.field(name).unwrap().entry {
                crate::Entry::Leaf(leaf) => assert_eq!(leaf.options, None, "{name}"),
                crate::Entry::Node(_) => panic!("{name} is not a group"),
            }
        }
    }
}
