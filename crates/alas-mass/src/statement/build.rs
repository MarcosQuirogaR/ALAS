// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ledger construction: turns a [`MassBreakdown`] plus
//! [`ComponentStations`] plus loadable items into the [`MassItem`] rows
//! [`super::MassStatement::build`] validates.
//!
//! [`build_ledger`] is the one entry point [`super`] calls; every `push_*`
//! function below appends one physical family of items to the ledger it is
//! handed, at the station and with the tensor the module doc's placement
//! contract names. Splitting this out of `statement.rs` keeps that file to
//! the public state-query API; this file is the physical detail of how each
//! component becomes one or more ledger rows.

use crate::breakdown::MassBreakdown;
use crate::flops_transport::{
    FlopsOperatingItemsBreakdown, FlopsSystemsBreakdown, FlopsTransportBreakdown,
};
use crate::inertia::{
    rectangular_prism, solid_cylinder_x, thin_cylinder_shell_x, thin_plate_xy, thin_plate_xz,
};
use crate::ledger::{InertiaTensor, MassGroup, MassItem, MassLedger, MassMethod, MassRole};
use crate::stations::ComponentStations;

use super::PayloadItemSummary;

/// Fraction of [`MassBreakdown::gear`] carried by the nose gear at static
/// weight, the rest going to the main gear -- Raymer, *Aircraft Design: A
/// Conceptual Approach*, 6th ed., ch. 11, typical nose/main static load
/// split (roughly 8-15% nose for a tricycle transport). This is a mass
/// split assumption, not a measured load: [`crate::stations`]'s gear
/// stations already carry the geometric x/z placement this module reuses.
const NOSE_GEAR_MASS_FRACTION: f64 = 0.15;

/// Build the complete, unvalidated ledger for one mass statement.
///
/// Validation is [`super::MassStatement::build`]'s job, once every item
/// (including caller-supplied payload and unusable-fuel items) has been
/// appended.
pub(super) fn build_ledger(
    masses: &MassBreakdown,
    stations: &ComponentStations,
    payload_items: &[PayloadItemSummary],
    unusable_fuel_items: Vec<MassItem>,
    flops: Option<&FlopsTransportBreakdown>,
) -> MassLedger {
    let mut ledger = MassLedger::new();
    push_structure(&mut ledger, masses, stations);
    push_gear(&mut ledger, masses, stations);
    push_propulsion(&mut ledger, masses, stations);
    match flops {
        Some(flops) => push_flops_systems_and_operating_items(&mut ledger, masses, stations, flops),
        None => push_lumped_systems_and_furnishings(&mut ledger, masses, stations),
    }
    for item in unusable_fuel_items {
        ledger.push(item);
    }
    push_payload(&mut ledger, masses, stations, payload_items);
    ledger
}

/// The nacelle mid-length points [`crate::stations::component_stations`]
/// resolved, or the single legacy no-nacelle point (the wing station,
/// `define_mass_coordinates`'s own `w_root_z - 1.0` offset) when there is no
/// nacelle geometry to place engines at.
fn propulsion_positions(stations: &ComponentStations) -> Vec<[f64; 3]> {
    if stations.propulsion_units.is_empty() {
        vec![[
            stations.wing.position_m[0],
            0.0,
            stations.wing.position_m[2] - 1.0,
        ]]
    } else {
        stations
            .propulsion_units
            .iter()
            .map(|unit| unit.position_m)
            .collect()
    }
}

/// Wing, both tails and the fuselage, as `MassRole::Fixed` Torenbeek items.
///
/// The tails use the same correlation tag as the wing because
/// `calculate_component_masses` derives all three from the identical
/// `mass_wing` Torenbeek method; Torenbeek/fuselage attribution is the one
/// this module's contract names explicitly.
fn push_structure(ledger: &mut MassLedger, masses: &MassBreakdown, stations: &ComponentStations) {
    ledger.push(MassItem {
        id: "wing".to_owned(),
        group: MassGroup::WingStructure,
        role: MassRole::Fixed,
        mass_kg: masses.wing,
        position_m: stations.wing.position_m,
        local_inertia: thin_plate_xy(
            masses.wing,
            stations.wing.extent_m[0],
            stations.wing.extent_m[1],
        ),
        method: MassMethod::Correlation("Torenbeek"),
    });
    ledger.push(MassItem {
        id: "h_stab".to_owned(),
        group: MassGroup::HorizontalTail,
        role: MassRole::Fixed,
        mass_kg: masses.h_stab,
        position_m: stations.horizontal_tail.position_m,
        local_inertia: thin_plate_xy(
            masses.h_stab,
            stations.horizontal_tail.extent_m[0],
            stations.horizontal_tail.extent_m[1],
        ),
        method: MassMethod::Correlation("Torenbeek"),
    });
    ledger.push(MassItem {
        id: "v_stab".to_owned(),
        group: MassGroup::VerticalTail,
        role: MassRole::Fixed,
        mass_kg: masses.v_stab,
        position_m: stations.vertical_tail.position_m,
        local_inertia: thin_plate_xz(
            masses.v_stab,
            stations.vertical_tail.extent_m[0],
            stations.vertical_tail.extent_m[2],
        ),
        method: MassMethod::Correlation("Torenbeek"),
    });
    ledger.push(MassItem {
        id: "fuselage".to_owned(),
        group: MassGroup::Fuselage,
        role: MassRole::Fixed,
        mass_kg: masses.fuselage,
        position_m: stations.fuselage.position_m,
        local_inertia: thin_cylinder_shell_x(
            masses.fuselage,
            stations.fuselage.extent_m[1] / 2.0,
            stations.fuselage.extent_m[0],
        ),
        method: MassMethod::Correlation("Torenbeek"),
    });
}

/// Nose and main gear, split by [`NOSE_GEAR_MASS_FRACTION`], as point masses:
/// no strut geometry is modelled, so a shape-based tensor would be invented.
fn push_gear(ledger: &mut MassLedger, masses: &MassBreakdown, stations: &ComponentStations) {
    let nose_mass = masses.gear * NOSE_GEAR_MASS_FRACTION;
    let main_mass = masses.gear - nose_mass;
    ledger.push(MassItem {
        id: "nose_gear".to_owned(),
        group: MassGroup::LandingGear,
        role: MassRole::Fixed,
        mass_kg: nose_mass,
        position_m: stations.nose_gear.position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::TakeoffMassFraction,
    });
    ledger.push(MassItem {
        id: "main_gear".to_owned(),
        group: MassGroup::LandingGear,
        role: MassRole::Fixed,
        mass_kg: main_mass,
        position_m: stations.main_gear.position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::TakeoffMassFraction,
    });
}

/// Propulsion mass split equally across nacelles (or the single legacy
/// point with no nacelle geometry), each as a solid-cylinder item.
fn push_propulsion(ledger: &mut MassLedger, masses: &MassBreakdown, stations: &ComponentStations) {
    let positions = propulsion_positions(stations);
    let share = masses.propulsion / positions.len() as f64;
    for (index, position_m) in positions.iter().enumerate() {
        let local_inertia = stations
            .propulsion_units
            .get(index)
            .map_or(InertiaTensor::ZERO, |unit| {
                solid_cylinder_x(share, unit.extent_m[1] / 2.0, unit.extent_m[0])
            });
        let id = if positions.len() == 1 {
            "propulsion".to_owned()
        } else {
            format!("propulsion-{index}")
        };
        ledger.push(MassItem {
            id,
            group: MassGroup::Propulsion,
            role: MassRole::Fixed,
            mass_kg: share,
            position_m: *position_m,
            local_inertia,
            method: MassMethod::Correlation("thrust-to-weight"),
        });
    }
}

/// The reference-compatible path: one lumped systems item, one lumped
/// furnishings item, each a rectangular-prism extent from its station.
fn push_lumped_systems_and_furnishings(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
) {
    ledger.push(MassItem {
        id: "systems".to_owned(),
        group: MassGroup::Systems,
        role: MassRole::Fixed,
        mass_kg: masses.systems,
        position_m: stations.systems.position_m,
        local_inertia: rectangular_prism(
            masses.systems,
            stations.systems.extent_m[0],
            stations.systems.extent_m[1],
            stations.systems.extent_m[2],
        ),
        method: MassMethod::TakeoffMassFraction,
    });
    push_furnishings_item(
        ledger,
        masses.furnishings,
        stations,
        MassMethod::TakeoffMassFraction,
    );
}

fn push_furnishings_item(
    ledger: &mut MassLedger,
    mass_kg: f64,
    stations: &ComponentStations,
    method: MassMethod,
) {
    ledger.push(MassItem {
        id: "furnishings".to_owned(),
        group: MassGroup::Furnishings,
        role: MassRole::Fixed,
        mass_kg,
        position_m: stations.furnishings.position_m,
        local_inertia: rectangular_prism(
            mass_kg,
            stations.furnishings.extent_m[0],
            stations.furnishings.extent_m[1],
            stations.furnishings.extent_m[2],
        ),
        method,
    });
}

/// A point-mass ledger item tagged `MassMethod::Correlation("FLOPS")`.
///
/// None of the individual FLOPS systems/operating-item components has a
/// declared shape of its own, so each is a point mass at its station; only
/// the lumped groups above (and the reduced furnishings remainder) get a
/// prism extent.
fn push_flops_item(
    ledger: &mut MassLedger,
    id: &str,
    group: MassGroup,
    role: MassRole,
    mass_kg: f64,
    position_m: [f64; 3],
) {
    ledger.push(MassItem {
        id: id.to_owned(),
        group,
        role,
        mass_kg,
        position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::Correlation("FLOPS"),
    });
}

/// FLOPS's nine systems-and-equipment components at the stations the
/// module doc's placement contract names: all at the systems station
/// except APU (0.95 fuselage length), avionics/instruments (0.10 fuselage
/// length), anti-ice (the wing station), and surface controls (60% wing,
/// split evenly between the two tail surfaces for the remaining 40% --
/// FLOPS has no combined "tails" station this crate can place the tail
/// share at once).
fn push_flops_systems(
    ledger: &mut MassLedger,
    stations: &ComponentStations,
    systems: &FlopsSystemsBreakdown,
) {
    let fuselage_length_m = stations.fuselage.extent_m[0];
    let z = stations.fuselage.position_m[2];
    let apu_station = [fuselage_length_m * 0.95, 0.0, z];
    let forward_bay_station = [fuselage_length_m * 0.10, 0.0, z];
    let systems_station = stations.systems.position_m;
    let wing_station = stations.wing.position_m;

    let items: [(&str, f64, [f64; 3]); 11] = [
        ("systems-apu", systems.apu_kg, apu_station),
        (
            "systems-instruments",
            systems.instruments_kg,
            forward_bay_station,
        ),
        ("systems-avionics", systems.avionics_kg, forward_bay_station),
        ("systems-anti_ice", systems.anti_ice_kg, wing_station),
        ("systems-hydraulics", systems.hydraulics_kg, systems_station),
        ("systems-electrical", systems.electrical_kg, systems_station),
        (
            "systems-furnishings",
            systems.furnishings_kg,
            systems_station,
        ),
        (
            "systems-air_conditioning",
            systems.air_conditioning_kg,
            systems_station,
        ),
        (
            "systems-surface_controls-wing",
            systems.surface_controls_kg * 0.60,
            wing_station,
        ),
        (
            "systems-surface_controls-htail",
            systems.surface_controls_kg * 0.20,
            stations.horizontal_tail.position_m,
        ),
        (
            "systems-surface_controls-vtail",
            systems.surface_controls_kg * 0.20,
            stations.vertical_tail.position_m,
        ),
    ];
    for (id, mass_kg, position_m) in items {
        push_flops_item(
            ledger,
            id,
            MassGroup::Systems,
            MassRole::Fixed,
            mass_kg,
            position_m,
        );
    }
}

/// The four FLOPS operating items the module doc's placement contract
/// names (flight crew at 0.05 fuselage length, cabin crew and passenger
/// service at the operating-items station, engine oil split across the
/// propulsion positions). Unusable fuel and cargo containers are not
/// placed here: unusable fuel arrives through
/// [`super::MassStatementInputs::unusable_fuel_items`], and both remain
/// folded into the reduced furnishings remainder
/// [`push_flops_systems_and_operating_items`] computes, so the group total
/// is still exact.
///
/// Returns the total mass placed, so the caller can size that remainder.
fn push_flops_operating_items(
    ledger: &mut MassLedger,
    stations: &ComponentStations,
    operating_items: &FlopsOperatingItemsBreakdown,
) -> f64 {
    let fuselage_length_m = stations.fuselage.extent_m[0];
    let z = stations.fuselage.position_m[2];
    let flight_crew_station = [fuselage_length_m * 0.05, 0.0, z];

    let named_items: [(&str, f64, [f64; 3]); 3] = [
        (
            "operating-flight_crew",
            operating_items.flight_crew_and_baggage_kg,
            flight_crew_station,
        ),
        (
            "operating-cabin_crew",
            operating_items.cabin_crew_and_baggage_kg,
            stations.operating_items.position_m,
        ),
        (
            "operating-passenger_service",
            operating_items.passenger_service_kg,
            stations.operating_items.position_m,
        ),
    ];
    for (id, mass_kg, position_m) in named_items {
        push_flops_item(
            ledger,
            id,
            MassGroup::OperatingItems,
            MassRole::OperatingItem,
            mass_kg,
            position_m,
        );
    }

    let oil_positions = propulsion_positions(stations);
    let oil_share = operating_items.engine_oil_kg / oil_positions.len() as f64;
    for (index, position_m) in oil_positions.iter().enumerate() {
        let id = if oil_positions.len() == 1 {
            "operating-engine_oil".to_owned()
        } else {
            format!("operating-engine_oil-{index}")
        };
        push_flops_item(
            ledger,
            &id,
            MassGroup::OperatingItems,
            MassRole::OperatingItem,
            oil_share,
            *position_m,
        );
    }

    operating_items.flight_crew_and_baggage_kg
        + operating_items.cabin_crew_and_baggage_kg
        + operating_items.passenger_service_kg
        + operating_items.engine_oil_kg
}

/// The FLOPS path: nine named systems items, four named operating items,
/// and the furnishings remainder reduced so the group sums stay exact --
/// see the module doc's FLOPS split contract.
///
/// `masses.furnishings` already equals FLOPS `furnishings_kg` plus the
/// *entire* operating-items total (including unusable fuel and cargo
/// containers, which this module does not place as their own items), so
/// subtracting only the four explicitly placed items leaves that unusable
/// fuel/cargo remainder inside the lumped furnishings item rather than
/// dropping it.
fn push_flops_systems_and_operating_items(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    flops: &FlopsTransportBreakdown,
) {
    push_flops_systems(ledger, stations, &flops.systems);
    let placed_operating_items_kg =
        push_flops_operating_items(ledger, stations, &flops.operating_items);
    let reduced_furnishings_kg = masses.furnishings - placed_operating_items_kg;
    push_furnishings_item(
        ledger,
        reduced_furnishings_kg,
        stations,
        MassMethod::Correlation("FLOPS"),
    );
}

/// Payload: one item per [`PayloadItemSummary`], or the lumped fallback at
/// [`ComponentStations::payload_fallback`] when the slice is empty.
fn push_payload(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    payload_items: &[PayloadItemSummary],
) {
    if payload_items.is_empty() {
        ledger.push(MassItem {
            id: "payload".to_owned(),
            group: MassGroup::Payload,
            role: MassRole::Payload,
            mass_kg: masses.payload,
            position_m: stations.payload_fallback.position_m,
            local_inertia: rectangular_prism(
                masses.payload,
                stations.payload_fallback.extent_m[0],
                stations.payload_fallback.extent_m[1],
                stations.payload_fallback.extent_m[2],
            ),
            method: MassMethod::LayoutPlacement,
        });
        return;
    }
    for (index, item) in payload_items.iter().enumerate() {
        ledger.push(MassItem {
            id: format!("payload-{index}-{}", item.label),
            group: MassGroup::Payload,
            role: MassRole::Payload,
            mass_kg: item.mass_kg,
            position_m: item.position_m,
            local_inertia: rectangular_prism(
                item.mass_kg,
                item.extent_m[0],
                item.extent_m[1],
                item.extent_m[2],
            ),
            method: MassMethod::LayoutPlacement,
        });
    }
}
