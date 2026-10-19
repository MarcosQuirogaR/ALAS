// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass items a military freighter carries and a passenger transport does not.
//!
//! FLOPS equation 56 prices the military cargo floor (`CARGF`, +38 percent of
//! the fuselage structure) and equation 110 drops the passenger cabin, but
//! FLOPS has no term for the powered cargo-handling system, the tie-down and
//! jacking fittings or the loading-ramp mechanism. Three handbook statistics
//! supply them, each read on the page:
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

use super::units::{ft2, kg, lb};

/// Raymer equation 15.45 coefficient, lb per ft^2 of cargo floor.
const CARGO_HANDLING_LB_PER_FT2: f64 = 2.4;
/// Raymer equation 15.44 coefficient, fraction of the design gross weight.
const HANDLING_GEAR_FRACTION: f64 = 3.0e-4;
/// Midpoint of Jenkinson's 7-9 percent rear-loading-ramp fuselage increment.
const REAR_RAMP_FUSELAGE_FRACTION: f64 = 0.08;

/// The declared main-deck freight hold of a military transport or freighter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MilitaryCargoHold {
    /// Loadable cargo floor area, m^2, including a load-bearing ramp.
    pub floor_area_m2: f64,
    /// Whether the hold has a rear-loading ramp.
    pub rear_ramp: bool,
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
    MilitaryCargoEquipment {
        handling_system_kg: kg(CARGO_HANDLING_LB_PER_FT2 * ft2(hold.floor_area_m2)),
        handling_gear_kg: kg(HANDLING_GEAR_FRACTION * lb(design_gross_mass_kg)),
        ramp_kg: if hold.rear_ramp {
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
}
