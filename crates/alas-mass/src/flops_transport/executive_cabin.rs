// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Furnishings and occupant operating items of an executive (business) jet.
//!
//! Selected by [`alas_config::CabinEquipmentMethod::ExecutiveJetV1`], which a
//! preset declaring the business-jet class takes. Source: E. Torenbeek,
//! *Synthesis of Subsonic Airplane Design* (1982), metric coefficients as
//! printed in brackets (kg, m^2, m^3):
//!
//! * Table 8-12, p. 291, "Furnishing and equipment group weight for transport
//!   and executive aircraft": flight-deck accommodations `16.5 W_DE^0.285`
//!   (jet) or `9.1 W_DE^0.285` (propeller), `W_DE` the delivery empty weight;
//!   passenger and attendants' seats from Table 3-2; galley structure and
//!   provisions 113.4 kg per main-meal galley, 45.3 kg per snack pantry and
//!   29.5 kg per coffee bar; lavatory and toilet provisions with the dry water
//!   system 136.0 kg per toilet medium/long haul, 75.0 kg short haul; floor
//!   covering `1.25 S_cf^1.15` (jet) or `0.94 S_cf^1.15`; soundproofing,
//!   insulation, linings, hatracks, partitions, wardrobes and hold linings
//!   `6.17 (V_pc + V_ch)^1.07` (jet) or `3.69 (V_pc + V_ch)^1.14`; cargo
//!   restraints `1.28 V_ch`; fixed and portable oxygen `9.1 + 0.227 N_pax` up
//!   to 7,620 m cruise altitude, `13.6 + 0.544 N_pax` above it and
//!   `18.1 + 1.09 N_pax` for extended overwater flights; fire detection and
//!   extinguishing `0.0012 W_to` (jet) or `0.0030 W_to` (turboprop); escape
//!   provisions 0.453 kg per occupant.
//! * Table 3-2, p. 76: executive seat, single, normal 18.1 kg (the same table
//!   gives VIP 22.7 kg and small aircraft 14.5 kg); attendants' seat 8.2 kg
//!   medium/long haul, 6.4 kg short haul.
//! * Table 8-13, p. 292, operational items: passenger cabin supplies 2.27 kg
//!   per passenger with snacks only, 6.35 / 8.62 kg with a main meal short /
//!   long range, "First class: all data 5 lb (2.27 kg) per passenger higher";
//!   potable water and toilet chemicals 54.4 kg per toilet short/medium range,
//!   90.7 kg long range; safety equipment 0.907 kg per passenger on short or
//!   no overwater sectors, 3.4 kg extended overwater.
//!
//! The same page prints `0.196 W_ZF^0.91` kg (eq. 8-44) as the one-term
//! approximation for "passenger transports and jet executives"; on the
//! Learjet 60 (7,711 kg MZFW) it gives 675 kg against 620 kg itemised here.
//!
//! # Scope against FLOPS
//!
//! Table 8-12 is the whole furnishing-and-equipment group, the scope of FLOPS
//! `WFURN` (NASA/TM-2017-219627 Vol. I eq. 110) including the flight-crew
//! seats, so it replaces equation 110 one to one. The three Table 8-13 cabin
//! items replace `WSRV` (eq. 124). Crew and baggage (eqs. 119-120), unusable
//! fuel (eq. 121) and engine oil (eq. 123) do not depend on the cabin and stay
//! FLOPS: Table 8-13 prints transport-wide rows for them too, and pricing the
//! same item by two methods according to class would be a second accounting
//! system. Nothing is counted twice and nothing is dropped.
//!
//! # Readings (this module's, stated)
//!
//! * Every passenger seat is Table 3-2's executive single, normal.
//! * An executive passenger is served to the first-class standard, so the
//!   Table 8-13 first-class increment applies to every seat.
//! * The declared haul class picks each table's row: long haul takes the
//!   medium/long-haul toilet and the long-range water, short/medium haul the
//!   short-haul toilet and the short/medium-range water.
//! * `N_pax` of the oxygen row is, as Table 8-12 defines it, the maximum
//!   number of passengers for certification, read from the declaration; the
//!   seats and the escape provisions read the installed occupants.
//! * `S_cf` and `V_pc` are published for one cabin length. With a constant
//!   cabin section both are proportional to it, so each is the declared value
//!   times the built passenger-compartment length over the declared one: a
//!   design or optimisation run that stretches the fuselage stretches them, a
//!   baseline built at the published length reads them unchanged. `V_ch` is
//!   the declared hold volume. A baggage compartment declared inside the
//!   cabin volume is taken out of `V_pc` so `V_pc + V_ch` counts it once.

use alas_config::{ExecutiveCabinDeclaration, OperatingHaulClass};

use super::{FlopsTransportInputError, FlopsTransportInputs, PropulsionSizing};

/// Table 3-2 executive seat, single, normal, kg.
pub const EXECUTIVE_SEAT_KG: f64 = 18.1;
/// Cruise altitude above which Table 8-12 prices the high-altitude oxygen
/// system, m (25,000 ft).
const HIGH_ALTITUDE_OXYGEN_M: f64 = 7_620.0;
/// Table 8-13 first-class increment of the passenger cabin supplies, kg.
const FIRST_CLASS_SUPPLIES_KG: f64 = 2.27;

/// The executive interior and the delivery empty mass it is priced at.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct ExecutiveCabinInputs {
    /// The declared interior.
    pub declaration: ExecutiveCabinDeclaration,
    /// Torenbeek's delivery empty weight `W_DE`, kg, read by the flight-deck
    /// term. `None` is the first iterate of the product buildup's fixed point
    /// and evaluates that term at the design gross mass; the buildup then
    /// supplies its own empty mass until the value no longer moves
    /// (`d term / d W_DE` is about 0.009, so it converges in two passes).
    pub delivery_empty_mass_kg: Option<f64>,
}

impl ExecutiveCabinInputs {
    /// The declared interior at the first fixed-point iterate.
    pub fn declared(declaration: ExecutiveCabinDeclaration) -> Self {
        Self {
            declaration,
            delivery_empty_mass_kg: None,
        }
    }
}

/// The declared interior on the built cabin: `S_cf` and `V_pc` at the built
/// passenger-compartment length, `V_pc` net of an in-cabin baggage volume.
struct Resolved {
    declared: ExecutiveCabinDeclaration,
    floor_area_m2: f64,
    cabin_volume_m3: f64,
    delivery_empty_kg: f64,
}

fn resolved(inputs: &FlopsTransportInputs) -> Result<Resolved, FlopsTransportInputError> {
    let refused = FlopsTransportInputError {
        field: "executive_cabin",
    };
    let cabin = inputs.executive_cabin.ok_or(refused)?;
    let declared = cabin.declaration;
    let delivery_empty_kg = cabin
        .delivery_empty_mass_kg
        .unwrap_or(inputs.design_gross_mass_kg);
    let in_cabin_baggage_m3 = declared.in_cabin_baggage_volume_m3.unwrap_or(0.0);
    let dimensions = [
        declared.cabin_floor_area_m2,
        declared.passenger_cabin_volume_m3,
        declared.cargo_hold_volume_m3,
        declared.maximum_operating_altitude_m,
        in_cabin_baggage_m3,
    ];
    let lengths = [
        declared.passenger_compartment_length_m,
        inputs.passenger_compartment_length_m,
        delivery_empty_kg,
    ];
    if !(dimensions.iter().all(|value| value.is_finite() && *value >= 0.0)
        && lengths.iter().all(|value| value.is_finite() && *value > 0.0)
        && in_cabin_baggage_m3 <= declared.cargo_hold_volume_m3
        && in_cabin_baggage_m3 <= declared.passenger_cabin_volume_m3)
    {
        return Err(refused);
    }
    let stretch = inputs.passenger_compartment_length_m / declared.passenger_compartment_length_m;
    Ok(Resolved {
        declared,
        floor_area_m2: declared.cabin_floor_area_m2 * stretch,
        cabin_volume_m3: (declared.passenger_cabin_volume_m3 - in_cabin_baggage_m3) * stretch,
        delivery_empty_kg,
    })
}

/// Table 8-12 furnishing and equipment group, kg: replaces FLOPS eq. 110.
///
/// # Errors
///
/// [`FlopsTransportInputError`] (`executive_cabin`) when no interior is
/// declared, a declared dimension is negative or not finite, a declared or
/// built cabin length or the delivery empty mass is not positive, or the
/// in-cabin baggage volume exceeds the hold or cabin volume.
pub(super) fn furnishings_kg(
    inputs: &FlopsTransportInputs,
) -> Result<f64, FlopsTransportInputError> {
    let Resolved {
        declared: cabin,
        floor_area_m2,
        cabin_volume_m3,
        delivery_empty_kg,
    } = resolved(inputs)?;
    let jet = matches!(inputs.propulsion_sizing, PropulsionSizing::RatedThrust);
    let long_haul = inputs.haul_class == OperatingHaulClass::LongHaul;
    let passengers = inputs.passenger_count() as f64;
    let attendants = inputs.flight_attendant_count as f64;
    let occupants = passengers
        + attendants
        + inputs.galley_crew_count as f64
        + inputs.flight_crew_count as f64;
    let volume_m3 = cabin_volume_m3 + cabin.cargo_hold_volume_m3;
    let (flight_deck, floor_covering, linings) = if jet {
        (
            16.5 * delivery_empty_kg.powf(0.285),
            1.25 * floor_area_m2.powf(1.15),
            6.17 * volume_m3.powf(1.07),
        )
    } else {
        (
            9.1 * delivery_empty_kg.powf(0.285),
            0.94 * floor_area_m2.powf(1.15),
            3.69 * volume_m3.powf(1.14),
        )
    };
    let attendant_seat_kg = if long_haul { 8.2 } else { 6.4 };
    let seats = EXECUTIVE_SEAT_KG * passengers + attendant_seat_kg * attendants;
    let galleys = 113.4 * f64::from(cabin.main_meal_galleys)
        + 45.3 * f64::from(cabin.snack_pantries)
        + 29.5 * f64::from(cabin.coffee_bars);
    let toilet_kg = if long_haul { 136.0 } else { 75.0 };
    let toilets = toilet_kg * f64::from(cabin.lavatories);
    let cargo_restraints = 1.28 * cabin.cargo_hold_volume_m3;
    let certified = f64::from(cabin.certified_passenger_count);
    let oxygen = if cabin.extended_overwater {
        18.1 + 1.09 * certified
    } else if cabin.maximum_operating_altitude_m > HIGH_ALTITUDE_OXYGEN_M {
        13.6 + 0.544 * certified
    } else {
        9.1 + 0.227 * certified
    };
    let fire_fraction = if jet { 0.0012 } else { 0.0030 };
    let fire = fire_fraction * inputs.design_gross_mass_kg;
    let escape = 0.453 * occupants;
    Ok(flight_deck
        + seats
        + galleys
        + toilets
        + floor_covering
        + linings
        + cargo_restraints
        + oxygen
        + fire
        + escape)
}

/// Table 8-13 passenger cabin supplies, potable water and toilet chemicals
/// and safety equipment, kg: replaces FLOPS eq. 124.
///
/// # Errors
///
/// As [`furnishings_kg`].
pub(super) fn passenger_service_kg(
    inputs: &FlopsTransportInputs,
) -> Result<f64, FlopsTransportInputError> {
    let cabin = resolved(inputs)?.declared;
    let long_haul = inputs.haul_class == OperatingHaulClass::LongHaul;
    let passengers = inputs.passenger_count() as f64;
    let meal_kg = match (cabin.main_meal_galleys > 0, long_haul) {
        (true, true) => 8.62,
        (true, false) => 6.35,
        (false, _) => 2.27,
    };
    let supplies = (meal_kg + FIRST_CLASS_SUPPLIES_KG) * passengers;
    let water_per_toilet_kg = if long_haul { 90.7 } else { 54.4 };
    let water = water_per_toilet_kg * f64::from(cabin.lavatories);
    let safety_per_passenger_kg = if cabin.extended_overwater { 3.4 } else { 0.907 };
    Ok(supplies + water + safety_per_passenger_kg * passengers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flops_transport::estimate_flops_transport;

    /// The Learjet 60 as the baseline design database resolves it (design database
    /// `weights.flops_mass_buildup.inputs`), with the declared interior of
    /// `alas-config` `preset_flops::architecture_rows::declared_executive_cabin`.
    fn learjet_60(method: alas_config::CabinEquipmentMethod) -> FlopsTransportInputs {
        FlopsTransportInputs {
            maximum_mach: 0.81,
            design_range_nmi: 2_493.0,
            design_gross_mass_kg: 10_659.0,
            wing_area_m2: 24.57,
            movable_surface_area_m2: 6.381_337_781_674_836,
            wing_span_m: 12.549_558_380_293_467,
            quarter_chord_sweep_deg: 12.931_779_160_762_456,
            fuselage_length_m: 17.12,
            fuselage_width_m: 1.988_864_847_065_637_6,
            fuselage_depth_m: 1.896_145_739_247_48,
            fuselage_count: 1,
            passenger_compartment_length_m: 5.38,
            first_class_passenger_count: 0,
            business_class_passenger_count: 0,
            tourist_class_passenger_count: 8,
            systems_passenger_count: None,
            flight_crew_count: 2,
            flight_attendant_count: 0,
            galley_crew_count: 0,
            wing_mounted_engine_count: 0,
            fuselage_mounted_engine_count: 2,
            engine_count: 2,
            rated_thrust_per_engine_n: 20_460.0,
            nacelle_diameter_m: 1.06,
            hydraulic_pressure_pa: 20_684_271.879_504,
            variable_sweep_penalty: 0.0,
            maximum_fuel_capacity_kg: 3_588.0,
            apu_installed: false,
            bleed_air_system: true,
            fuel_tank_count: 3,
            containerized_cargo_kg: 0.0,
            containerized_baggage_kg: 0.0,
            cargo_loading: alas_config::CargoHoldLoading::Bulk,
            cabin_equipment_method: method,
            haul_class: OperatingHaulClass::ShortMediumHaul,
            propulsion_sizing: PropulsionSizing::RatedThrust,
            military_cargo: None,
            all_coach_passenger_count: None,
            executive_cabin: Some(ExecutiveCabinInputs {
                declaration: ExecutiveCabinDeclaration {
                    cabin_floor_area_m2: 6.40,
                    passenger_cabin_volume_m3: 12.8,
                    passenger_compartment_length_m: 5.38,
                    cargo_hold_volume_m3: 1.67,
                    in_cabin_baggage_volume_m3: None,
                    certified_passenger_count: 8,
                    main_meal_galleys: 0,
                    snack_pantries: 1,
                    coffee_bars: 0,
                    lavatories: 1,
                    maximum_operating_altitude_m: 51_000.0 * 0.3048,
                    extended_overwater: false,
                },
                delivery_empty_mass_kg: Some(6_209.32),
            }),
        }
    }

    /// Hand calculation, kg: flight deck 16.5 x 6,209.32^0.285 = 198.839;
    /// seats 8 x 18.1 = 144.8; snack pantry 45.3; short-haul toilet 75.0;
    /// floor 1.25 x 6.40^1.15 = 10.569; linings 6.17 x 14.47^1.07 = 107.643;
    /// restraints 1.28 x 1.67 = 2.138; oxygen 13.6 + 0.544 x 8 = 17.952; fire
    /// 0.0012 x 10,659 = 12.791; escape 0.453 x 10 = 4.530; total 619.561.
    /// Service: (2.27 + 2.27) x 8 + 54.4 + 0.907 x 8 = 97.976.
    #[test]
    fn reproduces_the_hand_calculated_learjet_60_cabin() {
        let inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        let furnishings = furnishings_kg(&inputs).expect("declared interior");
        assert!((furnishings - 619.561_127_755).abs() < 1e-6, "{furnishings}");
        let service = passenger_service_kg(&inputs).expect("declared interior");
        assert!((service - 97.976).abs() < 1e-9, "{service}");

        let executive = estimate_flops_transport(&inputs).expect("executive evaluates");
        assert!((executive.systems.furnishings_kg - furnishings).abs() < 1e-9);
        assert!((executive.operating_items.passenger_service_kg - service).abs() < 1e-9);
        assert!((executive.seats.passenger_kg.iter().sum::<f64>() - 144.8).abs() < 1e-9);
        assert_eq!(executive.seats.flight_deck_kg, 0.0);

        // Only eqs. 110 and 124 are replaced: every other system and the crew,
        // unusable fuel and oil are the FLOPS values of the same inputs.
        let flops = estimate_flops_transport(&learjet_60(
            alas_config::CabinEquipmentMethod::FlopsTransportV1,
        ))
        .expect("FLOPS evaluates");
        assert!((flops.systems.furnishings_kg - 540.205).abs() < 1e-3);
        let others = |b: &crate::flops_transport::FlopsTransportBreakdown| {
            b.systems.total_kg - b.systems.furnishings_kg
        };
        assert!((others(&executive) - others(&flops)).abs() < 1e-9);
        let items = |b: &crate::flops_transport::FlopsTransportBreakdown| {
            b.operating_items.total_kg - b.operating_items.passenger_service_kg
        };
        assert!((items(&executive) - items(&flops)).abs() < 1e-9);
    }

    #[test]
    fn an_undeclared_interior_is_refused_not_priced() {
        let mut inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        inputs.executive_cabin = None;
        assert_eq!(
            estimate_flops_transport(&inputs).map(|_| ()),
            Err(FlopsTransportInputError {
                field: "executive_cabin"
            })
        );
    }

    #[test]
    fn the_first_iterate_reads_the_design_gross_mass() {
        let mut inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        let converged = furnishings_kg(&inputs).expect("declared");
        if let Some(cabin) = inputs.executive_cabin.as_mut() {
            cabin.delivery_empty_mass_kg = None;
        }
        let first = furnishings_kg(&inputs).expect("declared");
        let expected = 16.5 * (10_659.0_f64.powf(0.285) - 6_209.32_f64.powf(0.285));
        assert!((first - converged - expected).abs() < 1e-9);
    }

    /// A built cabin 1 m longer than the published 5.38 m scales `S_cf` and
    /// `V_pc` by 6.38 / 5.38 and moves nothing but the floor and linings.
    #[test]
    fn floor_area_and_cabin_volume_follow_the_built_cabin_length() {
        let mut inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        let published = furnishings_kg(&inputs).expect("declared");
        inputs.passenger_compartment_length_m = 6.38;
        let stretched = furnishings_kg(&inputs).expect("declared");
        let stretch: f64 = 6.38 / 5.38;
        let expected = 1.25 * ((6.40 * stretch).powf(1.15) - 6.40_f64.powf(1.15))
            + 6.17 * ((12.8 * stretch + 1.67).powf(1.07) - 14.47_f64.powf(1.07));
        assert!((stretched - published - expected).abs() < 1e-9);
        assert!((expected - 21.330_674).abs() < 1e-5, "{expected}");
        inputs.passenger_compartment_length_m = 0.0;
        assert!(furnishings_kg(&inputs).is_err());
    }

    /// The oxygen row reads the certified maximum, not the installed seats;
    /// an in-cabin baggage volume leaves `V_pc + V_ch` and is counted once.
    #[test]
    fn oxygen_reads_the_certified_maximum_and_in_cabin_baggage_counts_once() {
        let mut inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        let base = furnishings_kg(&inputs).expect("declared");
        let Some(cabin) = inputs.executive_cabin.as_mut() else {
            panic!("declared interior")
        };
        cabin.declaration.certified_passenger_count = 10;
        let certified_ten = furnishings_kg(&inputs).expect("declared");
        assert!((certified_ten - base - 0.544 * 2.0).abs() < 1e-9);
        assert_eq!(inputs.passenger_count(), 8);

        let mut inputs = learjet_60(alas_config::CabinEquipmentMethod::ExecutiveJetV1);
        let Some(cabin) = inputs.executive_cabin.as_mut() else {
            panic!("declared interior")
        };
        cabin.declaration.in_cabin_baggage_volume_m3 = Some(1.67);
        let netted = furnishings_kg(&inputs).expect("declared");
        let expected = 6.17 * (12.8_f64.powf(1.07) - 14.47_f64.powf(1.07));
        assert!((netted - base - expected).abs() < 1e-9);
        if let Some(cabin) = inputs.executive_cabin.as_mut() {
            cabin.declaration.in_cabin_baggage_volume_m3 = Some(1.68);
        }
        assert!(furnishings_kg(&inputs).is_err());
    }
}
