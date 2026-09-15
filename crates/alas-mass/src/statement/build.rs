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
use crate::flops_transport::FlopsTransportBreakdown;
use crate::inertia::{
    rectangular_prism, solid_cylinder_x, thin_cylinder_shell_x, thin_plate_xy, thin_plate_xz,
};
use crate::ledger::{
    InertiaTensor, LedgerError, MassGroup, MassItem, MassLedger, MassMethod, MassRole,
};
use crate::stations::ComponentStations;

use super::flops_items::push_flops_systems_and_operating_items;
use super::{LedgerMethods, PayloadItemSummary};

/// Fraction of [`MassBreakdown::gear`] carried by the nose gear at static
/// weight, the rest going to the main gear: Raymer, *Aircraft Design: A
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
    methods: LedgerMethods,
) -> Result<MassLedger, LedgerError> {
    let mut ledger = MassLedger::new();
    push_structure(&mut ledger, masses, stations, methods);
    push_gear(&mut ledger, masses, stations, methods);
    push_propulsion(&mut ledger, masses, stations, methods);
    match flops {
        Some(flops) => push_flops_systems_and_operating_items(
            &mut ledger,
            masses,
            stations,
            flops,
            &unusable_fuel_items,
        )?,
        None => push_lumped_systems_and_furnishings(&mut ledger, masses, stations),
    }
    for item in unusable_fuel_items {
        ledger.push(item);
    }
    push_payload(&mut ledger, masses, stations, payload_items);
    Ok(ledger)
}

/// Relative tolerance for the two group-closure checks below: the
/// systems-group residual and the unusable-fuel allocation. Both compare
/// sums of `f64` masses that travelled through several correlations, so an
/// exact equality test would fail on rounding alone; anything larger than
/// this is a real disagreement and is surfaced, not absorbed.
const CLOSURE_RELATIVE_TOLERANCE: f64 = 1.0e-6;

pub(super) fn closure_tolerance(reference_kg: f64) -> f64 {
    CLOSURE_RELATIVE_TOLERANCE * reference_kg.abs().max(1.0)
}

/// The nacelle mid-length points [`crate::stations::component_stations`]
/// resolved, or the single legacy no-nacelle point (the wing station,
/// `define_mass_coordinates`'s own `w_root_z - 1.0` offset) when there is no
/// nacelle geometry to place engines at.
pub(super) fn propulsion_positions(stations: &ComponentStations) -> Vec<[f64; 3]> {
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

/// Wing, both tails and the fuselage, as `MassRole::Fixed` items tagged with
/// whichever structural method actually produced them.
///
/// The tails carry the same tag as the wing: both the reference-compatible
/// buildup (all three from the identical `mass_wing` Torenbeek method) and
/// the FLOPS structural group derive them together.
fn push_structure(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    methods: LedgerMethods,
) {
    let method = methods.structure;
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
        method,
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
        method,
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
        method,
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
        method,
    });
}

/// Nose and main gear, split by [`NOSE_GEAR_MASS_FRACTION`], as point masses:
/// no strut geometry is modelled, so a shape-based tensor would be invented.
fn push_gear(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    methods: LedgerMethods,
) {
    let nose_mass = masses.gear * NOSE_GEAR_MASS_FRACTION;
    let main_mass = masses.gear - nose_mass;
    ledger.push(MassItem {
        id: "nose_gear".to_owned(),
        group: MassGroup::LandingGear,
        role: MassRole::Fixed,
        mass_kg: nose_mass,
        position_m: stations.nose_gear.position_m,
        local_inertia: InertiaTensor::ZERO,
        method: methods.landing_gear,
    });
    ledger.push(MassItem {
        id: "main_gear".to_owned(),
        group: MassGroup::LandingGear,
        role: MassRole::Fixed,
        mass_kg: main_mass,
        position_m: stations.main_gear.position_m,
        local_inertia: InertiaTensor::ZERO,
        method: methods.landing_gear,
    });
}

/// Propulsion mass split equally across nacelles (or the single legacy
/// point with no nacelle geometry), each as a solid-cylinder item.
fn push_propulsion(
    ledger: &mut MassLedger,
    masses: &MassBreakdown,
    stations: &ComponentStations,
    methods: LedgerMethods,
) {
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
            method: methods.propulsion,
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

pub(super) fn push_furnishings_item(
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
