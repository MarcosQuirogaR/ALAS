// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass items a military freighter carries and a passenger transport does not.
//!
//! FLOPS equation 56 prices the military cargo floor (`CARGF`, +38 percent of
//! the fuselage structure), and FLOPS has no separate term for the powered
//! cargo-handling system, the tie-down and jacking fittings or the
//! loading-ramp mechanism. Three handbook statistics price them for a fuselage
//! that does not already carry them (see *One owner per item*), each read on
//! the page:
//!
//! - Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., section 15.3.2
//!   (book p. 575, equation 15.45): military cargo handling system
//!   `W = 2.4 lb/ft^2 x cargo floor area` (the powered rollers that move
//!   pallets into position and lock them down, as the text defines them).
//! - Raymer, same page, equation 15.44: handling gear `W = 3.0e-4 W_dg`
//!   (jacking pads, tie-downs, tow-hook attachments).
//! - Jenkinson, *Civil Jet Aircraft Design*, section 17 (book p. 387):
//!   rear-loading ramps cost "7-9% of fuselage mass" added to the basic
//!   fuselage mass; the midpoint 8 percent is used (a published range, not a
//!   fit to any aircraft).
//!
//! The terms apply to an aircraft that declares a main-deck freight hold, and
//! to nothing else, so every passenger transport is bit-identical.
//!
//! # One owner per item
//!
//! Each item has one owner, fixed by the group definition of the method that
//! prices the fuselage. Beltramo et al., *Parametric Study of Transport
//! Aircraft Systems Cost and Weight*, NASA CR-151970 (1977), pp. 4-3 and 4-4,
//! defines the body system as the shell, door and window frames, doors,
//! windows, floors, bulkheads, windshield, radome and tailcone, with the door
//! actuation mechanisms and airstairs; it puts the cargo loading system in the
//! body only for the C-5A and AST(M), where it is built integral with the floor.
//!
//! - Ramp: a door with its actuation, so it belongs to a military-transport
//!   fuselage statistic. FLOPS equation 56 is one when it carries its military
//!   cargo floor term (`CARGF = 1`, the only military value NASA TM-2017-219627
//!   gives; the TM lists no hardware for it). Under that fuselage the ramp item
//!   is zero. Any other fuselage equation (equation 56 with `CARGF = 0`, or the
//!   regional-turboprop pressurized method, which has no cargo-floor term) is a
//!   basic fuselage, and the ramp is charged as Jenkinson's increment.
//! - Cargo-handling system (Raymer equation 15.45): always its own item. No
//!   source puts it inside a fuselage statistic other than the two integral
//!   floors named above.
//! - Handling gear (Raymer equation 15.44): always its own item.
//!
//! # Civil main-deck freighters
//!
//! A civil freighter that carries freight on its main deck (a production
//! freighter such as the B747-400F or DC-10-30F, or a converted passenger
//! aircraft such as the A320P2F) has the same main-deck hardware a passenger
//! fuselage lacks, whichever way it was built:
//!
//! - the large main-deck cargo door and its surround: Jenkinson, Simpkin and
//!   Rhodes, *Civil Jet Aircraft Design* (1999), section 7.2, p. 136, amend
//!   the basic fuselage mass "increased 10% for large cargo door (etc.)
//!   discontinuity" ([`MAIN_DECK_CARGO_DOOR_FUSELAGE_FRACTION`]);
//! - the pallet handling system on the main-deck floor (Raymer equation 15.45)
//!   and the handling gear (Raymer equation 15.44), priced exactly as for a
//!   military hold over the main-deck floor (cabin length times the maximum
//!   fuselage width, an upper bound of the loadable floor by the lining and
//!   frame depth).
//!
//! The build differs in one respect. A production freighter is built
//! without passenger systems (Jane's All the World's Aircraft 2008-2009,
//! A300-600F entry; the 777F entry lists a modified environmental control
//! system), so FLOPS sizes its APU, air conditioning and electrical
//! equations for no passengers. A conversion removes the cabin outfit but
//! keeps those systems; Torenbeek, *Synthesis of Subsonic Airplane Design*
//! (1982), section 3.3.2, p. 82, notes that a converted passenger transport
//! saves only the furnishings, for a minor OEW reduction. The preset seeds
//! the seats of the airliner a conversion was built from
//! (`converted_freighter_seats`), and those three equations size for them.
//!
//! The doors, the freight floor and the handling system are placed where the
//! hardware is ([`super::main_deck_freight`]), not at the group stations.
//!
//! The rule reads one attribute, [`carries_main_deck_freight`]: a cargo
//! aircraft that declares a main-deck cargo door and no military freight hold.
//! A military hold keeps its own ramp and handling items above, so no item has
//! two owners, and every passenger transport is bit-identical.

use alas_config::{CabinConfig, DesignRequirements};

use super::units::{ft2, kg, lb};

/// Jenkinson, Simpkin and Rhodes (1999), section 7.2, p. 136: fuselage mass
/// increment for a large cargo door discontinuity.
pub const MAIN_DECK_CARGO_DOOR_FUSELAGE_FRACTION: f64 = 0.10;

/// Whether the aircraft is a civil main-deck freighter: a cargo aircraft with
/// a declared main-deck cargo door and no military freight hold.
pub fn carries_main_deck_freight(requirements: &DesignRequirements, cabin: &CabinConfig) -> bool {
    requirements.aircraft_type == "cargo"
        && cabin.cargo.freight_hold.is_none()
        && cabin.cargo.main_deck_door_clear_height_m.is_some()
}

/// The main-deck floor of a civil freighter as the hold the handling items
/// are priced over: cabin length times the maximum fuselage width, no ramp.
pub fn civil_main_deck_hold(
    fuselage: &alas_geom::aircraft::fuselage::Fuselage,
    geometry: &alas_config::GeometryConfig,
) -> MilitaryCargoHold {
    let (width, _) = super::product::max_fuselage_width_depth(fuselage);
    let length = fuselage.xsecs.last().map_or(0.0, |last| last.xyz_c[0])
        - fuselage.xsecs.first().map_or(0.0, |first| first.xyz_c[0]);
    let compartment_length =
        (length - geometry.fuselage.cabin_start_x_m - geometry.fuselage.tailcone_length_m).max(0.0);
    MilitaryCargoHold {
        floor_area_m2: compartment_length * width,
        rear_ramp: false,
        fuselage_owns_ramp: false,
        military: false,
        body_wetted_area_m2: fuselage.area_wetted(),
    }
}

/// Raymer equation 15.45 coefficient, lb per ft^2 of cargo floor.
const CARGO_HANDLING_LB_PER_FT2: f64 = 2.4;
/// Raymer equation 15.44 coefficient, fraction of the design gross weight.
const HANDLING_GEAR_FRACTION: f64 = 3.0e-4;
/// Midpoint of Jenkinson's 7-9 percent rear-loading-ramp fuselage increment.
const REAR_RAMP_FUSELAGE_FRACTION: f64 = 0.08;

/// The declared main-deck freight hold of a military transport or freighter.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct MilitaryCargoHold {
    /// Loadable cargo floor area, m^2, including a load-bearing ramp.
    pub floor_area_m2: f64,
    /// Whether the hold has a rear-loading ramp.
    pub rear_ramp: bool,
    /// Whether the fuselage equation in use is the military-transport one
    /// (FLOPS equation 56 with `CARGF = 1`), which owns the ramp.
    pub fuselage_owns_ramp: bool,
    /// Whether the hold is a declared military freight hold
    /// (`cabin.cargo.freight_hold`), not a civil main-deck freighter's floor.
    #[serde(default)]
    pub military: bool,
    /// Wetted area of the built fuselage, m^2: the body wetted area `Sb` of
    /// NASA CR-151970 that sizes the military avionics group.
    #[serde(default)]
    pub body_wetted_area_m2: f64,
}

/// The three military cargo items, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MilitaryCargoEquipment {
    /// Powered rollers, rails and locks, Raymer equation 15.45.
    pub handling_system_kg: f64,
    /// Jacking pads, tie-downs and tow fittings, Raymer equation 15.44.
    pub handling_gear_kg: f64,
    /// Rear ramp and door mechanism, Jenkinson 8 percent of basic fuselage.
    pub ramp_kg: f64,
}

impl MilitaryCargoEquipment {
    /// Sum of the three items, kg.
    pub fn total_kg(&self) -> f64 {
        self.handling_system_kg + self.handling_gear_kg + self.ramp_kg
    }
}

/// Price the military cargo items for a hold. `basic_fuselage_kg` is the
/// fuselage of equation 56 without the military floor factor.
pub fn military_cargo_equipment(
    hold: &MilitaryCargoHold,
    design_gross_mass_kg: f64,
    basic_fuselage_kg: f64,
) -> MilitaryCargoEquipment {
    // Owners per the module docs: the ramp is the fuselage's only under a
    // military-transport fuselage equation.
    MilitaryCargoEquipment {
        handling_system_kg: kg(CARGO_HANDLING_LB_PER_FT2 * ft2(hold.floor_area_m2)),
        handling_gear_kg: kg(HANDLING_GEAR_FRACTION * lb(design_gross_mass_kg)),
        ramp_kg: if hold.rear_ramp && !hold.fuselage_owns_ramp {
            REAR_RAMP_FUSELAGE_FRACTION * basic_fuselage_kg
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_items_follow_the_published_coefficients() {
        let hold = MilitaryCargoHold {
            floor_area_m2: 100.0,
            rear_ramp: true,
            fuselage_owns_ramp: false,
            military: true,
            body_wetted_area_m2: 0.0,
        };
        let items = military_cargo_equipment(&hold, 100_000.0, 10_000.0);
        // 100 m^2 = 1076.39 ft^2 x 2.4 lb/ft^2 = 2583.3 lb = 1171.8 kg.
        assert!((items.handling_system_kg - 1_171.8).abs() < 0.5);
        assert!((items.handling_gear_kg - 30.0).abs() < 1e-9);
        assert!((items.ramp_kg - 800.0).abs() < 1e-9);
        let nose_loader = MilitaryCargoHold {
            rear_ramp: false,
            ..hold
        };
        assert_eq!(
            military_cargo_equipment(&nose_loader, 100_000.0, 10_000.0).ramp_kg,
            0.0
        );
    }

    #[test]
    fn only_a_cargo_aircraft_with_a_main_deck_door_and_no_military_hold_is_a_main_deck_freighter() {
        let mut requirements = DesignRequirements::default();
        let mut cabin = CabinConfig::default();
        cabin.cargo.main_deck_door_clear_height_m = Some(2.2);
        requirements.aircraft_type = "passenger".to_string();
        assert!(!carries_main_deck_freight(&requirements, &cabin));
        requirements.aircraft_type = "cargo".to_string();
        assert!(carries_main_deck_freight(&requirements, &cabin));
        cabin.cargo.main_deck_door_clear_height_m = None;
        assert!(!carries_main_deck_freight(&requirements, &cabin));
    }

    #[test]
    fn a_military_transport_fuselage_owns_the_ramp_only() {
        let owned = MilitaryCargoHold {
            floor_area_m2: 100.0,
            rear_ramp: true,
            fuselage_owns_ramp: true,
            military: true,
            body_wetted_area_m2: 0.0,
        };
        let items = military_cargo_equipment(&owned, 100_000.0, 10_000.0);
        assert_eq!(items.ramp_kg, 0.0);
        assert!((items.handling_system_kg - 1_171.8).abs() < 0.5);
        assert!((items.handling_gear_kg - 30.0).abs() < 1e-9);
    }
}
