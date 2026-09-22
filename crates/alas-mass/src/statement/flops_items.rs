// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Placing the FLOPS systems, empty-mass margin and operating items as
//! named ledger rows.
//!
//! [`super::build`] owns the reference-compatible placement, where systems
//! and furnishings are two lumped items. This module owns the FLOPS path,
//! which is a different physical contract: nine named components, four named
//! operating items, and three group closures that are enforced rather than
//! assumed (see [`push_flops_systems_and_operating_items`]).

use crate::breakdown::MassBreakdown;
use crate::flops_transport::{
    FlopsOperatingItemsBreakdown, FlopsSystemsBreakdown, FlopsTransportBreakdown,
};
use crate::ledger::{
    InertiaTensor, LedgerError, MassGroup, MassItem, MassLedger, MassMethod, MassRole,
};
use crate::stations::ComponentStations;

use super::build::{closure_tolerance, propulsion_positions, push_furnishings_item};

/// A point-mass ledger item retaining the equation source that produced it.
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
    method: MassMethod,
) {
    ledger.push(MassItem {
        id: id.to_owned(),
        group,
        role,
        mass_kg,
        position_m,
        local_inertia: InertiaTensor::ZERO,
        method,
    });
}

/// Eight of FLOPS's nine systems-and-equipment components at the stations
/// the module doc's placement contract names: all at the systems station
/// except APU (0.95 fuselage length), avionics/instruments (0.10 fuselage
/// length), anti-ice (the wing station), and surface controls (60% wing,
/// split evenly between the two tail surfaces for the remaining 40%:
/// FLOPS has no combined "tails" station this crate can place the tail
/// share at once).
///
/// Furnishings, the ninth component of FLOPS equation 138's group, are
/// deliberately not pushed here: [`MassBreakdown::furnishings`] owns them,
/// and [`push_flops_systems_and_operating_items`] places them once through
/// [`push_furnishings_item`]. Pushing them in both places double-counted
/// `WFURN` in the ledger's group totals.
///
/// Returns the mass actually placed, so the caller can size the
/// empty-mass-margin residual against [`MassBreakdown::systems`].
fn push_flops_systems(
    ledger: &mut MassLedger,
    stations: &ComponentStations,
    systems: &FlopsSystemsBreakdown,
) -> f64 {
    let fuselage_length_m = stations.fuselage.extent_m[0];
    let z = stations.fuselage.position_m[2];
    let apu_station = [fuselage_length_m * 0.95, 0.0, z];
    let forward_bay_station = [fuselage_length_m * 0.10, 0.0, z];
    let systems_station = stations.systems.position_m;
    let wing_station = stations.wing.position_m;

    let items: [(&str, f64, [f64; 3]); 10] = [
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
    let mut placed_kg = 0.0;
    for (id, mass_kg, position_m) in items {
        push_flops_item(
            ledger,
            id,
            MassGroup::Systems,
            MassRole::Fixed,
            mass_kg,
            position_m,
            MassMethod::Correlation("FLOPS"),
        );
        placed_kg += mass_kg;
    }
    placed_kg
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
    flops: &FlopsTransportBreakdown,
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
            if flops.cabin_equipment_method
                == alas_config::CabinEquipmentMethod::LthCivilTransportV1
            {
                MassMethod::Correlation("LTH civil transport cabin")
            } else {
                MassMethod::Correlation("FLOPS")
            },
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
            match flops.propulsion_sizing {
                crate::flops_transport::PropulsionSizing::RatedThrust => {
                    MassMethod::Correlation("FLOPS")
                }
                crate::flops_transport::PropulsionSizing::ShaftPower { .. } => MassMethod::Declared,
            },
        );
    }

    operating_items.flight_crew_and_baggage_kg
        + operating_items.cabin_crew_and_baggage_kg
        + operating_items.passenger_service_kg
        + operating_items.engine_oil_kg
}

/// The FLOPS path: eight named systems items, the systems-group residual,
/// four named operating items, and the furnishings item reduced so every
/// group sum closes exactly, see the module doc's placement contract.
///
/// Three closures are enforced here rather than assumed.
///
/// **Systems.** The eight named items sum to the equation 138 group *less*
/// furnishings, which is what the FLOPS buildup writes into
/// [`MassBreakdown::systems`] *before* it adds the equation 139 empty-mass
/// margin to the same slot. The difference is pushed as an explicit
/// `systems-empty_mass_margin_and_residual` row at the systems station, so a
/// configured margin appears in the ledger instead of vanishing from it. A
/// residual more negative than [`closure_tolerance`] is pushed unchanged and
/// rejected by [`MassLedger::validate`] as an invalid mass: a systems slot
/// smaller than the components it is supposed to contain is a fault, not
/// something to clamp away.
///
/// **Unusable fuel.** [`MassBreakdown::furnishings`] carries FLOPS
/// `furnishings_kg` plus the *entire* operating-items total, unusable fuel
/// included. With no caller-supplied unusable-fuel rows that allocation stays
/// inside the lumped furnishings remainder. When the caller does supply rows
/// (from the resolved tanks), their sum must agree with the FLOPS allocation
/// within [`closure_tolerance`] and the allocation is subtracted from the
/// remainder before the rows are added, so the fuel is placed once at its
/// real tank stations. A disagreement returns
/// [`LedgerError::UnusableFuelAllocationMismatch`] rather than silently
/// double-counting or silently dropping either number.
///
/// **Cargo containers.** Not placed as their own row, so they remain inside
/// the furnishings remainder and the group total stays exact.
pub(super) fn push_flops_systems_and_operating_items(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    flops: &FlopsTransportBreakdown,
    unusable_fuel_items: &[MassItem],
) -> Result<(), LedgerError> {
    let placed_systems_kg = push_flops_systems(ledger, stations, &flops.systems);
    let systems_residual_kg = masses.systems - placed_systems_kg;
    if systems_residual_kg.abs() > closure_tolerance(masses.systems) {
        push_flops_item(
            ledger,
            "systems-empty_mass_margin_and_residual",
            MassGroup::Systems,
            MassRole::Fixed,
            systems_residual_kg,
            stations.systems.position_m,
            MassMethod::Correlation("FLOPS empty mass margin"),
        );
    }

    let placed_operating_items_kg =
        push_flops_operating_items(ledger, stations, &flops.operating_items, flops);
    let allocated_unusable_kg = flops.operating_items.unusable_fuel_kg;
    let supplied_unusable_kg: f64 = unusable_fuel_items.iter().map(|item| item.mass_kg).sum();
    let relieved_unusable_kg = if unusable_fuel_items.is_empty() {
        0.0
    } else {
        if (supplied_unusable_kg - allocated_unusable_kg).abs()
            > closure_tolerance(allocated_unusable_kg)
        {
            return Err(LedgerError::UnusableFuelAllocationMismatch {
                supplied_kg: supplied_unusable_kg,
                allocated_kg: allocated_unusable_kg,
            });
        }
        allocated_unusable_kg
    };

    let reduced_furnishings_kg =
        masses.furnishings - placed_operating_items_kg - relieved_unusable_kg;
    push_furnishings_item(
        ledger,
        reduced_furnishings_kg,
        stations,
        if flops.cabin_equipment_method == alas_config::CabinEquipmentMethod::LthCivilTransportV1 {
            if relieved_unusable_kg == 0.0 && allocated_unusable_kg > 0.0 {
                MassMethod::Correlation("LTH furnishings + FLOPS unusable fuel")
            } else {
                MassMethod::Correlation("LTH civil transport cabin")
            }
        } else {
            MassMethod::Correlation("FLOPS")
        },
    );
    Ok(())
}
