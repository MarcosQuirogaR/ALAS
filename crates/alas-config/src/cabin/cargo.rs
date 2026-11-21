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
    /// Usable volume, m3, when the source publishes it; `None` takes the
    /// enclosed volume of the built deck over the declared extent.
    #[serde(default)]
    pub volume_m3: Option<f64>,
    /// Structural net-mass limit, kg, when one is published.
    #[serde(default)]
    pub max_net_kg: Option<f64>,
    /// Which deck the compartment is on.
    #[serde(default)]
    pub deck: HoldDeck,
    /// Number of container rows the source prints for a lower hold, when it does.
    /// The rows are then spread evenly over the compartment (pitch = length / rows)
    /// instead of standing at the container length plus the default clearance.
    #[serde(default)]
    pub container_rows: Option<u32>,
    /// Clear width of a lower hold, m, where its source draws the compartment
    /// narrower than the built deck: it bounds the enclosed volume and the
    /// containers placed across. `None` takes the built deck's width.
    #[serde(default)]
    pub clear_width_m: Option<f64>,
    /// A forward part of the compartment narrower than the rest, as
    /// `[length, clear width]` in metres (the A380's T-shaped forward aft
    /// compartment). `None` when the compartment has none.
    #[serde(default)]
    pub narrow_forward_part_m: Option<[f64; 2]>,
    /// The compartment stands behind the last seat row of the main deck (a
    /// business jet's aft cabin baggage area, whose certified loading arm the
    /// type certificate prints): its stations bound the main-deck compartment
    /// the cabin derives there instead of reserving seat floor, so the seats
    /// keep their layout and the compartment never reaches forward into them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub behind_seats: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl HoldCompartmentConfig {
    /// Whether the extent, volume and limit are finite and physical.
    pub fn is_valid(&self) -> bool {
        self.x_start_m.is_finite()
            && self.x_end_m.is_finite()
            && self.x_end_m > self.x_start_m
            && self
                .volume_m3
                .is_none_or(|volume| volume.is_finite() && volume > 0.0)
            && self
                .max_net_kg
                .is_none_or(|limit| limit.is_finite() && limit >= 0.0)
            && self.container_rows.is_none_or(|rows| rows >= 1)
            && self
                .clear_width_m
                .is_none_or(|width| width.is_finite() && width > 0.0)
            && self.narrow_forward_part_m.is_none_or(|[length, width]| {
                length.is_finite()
                    && length > 0.0
                    && length <= self.x_end_m - self.x_start_m
                    && width.is_finite()
                    && width > 0.0
            })
    }

    /// The clear width the source draws at station `x`, m, or `None` where
    /// the built deck's own width applies.
    pub fn clear_width_at(&self, x: f64) -> Option<f64> {
        match self.narrow_forward_part_m {
            Some([length, width]) if x < self.x_start_m + length => Some(width),
            _ => self.clear_width_m,
        }
    }
}

/// A military or freighter main-deck hold: one continuous loadable floor plus a
/// load-carrying ramp, loaded with pallets rather than underfloor containers.
///
/// A body with a declared freight hold has no underfloor hold and no cabin
/// seat deck: the whole payload rides on this floor. Stations are metres aft
/// of the nose tip; the clear width and height are the interior dimensions the
/// source publishes, not the outer loft.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreightHoldConfig {
    /// Forward end of the loadable floor, m.
    pub floor_start_x_m: f64,
    /// Floor length ahead of the ramp hinge, m.
    pub floor_length_m: f64,
    /// Ramp length, m. The ramp continues the floor aft of the hinge.
    pub ramp_length_m: f64,
    /// Clear width at the floor, m: sets how many pallets stand side by side.
    pub clear_width_m: f64,
    /// Clear height above the floor under the lowest part of the roof, m.
    pub clear_height_m: f64,
    /// Total load the ramp may carry, kg, or `None` when the source states no
    /// ramp limit (the ramp is then as strong as the floor).
    #[serde(default)]
    pub ramp_load_limit_kg: Option<f64>,
}

impl FreightHoldConfig {
    /// Whether every dimension is finite and positive and the limit sane.
    pub fn is_valid(&self) -> bool {
        [
            self.floor_start_x_m,
            self.floor_length_m,
            self.clear_width_m,
            self.clear_height_m,
        ]
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
            && self.ramp_length_m.is_finite()
            && self.ramp_length_m >= 0.0
            && self
                .ramp_load_limit_kg
                .is_none_or(|limit| limit.is_finite() && limit >= 0.0)
    }

    /// Station of the ramp hinge, m: floor ahead of it, ramp aft of it.
    pub fn ramp_start_x_m(&self) -> f64 {
        self.floor_start_x_m + self.floor_length_m
    }

    /// Aft end of the ramp, m.
    pub fn aft_end_x_m(&self) -> f64 {
        self.ramp_start_x_m() + self.ramp_length_m
    }
}

/// Cargo loading configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct CargoDeckConfig {
    /// Whether freight is carried on the main deck as well as below.
    #[config(
        help = "Carry freight on the main deck, as a dedicated freighter does, rather than only in the lower holds. A passenger aircraft has no main-deck cargo.",
        long_help = "Carry freight on the main deck, as a dedicated freighter does, rather than only in the lower holds. A passenger aircraft has no main-deck cargo, so this distinguishes the two configurations."
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
        help = "Loading format for lower holds, including passenger baggage.",
        long_help = "Loading format for lower holds, including passenger baggage. 'BLK' permits loose bulk only and never enables a container system. Use 'AUTO' to compare physically feasible uniform ULD formats. Other explicit codes degrade to a shorter container and then bulk when they cannot fit."
    )]
    pub lower_deck_uld: String,

    /// How the load is distributed among the available positions.
    #[config(
        options = CargoLoadingStrategy,
        help = "How to place the load: 'target_cg' trims to the target CG, 'min_pallets' concentrates full containers, 'door_proximity' favours doors, 'uniform' spreads evenly.",
        long_help = "How to place the load: 'target_cg' spreads it and trims to the target centre of gravity, 'min_pallets' concentrates full containers near that point, 'door_proximity' favours the positions nearest the doors for a fast turnaround, and 'uniform' spreads it evenly regardless."
    )]
    pub loading_strategy: String,

    /// Where the loaded centre of gravity is trimmed to.
    #[config(
        help = "Centre of gravity the load is trimmed toward [% MAC].",
        long_help = "Centre of gravity the load is trimmed toward, as a percentage of mean aerodynamic chord. Zero or below means the centre of the CG envelope, so the loader targets a valid point without being told where one is."
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
        help = "Iteration cap for the CG trim. A very large aircraft with small steps may need more than the default; raise this rather than the step if the trim does not converge.",
        long_help = "Iteration cap for the centre-of-gravity trim. A very large aircraft moving a small step per iteration can need more than the default; raise this rather than the step if the trim is not converging."
    )]
    pub cg_trim_max_iterations: i64,

    /// How checked baggage is split between compartments.
    #[serde(default, skip_serializing_if = "BaggagePolicy::is_default")]
    #[config(
        hidden,
        options = BaggagePolicy,
        help = "How checked baggage is split between hold compartments: 'target_cg' trims toward the balance target, 'volume_proportional' splits by compartment volume.",
        long_help = "How checked baggage is divided between hold compartments: 'target_cg' trims the split toward the balance target within each compartment's limit, 'volume_proportional' splits it in proportion to compartment volume."
    )]
    pub baggage_policy: BaggagePolicy,

    /// Declared hold compartments, replacing the derived ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[config(
        hidden,
        help = "Declared baggage compartments with station extent, volume and optional net-mass limit. Empty means the compartments are derived from the fuselage geometry."
    )]
    pub hold_compartments: Vec<HoldCompartmentConfig>,
    /// Declared main-deck freight hold of a military or freighter body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Declared freight hold (floor, ramp, clear width and height) of a military transport or freighter.",
        long_help = "Declared freight hold (floor, ramp, clear width and height) of a military transport or freighter. When present the payload rides on pallets along this floor and the body has no underfloor hold."
    )]
    pub freight_hold: Option<FreightHoldConfig>,
    /// Density the hold's freight and baggage stow at, kg/m3, or the ICAO default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Stowage density of freight and baggage [kg/m^3]: a container carries at most its volume times this.",
        long_help = "Stowage density of the freight and baggage, kg per cubic metre of container or hold volume: a container carries at most its volume times this density. Unset uses the ICAO default conversion factor, 161 kg/m3, which suits general freight and baggage; denser freight (machinery, metals) needs a higher value."
    )]
    pub stowage_density_kg_m3: Option<f64>,
    /// Height main-deck loads are built up to, m, when the source prints it: the
    /// cargo door clear height, or the lower load contour drawn for the deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Height main-deck loads are built up to [m]: the cargo door clear height or the deck's lower load contour; below it they follow the section. Unset: up to the section."
    )]
    pub main_deck_door_clear_height_m: Option<f64>,
    /// Clear width of the main-deck cargo door, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Clear width of the main-deck cargo door [m]: with the clear height it sizes the door placed at its station in the mass ledger."
    )]
    pub main_deck_door_clear_width_m: Option<f64>,
    /// Centreline of the main-deck cargo door, m aft of the nose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Centreline of the main-deck cargo door [m aft of the nose]: where the mass ledger places the door. Unset: the door stays with the freight floor at mid-deck."
    )]
    pub main_deck_door_station_m: Option<f64>,
    /// Width of a freighter's nose loading door, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Width of the nose loading door at its break [m]. Unset: the freighter has no nose door."
    )]
    pub nose_cargo_door_width_m: Option<f64>,
    /// Height of a freighter's nose loading door, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Height of the nose loading door at its break [m]. Unset: the freighter has no nose door."
    )]
    pub nose_cargo_door_height_m: Option<f64>,
    /// Passenger seats of the airliner a converted main-deck freighter was
    /// built from, when the type's freighter is a conversion; unset for a
    /// production freighter. The conversion keeps the systems that airliner
    /// was sized for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Seats of the airliner a converted freighter was built from: a conversion keeps the APU, air conditioning and electrical systems sized for them. Unset is a production freighter, whose systems are sized for no passengers."
    )]
    pub converted_freighter_seats: Option<u32>,
    /// Runtime load-plan input, never serialised: the payload centroid, m, the
    /// loaded containers are relocated toward without changing the container
    /// set or any container's load (`alas-pipeline` load plan).
    #[serde(skip)]
    #[config(skip)]
    pub plan_payload_cg_x_m: Option<f64>,
}

impl Default for CargoDeckConfig {
    fn default() -> Self {
        Self {
            use_main_deck: true,
            main_deck_uld: "AUTO".to_owned(),
            lower_deck_uld: "AUTO".to_owned(),
            loading_strategy: "target_cg".to_owned(),
            target_cg_pct_mac: 25.0,
            main_door_x_m: 0.0,
            fwd_door_x_m: 0.0,
            aft_door_x_m: 0.0,
            cg_trim_step_kg: 50.0,
            cg_trim_max_iterations: 2000,
            baggage_policy: BaggagePolicy::TargetCg,
            hold_compartments: Vec::new(),
            freight_hold: None,
            stowage_density_kg_m3: None,
            main_deck_door_clear_height_m: None,
            main_deck_door_clear_width_m: None,
            main_deck_door_station_m: None,
            nose_cargo_door_width_m: None,
            nose_cargo_door_height_m: None,
            converted_freighter_seats: None,
            plan_payload_cg_x_m: None,
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
                volume_m3: Some(4.0),
                max_net_kg: Some(600.0),
                deck: HoldDeck::Main,
                container_rows: None,
                clear_width_m: None,
                narrow_forward_part_m: None,
                behind_seats: true,
            }],
            ..CargoDeckConfig::default()
        };
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["baggage_policy"], "volume_proportional");
        assert_eq!(value["hold_compartments"][0]["deck"], "main");
        assert_eq!(value["hold_compartments"][0]["behind_seats"], true);
        let loaded: CargoDeckConfig = serde_json::from_value(value).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn a_compartment_needs_a_positive_extent_and_volume() {
        let good = HoldCompartmentConfig {
            name: "Fwd".to_owned(),
            x_start_m: 3.0,
            x_end_m: 4.0,
            volume_m3: Some(2.0),
            max_net_kg: None,
            deck: HoldDeck::Lower,
            container_rows: None,
            clear_width_m: None,
            narrow_forward_part_m: None,
            behind_seats: false,
        };
        assert!(good.is_valid());
        // The flag is left out of a serialized compartment that does not set it.
        assert!(!serde_json::to_value(&good)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("behind_seats"));
        for bad in [
            HoldCompartmentConfig {
                x_end_m: 3.0,
                ..good.clone()
            },
            HoldCompartmentConfig {
                volume_m3: Some(0.0),
                ..good.clone()
            },
            HoldCompartmentConfig {
                volume_m3: Some(f64::NAN),
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
    fn a_freight_hold_needs_finite_positive_dimensions_and_round_trips() {
        let good = FreightHoldConfig {
            floor_start_x_m: 8.0,
            floor_length_m: 17.71,
            ramp_length_m: 5.40,
            clear_width_m: 4.00,
            clear_height_m: 3.85,
            ramp_load_limit_kg: Some(6_000.0),
        };
        assert!(good.is_valid());
        assert!((good.ramp_start_x_m() - 25.71).abs() < 1.0e-12);
        assert!((good.aft_end_x_m() - 31.11).abs() < 1.0e-12);
        for bad in [
            FreightHoldConfig {
                floor_length_m: 0.0,
                ..good
            },
            FreightHoldConfig {
                clear_width_m: f64::NAN,
                ..good
            },
            FreightHoldConfig {
                ramp_length_m: -1.0,
                ..good
            },
            FreightHoldConfig {
                ramp_load_limit_kg: Some(-1.0),
                ..good
            },
        ] {
            assert!(!bad.is_valid());
        }
        let config = CargoDeckConfig {
            freight_hold: Some(good),
            ..CargoDeckConfig::default()
        };
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["freight_hold"]["floor_length_m"], 17.71);
        let loaded: CargoDeckConfig = serde_json::from_value(value).unwrap();
        assert_eq!(loaded, config);
        let default_value = serde_json::to_value(CargoDeckConfig::default()).unwrap();
        assert!(!default_value
            .as_object()
            .unwrap()
            .contains_key("freight_hold"));
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
