// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py (`CargoDeckConfig`)

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
//! has one sharp edge: a target
//! centre of gravity at or below zero means the centre of the envelope, so
//! there is no way to ask for a trim point at the datum itself.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// How checked baggage is divided between the hold compartments.
///
/// No regulation prescribes the split; operators either load in proportion to
/// compartment volume or trim to a balance target (engineering estimate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaggagePolicy {
    /// Trim the split within the compartment limits toward the balance target.
    #[default]
    TargetCg,
    /// Split in proportion to compartment volume, ignoring balance.
    VolumeProportional,
}

impl BaggagePolicy {
    /// Whether this is the default policy, which serialization omits.
    pub fn is_default(&self) -> bool {
        *self == Self::TargetCg
    }
}

/// Which deck a declared compartment is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldDeck {
    /// Under the cabin floor.
    #[default]
    Lower,
    /// On the passenger main deck, outside the seat and monument extent.
    Main,
}

/// One declared baggage compartment. A non-empty declared list replaces the
/// compartments derived from the geometry.
///
/// Stations are metres aft of the nose tip; volume is cubic metres and the
/// optional net-mass limit is kilograms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldCompartmentConfig {
    /// Display name.
    pub name: String,
    /// Forward station, m.
    pub x_start_m: f64,
    /// Aft station, m.
    pub x_end_m: f64,
    /// Usable volume, m3.
    pub volume_m3: f64,
    /// Structural net-mass limit, kg, when one is published.
    #[serde(default)]
    pub max_net_kg: Option<f64>,
    /// Which deck the compartment is on.
    #[serde(default)]
    pub deck: HoldDeck,
}

impl HoldCompartmentConfig {
    /// Whether the extent, volume and limit are finite and physical.
    pub fn is_valid(&self) -> bool {
        self.x_start_m.is_finite()
            && self.x_end_m.is_finite()
            && self.x_end_m > self.x_start_m
            && self.volume_m3.is_finite()
            && self.volume_m3 > 0.0
            && self
                .max_net_kg
                .is_none_or(|limit| limit.is_finite() && limit >= 0.0)
    }
}

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
        options = MainDeckUld,
        help = "Unit load device code for the main deck: the pallets and boxes a freighter's main deck takes, which are far larger than anything that fits a lower hold."
    )]
    pub main_deck_uld: String,

    /// Which container the lower holds are loaded with.
    #[config(
        options = LowerDeckUld,
        help = "Loading format for lower holds, including passenger baggage. 'BLK' permits loose bulk only and never enables a container system. Use 'AUTO' to compare physically feasible uniform ULD formats. Other explicit codes degrade to a shorter container and then bulk when they cannot fit."
    )]
    pub lower_deck_uld: String,

    /// How the load is distributed among the available positions.
    #[config(
        options = CargoLoadingStrategy,
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

    /// How checked baggage is split between compartments.
    #[serde(default, skip_serializing_if = "BaggagePolicy::is_default")]
    #[config(
        hidden,
        options = BaggagePolicy,
        help = "How checked baggage is divided between hold compartments: 'target_cg' trims the split toward the balance target within each compartment's limit, 'volume_proportional' splits it in proportion to compartment volume."
    )]
    pub baggage_policy: BaggagePolicy,

    /// Declared hold compartments, replacing the derived ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[config(
        hidden,
        help = "Declared baggage compartments with station extent, volume and optional net-mass limit. Empty means the compartments are derived from the fuselage geometry."
    )]
    pub hold_compartments: Vec<HoldCompartmentConfig>,
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
            baggage_policy: BaggagePolicy::TargetCg,
            hold_compartments: Vec::new(),
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
    fn an_older_cargo_config_without_the_baggage_fields_still_loads() {
        let mut value = serde_json::to_value(CargoDeckConfig::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        assert!(!object.contains_key("baggage_policy"));
        assert!(!object.contains_key("hold_compartments"));
        let loaded: CargoDeckConfig = serde_json::from_value(value).unwrap();
        assert_eq!(loaded, CargoDeckConfig::default());
    }

    #[test]
    fn non_default_baggage_fields_round_trip_in_snake_case() {
        let config = CargoDeckConfig {
            baggage_policy: BaggagePolicy::VolumeProportional,
            hold_compartments: vec![HoldCompartmentConfig {
                name: "Aft".to_owned(),
                x_start_m: 22.0,
                x_end_m: 24.0,
                volume_m3: 4.0,
                max_net_kg: Some(600.0),
                deck: HoldDeck::Main,
            }],
            ..CargoDeckConfig::default()
        };
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["baggage_policy"], "volume_proportional");
        assert_eq!(value["hold_compartments"][0]["deck"], "main");
        let loaded: CargoDeckConfig = serde_json::from_value(value).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn a_compartment_needs_a_positive_extent_and_volume() {
        let good = HoldCompartmentConfig {
            name: "Fwd".to_owned(),
            x_start_m: 3.0,
            x_end_m: 4.0,
            volume_m3: 2.0,
            max_net_kg: None,
            deck: HoldDeck::Lower,
        };
        assert!(good.is_valid());
        for bad in [
            HoldCompartmentConfig {
                x_end_m: 3.0,
                ..good.clone()
            },
            HoldCompartmentConfig {
                volume_m3: 0.0,
                ..good.clone()
            },
            HoldCompartmentConfig {
                volume_m3: f64::NAN,
                ..good.clone()
            },
            HoldCompartmentConfig {
                max_net_kg: Some(-1.0),
                ..good.clone()
            },
        ] {
            assert!(!bad.is_valid());
        }
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
    fn cargo_choice_fields_declare_their_option_sources() {
        let schema = CargoDeckConfig::default().schema();
        for (name, options) in [
            ("main_deck_uld", crate::OptionSource::MainDeckUld),
            ("lower_deck_uld", crate::OptionSource::LowerDeckUld),
            (
                "loading_strategy",
                crate::OptionSource::CargoLoadingStrategy,
            ),
        ] {
            match &schema.field(name).unwrap().entry {
                crate::Entry::Leaf(leaf) => assert_eq!(leaf.options, Some(options), "{name}"),
                crate::Entry::Node(_) => panic!("{name} is not a group"),
            }
        }
    }
}
