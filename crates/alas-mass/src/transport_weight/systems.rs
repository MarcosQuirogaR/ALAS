// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Common.systems.systems and
// mission reference.Methods.Weights.Correlations.Transport.operating_items.operating_items.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! On-board systems and operating-items masses.
//!
//! Both functions branch on `vehicle.systems.accessories`, an aircraft-type
//! category. Worth recording, because it silently steers this program's
//! results: `external tools/mission_runner/vehicle_builder.py` sets
//! `vehicle.systems.accessories = "long range"` (a space), while every branch
//! upstream tests against the *hyphenated* spellings (`"long-range"`,
//! `"short-range"`, `"medium-range"`). The two never match, so `systems` and
//! `operating_items` both fall through to their `else` case for every vehicle
//! this program builds -- the [`AccessoriesType::Other`] variant here.
//! Reproduced, not corrected.

use alas_units::POUND_MASS;

use super::{OperationalItems, SystemsBreakdown};

/// `vehicle.systems.control` -- how the flight control system is powered,
/// which scales the flight-control group's weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSystemType {
    /// `"fully powered"` -- every vehicle this program's mission reference bridge builds.
    FullyPowered,
    /// `"partially powered"`.
    PartiallyPowered,
    /// Anything else -- upstream's fully-aerodynamic `else` branch.
    Other,
}

/// `vehicle.systems.accessories` -- the aircraft-type category that selects
/// the instruments, avionics, furnishing and operating-item allowances.
///
/// [`Other`](Self::Other) is upstream's `else` branch, and is what every
/// vehicle this program builds actually reaches; see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessoriesType {
    /// `"short-range"` -- short-range domestic, austere accommodation.
    ShortRange,
    /// `"medium-range"` -- medium-range domestic.
    MediumRange,
    /// `"long-range"` -- long-range overwater.
    LongRange,
    /// `"business"` -- business jet.
    Business,
    /// `"cargo"` -- all-cargo.
    Cargo,
    /// `"commuter"` -- commuter.
    Commuter,
    /// `"sst"` -- supersonic transport.
    Sst,
    /// Anything else -- upstream's `else` branch, which every vehicle this
    /// program builds actually reaches.
    Other,
}

/// The mass of the on-board systems -- `systems`
/// (http://aerodesign.stanford.edu/aircraftdesign/structures/componentweight.html).
///
/// `tail_area_m2` is `sum(wing.areas.reference)` over the horizontal and
/// vertical tails; `main_wing_area_m2` feeds only the BWB fallback upstream
/// applies when there is no tail area at all.
/// Evaluate the mission reference `systems` correlation from its explicit inputs.
///
/// This is exported for comparison evidence and for a future explicitly
/// selected subsystem method. It is not the product mass-buildup path:
/// [`crate::breakdown::calculate_component_masses`] retains the frozen ALAS
/// fraction model until a separately validated replacement is selected.
pub fn systems(
    passenger_count: u32,
    control_type: ControlSystemType,
    accessories_type: AccessoriesType,
    reference_area_m2: f64,
    tail_area_m2: f64,
    main_wing_area_m2: f64,
) -> SystemsBreakdown {
    let num_seats = f64::from(passenger_count);
    let s_ref_ft2 = reference_area_m2 / (alas_units::FOOT * alas_units::FOOT);

    // With no tail (a BWB), upstream assumes the flight controls live on the
    // wing and charges 1% of its area instead. Kept faithfully, though every
    // vehicle this program builds has both tails.
    let s_tail_m2 = if tail_area_m2 == 0.0 {
        main_wing_area_m2 * 0.01
    } else {
        tail_area_m2
    };
    let area_hv_ft2 = s_tail_m2 / (alas_units::FOOT * alas_units::FOOT);

    let flt_ctrl_scaler = match control_type {
        ControlSystemType::FullyPowered => 3.5,
        ControlSystemType::PartiallyPowered => 2.5,
        ControlSystemType::Other => 1.7,
    };
    let flt_ctrl_kg = (flt_ctrl_scaler * area_hv_ft2) * POUND_MASS;

    // The APU floor is `max(apu, 70.)` upstream, where `apu` has already been
    // converted to kg (via `* Units.lb`) but `70` carries no unit -- so it
    // acts as a 70 kg floor, not the 70 lb the surrounding lb-scaled formula
    // reads as. Reproduced literally.
    let apu_raw_kg = if num_seats >= 6.0 {
        7.0 * num_seats * POUND_MASS
    } else {
        // Upstream's `0.0 * Units.lb`; zero either way.
        0.0
    };
    let apu_kg = apu_raw_kg.max(70.0);

    let hyd_pnu_kg = (0.65 * s_ref_ft2) * POUND_MASS;

    let mut elec_kg = (13.0 * num_seats) * POUND_MASS;

    let mut furnish_kg =
        ((43.7 - 0.037 * num_seats.min(300.0)) * num_seats + 46.0 * num_seats) * POUND_MASS;

    let ac_kg = (15.0 * num_seats) * POUND_MASS;

    let (instruments_kg, avionics_kg) = match accessories_type {
        AccessoriesType::ShortRange | AccessoriesType::MediumRange | AccessoriesType::Other => {
            (800.0 * POUND_MASS, 900.0 * POUND_MASS)
        }
        AccessoriesType::LongRange | AccessoriesType::Sst => {
            furnish_kg += 23.0 * num_seats * POUND_MASS;
            (1200.0 * POUND_MASS, 1500.0 * POUND_MASS)
        }
        AccessoriesType::Business => (100.0 * POUND_MASS, 300.0 * POUND_MASS),
        AccessoriesType::Cargo => {
            elec_kg = 1950.0 * POUND_MASS;
            (800.0 * POUND_MASS, 900.0 * POUND_MASS)
        }
        AccessoriesType::Commuter => (300.0 * POUND_MASS, 500.0 * POUND_MASS),
    };

    // Anti-ice is folded into the air conditioner upstream
    // (`air_conditioner = wt_ac + wt_anti_ice`, with `wt_anti_ice = 0`).
    let air_conditioner_kg = ac_kg;

    let total_kg = flt_ctrl_kg
        + apu_kg
        + hyd_pnu_kg
        + air_conditioner_kg
        + avionics_kg
        + elec_kg
        + furnish_kg
        + instruments_kg;

    SystemsBreakdown {
        control_systems_kg: flt_ctrl_kg,
        apu_kg,
        electrical_kg: elec_kg,
        avionics_kg,
        hydraulics_kg: hyd_pnu_kg,
        furnish_kg,
        air_conditioner_kg,
        instruments_kg,
        total_kg,
    }
}

/// The mass of the operating items -- crew, unusable fuel, engine oil,
/// passenger service and cargo containers -- `operating_items`
/// (http://aerodesign.stanford.edu/aircraftdesign/AircraftDesign.html).
/// Evaluate mission reference's operating-items correlation from its explicit inputs.
///
/// As with [`systems`], callers must identify the accessory category rather
/// than treating this as a replacement for the product mass model.
pub fn operating_items(
    passenger_count: u32,
    accessories_type: AccessoriesType,
) -> OperationalItems {
    let num_seats = f64::from(passenger_count);

    let operating_items_less_crew_kg = match accessories_type {
        AccessoriesType::ShortRange | AccessoriesType::Commuter => 17.0 * num_seats * POUND_MASS,
        AccessoriesType::MediumRange
        | AccessoriesType::LongRange
        | AccessoriesType::Business
        | AccessoriesType::Other => 28.0 * num_seats * POUND_MASS,
        // The one branch that does not scale with the seat count.
        AccessoriesType::Cargo => 56.0 * POUND_MASS,
        AccessoriesType::Sst => 40.0 * num_seats * POUND_MASS,
    };

    let flight_crew = if passenger_count >= 150 { 3.0 } else { 2.0 };
    let flight_attendants = if passenger_count < 51 {
        1.0
    } else {
        1.0 + (num_seats / 40.0).floor()
    };

    // The crew and attendant unit weights are in pounds (body plus baggage
    // allowance) and converted to kg once, matching upstream's
    // `* Units.lbs`.
    let flight_attendants_kg = flight_attendants * (170.0 + 40.0) * POUND_MASS;
    let flight_crew_kg = flight_crew * (190.0 + 50.0) * POUND_MASS;

    OperationalItems {
        operating_items_less_crew_kg,
        flight_crew_kg,
        flight_attendants_kg,
        total_kg: operating_items_less_crew_kg + flight_crew_kg + flight_attendants_kg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_accessories_override_the_electrical_group() {
        // Cargo aircraft carry a fixed 1950 lb electrical group rather than
        // the per-seat scaling every other category uses.
        let cargo = systems(
            10,
            ControlSystemType::FullyPowered,
            AccessoriesType::Cargo,
            300.0,
            60.0,
            500.0,
        );
        assert!((cargo.electrical_kg - 1950.0 * POUND_MASS).abs() < 1e-9);
    }

    #[test]
    fn long_range_adds_seat_furnishing_over_the_fallback() {
        let long = systems(
            300,
            ControlSystemType::FullyPowered,
            AccessoriesType::LongRange,
            500.0,
            80.0,
            500.0,
        );
        let fallback = systems(
            300,
            ControlSystemType::FullyPowered,
            AccessoriesType::Other,
            500.0,
            80.0,
            500.0,
        );
        assert!(long.furnish_kg > fallback.furnish_kg);
    }

    #[test]
    fn a_missing_tail_falls_back_to_one_percent_of_the_wing() {
        let with_tail = systems(
            100,
            ControlSystemType::FullyPowered,
            AccessoriesType::Other,
            400.0,
            40.0,
            400.0,
        );
        let no_tail = systems(
            100,
            ControlSystemType::FullyPowered,
            AccessoriesType::Other,
            400.0,
            0.0,
            400.0,
        );
        // 1% of 400 m^2 is 4 m^2, well below a real 40 m^2 tail, so the
        // control-system weight drops.
        assert!(no_tail.control_systems_kg < with_tail.control_systems_kg);
    }

    #[test]
    fn the_apu_floor_binds_for_a_small_cabin() {
        let tiny = systems(
            4,
            ControlSystemType::FullyPowered,
            AccessoriesType::Other,
            100.0,
            10.0,
            100.0,
        );
        assert_eq!(tiny.apu_kg, 70.0);
    }

    #[test]
    fn cargo_operating_items_do_not_scale_with_seats() {
        let few = operating_items(10, AccessoriesType::Cargo);
        let many = operating_items(200, AccessoriesType::Cargo);
        assert_eq!(
            few.operating_items_less_crew_kg,
            many.operating_items_less_crew_kg
        );
    }

    #[test]
    fn attendant_count_steps_with_the_cabin() {
        // 1 attendant below 51 seats, then 1 + floor(pax/40).
        let small = operating_items(50, AccessoriesType::Other);
        let large = operating_items(200, AccessoriesType::Other);
        assert!((small.flight_attendants_kg - 1.0 * 210.0 * POUND_MASS).abs() < 1e-9);
        assert!((large.flight_attendants_kg - 6.0 * 210.0 * POUND_MASS).abs() < 1e-9);
    }
}
