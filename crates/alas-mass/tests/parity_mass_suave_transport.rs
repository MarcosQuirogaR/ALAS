// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the transport-weight translation against the reference
//! `Weights_Transport` implementation. This unpublished parity test retains
//! the historical fixture and test filename for reproducibility.
//!
//! Every quantity here is closed-form `f64` arithmetic: imperial unit
//! conversions and empirical correlations, no factorization, spline fit or
//! iteration, so the whole row is checked at `Tier::Closed`, the tier
//! `docs/PORTING.md` assigns it.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_mass::transport_weight::{
    empty_weight, AccessoriesType, ControlSystemType, Fuselage, HorizontalTail, MainWing,
    TransportPropulsionMassBasis, TransportVehicle, VerticalTail, WeightBreakdown,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MainWingInput {
    span_m: f64,
    sweep_quarter_chord_rad: f64,
    area_m2: f64,
    thickness_to_chord: f64,
    taper_ratio: f64,
    root_chord_m: f64,
    mean_aerodynamic_chord_m: f64,
    origin_x_m: f64,
}

#[derive(Debug, Deserialize)]
struct TailInput {
    span_m: f64,
    sweep_quarter_chord_rad: f64,
    area_m2: f64,
    area_exposed_m2: f64,
    area_wetted_m2: f64,
    thickness_to_chord: f64,
    origin_x_m: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageInput {
    differential_pressure_pa: f64,
    width_m: f64,
    height_max_m: f64,
    length_total_m: f64,
    area_wetted_m2: f64,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    mtow_kg: f64,
    max_zero_fuel_kg: f64,
    cargo_kg: f64,
    passenger_count: f64,
    reference_area_m2: f64,
    ultimate_load_factor: f64,
    limit_load_factor: f64,
    control_type: String,
    accessories_type: String,
    engine_count: f64,
    sealevel_static_thrust_per_engine_n: f64,
    main_wing: MainWingInput,
    horizontal_tail: TailInput,
    vertical_tail: TailInput,
    fuselage: FuselageInput,
}

#[derive(Debug, Deserialize)]
struct Structures {
    wing: f64,
    horizontal_tail: f64,
    vertical_tail: f64,
    fuselage: f64,
    main_landing_gear: f64,
    nose_landing_gear: f64,
    nacelle: f64,
    paint: f64,
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Propulsion {
    engines: f64,
    thrust_reversers: f64,
    miscellaneous: f64,
    fuel_system: f64,
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Systems {
    control_systems: f64,
    apu: f64,
    electrical: f64,
    avionics: f64,
    hydraulics: f64,
    furnish: f64,
    air_conditioner: f64,
    instruments: f64,
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Payload {
    passengers: f64,
    baggage: f64,
    cargo: f64,
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Operational {
    operating_items_less_crew: f64,
    flight_crew: f64,
    flight_attendants: f64,
    total: f64,
}

#[derive(Debug, Deserialize)]
struct Breakdown {
    structures: Structures,
    propulsion_breakdown: Propulsion,
    systems_breakdown: Systems,
    payload_breakdown: Payload,
    operational_items: Operational,
    empty: f64,
    operating_empty: f64,
    zero_fuel_weight: f64,
    fuel: f64,
    max_takeoff: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    inputs: Inputs,
    weight_breakdown: Breakdown,
    vehicle_mass_properties_operating_empty: f64,
}

/// Resolve SUAVE's `vehicle.systems.control` string the same way
/// `systems.systems` does: a test-only mapping, since the crate's public
/// API rightly takes the enum and the production string resolution belongs to
/// `alas-mission::vehicle` (a later crate). `"long range"` (space) and the
/// other unhyphenated forms this program actually produces all resolve to the
/// `Other` fallback; see the module doc.
fn control_type(raw: &str) -> ControlSystemType {
    match raw {
        "fully powered" => ControlSystemType::FullyPowered,
        "partially powered" => ControlSystemType::PartiallyPowered,
        _ => ControlSystemType::Other,
    }
}

fn accessories_type(raw: &str) -> AccessoriesType {
    match raw {
        "short-range" => AccessoriesType::ShortRange,
        "medium-range" => AccessoriesType::MediumRange,
        "long-range" => AccessoriesType::LongRange,
        "business" => AccessoriesType::Business,
        "cargo" => AccessoriesType::Cargo,
        "commuter" => AccessoriesType::Commuter,
        "sst" => AccessoriesType::Sst,
        _ => AccessoriesType::Other,
    }
}

fn build_vehicle(inputs: &Inputs) -> TransportVehicle {
    TransportVehicle {
        mtow_kg: inputs.mtow_kg,
        max_zero_fuel_kg: inputs.max_zero_fuel_kg,
        cargo_kg: inputs.cargo_kg,
        passenger_count: inputs.passenger_count as u32,
        reference_area_m2: inputs.reference_area_m2,
        ultimate_load_factor: inputs.ultimate_load_factor,
        limit_load_factor: inputs.limit_load_factor,
        control_type: control_type(&inputs.control_type),
        accessories_type: accessories_type(&inputs.accessories_type),
        engine_count: inputs.engine_count as u32,
        sealevel_static_thrust_per_engine_n: inputs.sealevel_static_thrust_per_engine_n,
        propulsion_mass_basis: TransportPropulsionMassBasis::TurbofanThrust,
        main_wing: MainWing {
            span_m: inputs.main_wing.span_m,
            sweep_quarter_chord_rad: inputs.main_wing.sweep_quarter_chord_rad,
            area_m2: inputs.main_wing.area_m2,
            thickness_to_chord: inputs.main_wing.thickness_to_chord,
            taper_ratio: inputs.main_wing.taper_ratio,
            root_chord_m: inputs.main_wing.root_chord_m,
            mean_aerodynamic_chord_m: inputs.main_wing.mean_aerodynamic_chord_m,
            origin_x_m: inputs.main_wing.origin_x_m,
        },
        horizontal_tail: HorizontalTail {
            span_m: inputs.horizontal_tail.span_m,
            sweep_quarter_chord_rad: inputs.horizontal_tail.sweep_quarter_chord_rad,
            area_m2: inputs.horizontal_tail.area_m2,
            area_exposed_m2: inputs.horizontal_tail.area_exposed_m2,
            area_wetted_m2: inputs.horizontal_tail.area_wetted_m2,
            thickness_to_chord: inputs.horizontal_tail.thickness_to_chord,
            origin_x_m: inputs.horizontal_tail.origin_x_m,
        },
        vertical_tail: VerticalTail {
            span_m: inputs.vertical_tail.span_m,
            sweep_quarter_chord_rad: inputs.vertical_tail.sweep_quarter_chord_rad,
            area_m2: inputs.vertical_tail.area_m2,
            thickness_to_chord: inputs.vertical_tail.thickness_to_chord,
        },
        fuselage: Fuselage {
            differential_pressure_pa: inputs.fuselage.differential_pressure_pa,
            width_m: inputs.fuselage.width_m,
            height_max_m: inputs.fuselage.height_max_m,
            length_total_m: inputs.fuselage.length_total_m,
            area_wetted_m2: inputs.fuselage.area_wetted_m2,
        },
    }
}

fn compare(c: &mut Comparison, breakdown: &WeightBreakdown, expected: &Breakdown) {
    let s = &breakdown.structures;
    c.scalar("structures.wing", s.wing_kg, expected.structures.wing);
    c.scalar(
        "structures.horizontal_tail",
        s.horizontal_tail_kg,
        expected.structures.horizontal_tail,
    );
    c.scalar(
        "structures.vertical_tail",
        s.vertical_tail_kg,
        expected.structures.vertical_tail,
    );
    c.scalar(
        "structures.fuselage",
        s.fuselage_kg,
        expected.structures.fuselage,
    );
    c.scalar(
        "structures.main_landing_gear",
        s.main_landing_gear_kg,
        expected.structures.main_landing_gear,
    );
    c.scalar(
        "structures.nose_landing_gear",
        s.nose_landing_gear_kg,
        expected.structures.nose_landing_gear,
    );
    c.scalar(
        "structures.nacelle",
        s.nacelle_kg,
        expected.structures.nacelle,
    );
    c.scalar("structures.paint", s.paint_kg, expected.structures.paint);
    c.scalar("structures.total", s.total_kg, expected.structures.total);

    let p = &breakdown.propulsion;
    c.scalar(
        "propulsion.engines",
        p.engines_kg,
        expected.propulsion_breakdown.engines,
    );
    c.scalar(
        "propulsion.thrust_reversers",
        p.thrust_reversers_kg,
        expected.propulsion_breakdown.thrust_reversers,
    );
    c.scalar(
        "propulsion.miscellaneous",
        p.miscellaneous_kg,
        expected.propulsion_breakdown.miscellaneous,
    );
    c.scalar(
        "propulsion.fuel_system",
        p.fuel_system_kg,
        expected.propulsion_breakdown.fuel_system,
    );
    c.scalar(
        "propulsion.total",
        p.total_kg,
        expected.propulsion_breakdown.total,
    );

    let sys = &breakdown.systems;
    c.scalar(
        "systems.control_systems",
        sys.control_systems_kg,
        expected.systems_breakdown.control_systems,
    );
    c.scalar("systems.apu", sys.apu_kg, expected.systems_breakdown.apu);
    c.scalar(
        "systems.electrical",
        sys.electrical_kg,
        expected.systems_breakdown.electrical,
    );
    c.scalar(
        "systems.avionics",
        sys.avionics_kg,
        expected.systems_breakdown.avionics,
    );
    c.scalar(
        "systems.hydraulics",
        sys.hydraulics_kg,
        expected.systems_breakdown.hydraulics,
    );
    c.scalar(
        "systems.furnish",
        sys.furnish_kg,
        expected.systems_breakdown.furnish,
    );
    c.scalar(
        "systems.air_conditioner",
        sys.air_conditioner_kg,
        expected.systems_breakdown.air_conditioner,
    );
    c.scalar(
        "systems.instruments",
        sys.instruments_kg,
        expected.systems_breakdown.instruments,
    );
    c.scalar(
        "systems.total",
        sys.total_kg,
        expected.systems_breakdown.total,
    );

    let pay = &breakdown.payload;
    c.scalar(
        "payload.passengers",
        pay.passengers_kg,
        expected.payload_breakdown.passengers,
    );
    c.scalar(
        "payload.baggage",
        pay.baggage_kg,
        expected.payload_breakdown.baggage,
    );
    c.scalar(
        "payload.cargo",
        pay.cargo_kg,
        expected.payload_breakdown.cargo,
    );
    c.scalar(
        "payload.total",
        pay.total_kg,
        expected.payload_breakdown.total,
    );

    let op = &breakdown.operational_items;
    c.scalar(
        "operational.operating_items_less_crew",
        op.operating_items_less_crew_kg,
        expected.operational_items.operating_items_less_crew,
    );
    c.scalar(
        "operational.flight_crew",
        op.flight_crew_kg,
        expected.operational_items.flight_crew,
    );
    c.scalar(
        "operational.flight_attendants",
        op.flight_attendants_kg,
        expected.operational_items.flight_attendants,
    );
    c.scalar(
        "operational.total",
        op.total_kg,
        expected.operational_items.total,
    );

    c.scalar("empty", breakdown.empty_kg, expected.empty);
    c.scalar(
        "operating_empty",
        breakdown.operating_empty_kg,
        expected.operating_empty,
    );
    c.scalar(
        "zero_fuel_weight",
        breakdown.zero_fuel_weight_kg,
        expected.zero_fuel_weight,
    );
    c.scalar("fuel", breakdown.fuel_kg, expected.fuel);
    c.scalar(
        "max_takeoff",
        breakdown.max_takeoff_kg,
        expected.max_takeoff,
    );
}

#[test]
fn weight_transport_matches_suave() {
    let fixture: Fixture = alas_testkit::load("mass", "suave_transport");
    let vehicle = build_vehicle(&fixture.inputs);
    let breakdown = empty_weight(&vehicle);

    let mut c = Comparison::new("alas-mass::transport_weight::empty_weight", Tier::Closed);
    compare(&mut c, &breakdown, &fixture.weight_breakdown);
    c.finish();
}

#[test]
fn operating_empty_reproduces_the_evaluate_naming_quirk() {
    // `Weights_Transport.evaluate()` sets
    // `vehicle.mass_properties.operating_empty = results.empty`, not
    // `results.operating_empty`. The fixture records the vehicle attribute;
    // it must equal our `empty_kg`, not `operating_empty_kg`.
    let fixture: Fixture = alas_testkit::load("mass", "suave_transport");
    let vehicle = build_vehicle(&fixture.inputs);
    let breakdown = empty_weight(&vehicle);

    let mut c = Comparison::new(
        "alas-mass::transport_weight::operating_empty_quirk",
        Tier::Closed,
    );
    c.scalar(
        "vehicle.mass_properties.operating_empty == results.empty",
        breakdown.empty_kg,
        fixture.vehicle_mass_properties_operating_empty,
    );
    c.finish();
}
