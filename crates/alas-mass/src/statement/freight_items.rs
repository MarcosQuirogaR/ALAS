// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ledger rows of a civil main-deck freighter's doors, freight floor and
//! pallet handling system, at the stations
//! [`crate::flops_transport::main_deck_freight`] resolves. No mass changes:
//! the rows come out of the fuselage item and the systems group.

use crate::flops_transport::FlopsTransportBreakdown;
use crate::ledger::{InertiaTensor, MassGroup, MassItem, MassLedger, MassMethod, MassRole};
use crate::stations::ComponentStations;

/// Push the freighter's door and freight-floor rows and return the mass they
/// take out of the fuselage item, kg (zero for every other aircraft).
pub(super) fn push_freighter_structure(
    ledger: &mut MassLedger,
    stations: &ComponentStations,
    flops: Option<&FlopsTransportBreakdown>,
    method: MassMethod,
) -> f64 {
    let Some(placement) = flops.and_then(|flops| flops.main_deck_freight) else {
        return 0.0;
    };
    let z = stations.fuselage.position_m[2];
    let rows = [
        (
            "fuselage-nose_cargo_door",
            placement.nose_door_kg,
            placement.nose_door_x_m,
        ),
        (
            "fuselage-main_deck_cargo_door",
            placement.side_door_kg,
            placement.side_door_x_m,
        ),
        (
            "fuselage-freight_floor",
            placement.floor_kg(),
            placement.deck_centroid_x_m,
        ),
    ];
    let mut placed_kg = 0.0;
    for (id, mass_kg, x_m) in rows {
        if mass_kg > 0.0 {
            ledger.push(MassItem {
                id: id.to_owned(),
                group: MassGroup::Fuselage,
                role: MassRole::Fixed,
                mass_kg,
                position_m: [x_m, 0.0, z],
                local_inertia: InertiaTensor::ZERO,
                method,
            });
            placed_kg += mass_kg;
        }
    }
    placed_kg
}

/// The station of the pallet handling system on the main deck, when the
/// aircraft is a civil main-deck freighter.
pub(super) fn deck_handling_station(
    stations: &ComponentStations,
    flops: &FlopsTransportBreakdown,
) -> Option<[f64; 3]> {
    flops.main_deck_freight.map(|placement| {
        [
            placement.deck_centroid_x_m,
            0.0,
            stations.fuselage.position_m[2],
        ]
    })
}
