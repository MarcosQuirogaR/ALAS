// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::flops_transport::{FlopsOperatingItemsBreakdown, FlopsSystemsBreakdown};
use crate::ledger::{InertiaTensor, MassMethod, MassRole};
use crate::stations::ComponentStation;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
}

fn station(x: f64, z: f64) -> ComponentStation {
    ComponentStation {
        position_m: [x, 0.0, z],
        extent_m: [1.0, 1.0, 1.0],
        method: "test fixture",
    }
}

/// A hand-built, geometrically plausible station set: gear, tails and
/// systems all aft of the wing, no nacelle geometry (exercising the
/// legacy no-nacelle propulsion fallback).
fn sample_stations() -> ComponentStations {
    ComponentStations {
        wing: station(20.0, 0.0),
        horizontal_tail: station(38.0, 2.0),
        vertical_tail: station(37.0, 3.0),
        fuselage: ComponentStation {
            position_m: [18.0, 0.0, 0.0],
            extent_m: [40.0, 4.0, 4.0],
            method: "test fixture",
        },
        nose_gear: station(3.0, -2.0),
        main_gear: station(21.0, -2.0),
        propulsion_units: Vec::new(),
        systems: station(15.0, 0.0),
        furnishings: station(16.0, 0.0),
        operating_items: station(16.0, 0.0),
        payload_fallback: station(16.0, 0.0),
    }
}

fn sample_masses() -> MassBreakdown {
    MassBreakdown {
        wing: 8_000.0,
        h_stab: 800.0,
        v_stab: 600.0,
        fuselage: 9_000.0,
        gear: 3_000.0,
        propulsion: 4_000.0,
        systems: 3_000.0,
        furnishings: 4_000.0,
        payload: 15_000.0,
        fuel: 20_000.0,
    }
}

fn fuel_item(id: &str, mass_kg: f64, position_m: [f64; 3]) -> MassItem {
    MassItem {
        id: id.to_owned(),
        group: MassGroup::Fuel,
        role: MassRole::UsableFuel,
        mass_kg,
        position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::TankFill,
    }
}

fn build_statement(
    masses: &MassBreakdown,
    stations: &ComponentStations,
    takeoff_fuel_items: Vec<MassItem>,
) -> MassStatement {
    MassStatement::build(MassStatementInputs {
        masses,
        stations,
        payload_items: &[],
        takeoff_fuel_items,
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: None,
    })
    .expect("a consistent hand-built statement validates")
}

#[test]
fn state_identities_match_the_legacy_breakdown_sums() {
    let masses = sample_masses();
    let stations = sample_stations();
    let takeoff_fuel_items = vec![fuel_item("fuel-takeoff", 12_000.0, [20.0, 0.0, 1.0])];
    let takeoff_fuel_mass: f64 = takeoff_fuel_items.iter().map(|item| item.mass_kg).sum();
    let statement = build_statement(&masses, &stations, takeoff_fuel_items);

    let oew_sum = masses.wing
        + masses.h_stab
        + masses.v_stab
        + masses.fuselage
        + masses.gear
        + masses.propulsion
        + masses.systems
        + masses.furnishings;
    let oew = statement.state(LoadState::OperatingEmpty);
    assert!(
        close(oew.mass_kg, oew_sum),
        "oew={} expected={oew_sum}",
        oew.mass_kg
    );

    let zfw = statement.state(LoadState::ZeroFuel);
    assert!(close(zfw.mass_kg, oew_sum + masses.payload));

    let takeoff = statement.state(LoadState::Takeoff);
    assert!(close(takeoff.mass_kg, zfw.mass_kg + takeoff_fuel_mass));
}

#[test]
fn forward_fuel_moves_the_takeoff_cg_forward_of_zero_fuel() {
    let masses = sample_masses();
    let stations = sample_stations();
    let zero_fuel_cg_x = build_statement(&masses, &stations, Vec::new())
        .state(LoadState::ZeroFuel)
        .cg_m[0];

    let forward_fuel = vec![fuel_item(
        "fuel-forward",
        5_000.0,
        [zero_fuel_cg_x - 15.0, 0.0, 0.0],
    )];
    let statement = build_statement(&masses, &stations, forward_fuel);
    let zero_fuel = statement.state(LoadState::ZeroFuel);
    let takeoff = statement.state(LoadState::Takeoff);
    assert!(takeoff.cg_m[0] < zero_fuel.cg_m[0]);
}

#[test]
fn the_flops_split_preserves_systems_and_furnishings_totals() {
    let systems = FlopsSystemsBreakdown {
        surface_controls_kg: 900.0,
        apu_kg: 400.0,
        instruments_kg: 250.0,
        hydraulics_kg: 500.0,
        electrical_kg: 700.0,
        avionics_kg: 600.0,
        furnishings_kg: 1_800.0,
        air_conditioning_kg: 550.0,
        anti_ice_kg: 150.0,
        total_kg: 900.0 + 400.0 + 250.0 + 500.0 + 700.0 + 600.0 + 1_800.0 + 550.0 + 150.0,
    };
    let operating_items = FlopsOperatingItemsBreakdown {
        cabin_crew_and_baggage_kg: 380.0,
        flight_crew_and_baggage_kg: 190.0,
        unusable_fuel_kg: 120.0,
        engine_oil_kg: 60.0,
        passenger_service_kg: 240.0,
        cargo_containers_kg: 90.0,
        total_kg: 380.0 + 190.0 + 120.0 + 60.0 + 240.0,
        total_with_cargo_containers_kg: 380.0 + 190.0 + 120.0 + 60.0 + 240.0 + 90.0,
    };
    let flops = FlopsTransportBreakdown {
        systems,
        operating_items,
    };
    // The FLOPS buildup puts the equation 138 group less furnishings in the
    // systems slot and furnishings plus the operating items in the
    // furnishings slot, so `WFURN` is carried exactly once.
    let masses = MassBreakdown {
        systems: systems.total_kg - systems.furnishings_kg,
        furnishings: systems.furnishings_kg + operating_items.total_kg,
        ..sample_masses()
    };
    let stations = sample_stations();

    let statement = MassStatement::build(MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: Some(&flops),
    })
    .expect("a consistent FLOPS-split statement validates");

    let totals = statement.group_totals(LoadState::ZeroFuel);
    let systems_total: f64 = totals
        .iter()
        .filter(|(group, _)| *group == MassGroup::Systems)
        .map(|(_, mass_kg)| *mass_kg)
        .sum();
    let furnishings_and_operating_total: f64 = totals
        .iter()
        .filter(|(group, _)| {
            *group == MassGroup::Furnishings || *group == MassGroup::OperatingItems
        })
        .map(|(_, mass_kg)| *mass_kg)
        .sum();

    assert!(
        (systems_total - masses.systems).abs() < 1.0e-6,
        "systems_total={systems_total} expected={}",
        masses.systems
    );
    assert!(
        (furnishings_and_operating_total - masses.furnishings).abs() < 1.0e-6,
        "furnishings_and_operating_total={furnishings_and_operating_total} expected={}",
        masses.furnishings
    );
    // The whole ledger must carry the FLOPS group total and the operating
    // items exactly once: furnishings belong to equation 138's systems group
    // but are placed in the furnishings slot, never in both.
    let carried = systems_total + furnishings_and_operating_total;
    let expected = systems.total_kg + operating_items.total_kg;
    assert!(
        (carried - expected).abs() < 1.0e-6,
        "ledger carried {carried}, FLOPS group plus operating items {expected}"
    );
    assert!(
        !statement
            .ledger()
            .items()
            .iter()
            .any(|item| item.id == "systems-furnishings"),
        "furnishings must not also appear as a systems item"
    );
    for id in [
        "wing",
        "h_stab",
        "v_stab",
        "fuselage",
        "nose_gear",
        "main_gear",
        "propulsion",
    ] {
        let item = statement
            .ledger()
            .items()
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing FLOPS ledger item {id}"));
        assert_eq!(
            item.method,
            MassMethod::Correlation("FLOPS"),
            "a statement carrying FLOPS groups must not retain a legacy label for {id}"
        );
    }
}

/// The FLOPS fixture of [`the_flops_split_preserves_systems_and_furnishings_totals`],
/// reused by the closure tests below.
fn sample_flops() -> FlopsTransportBreakdown {
    let systems = FlopsSystemsBreakdown {
        surface_controls_kg: 900.0,
        apu_kg: 400.0,
        instruments_kg: 250.0,
        hydraulics_kg: 500.0,
        electrical_kg: 700.0,
        avionics_kg: 600.0,
        furnishings_kg: 1_800.0,
        air_conditioning_kg: 550.0,
        anti_ice_kg: 150.0,
        total_kg: 900.0 + 400.0 + 250.0 + 500.0 + 700.0 + 600.0 + 1_800.0 + 550.0 + 150.0,
    };
    let operating_items = FlopsOperatingItemsBreakdown {
        cabin_crew_and_baggage_kg: 380.0,
        flight_crew_and_baggage_kg: 190.0,
        unusable_fuel_kg: 120.0,
        engine_oil_kg: 60.0,
        passenger_service_kg: 240.0,
        cargo_containers_kg: 90.0,
        total_kg: 380.0 + 190.0 + 120.0 + 60.0 + 240.0,
        total_with_cargo_containers_kg: 380.0 + 190.0 + 120.0 + 60.0 + 240.0 + 90.0,
    };
    FlopsTransportBreakdown {
        systems,
        operating_items,
    }
}

/// The breakdown the FLOPS buildup writes for `flops`, plus `margin_kg` of
/// equation 139 empty-mass margin carried in the systems slot.
fn flops_masses(flops: &FlopsTransportBreakdown, margin_kg: f64) -> MassBreakdown {
    MassBreakdown {
        systems: flops.systems.total_kg - flops.systems.furnishings_kg + margin_kg,
        furnishings: flops.systems.furnishings_kg + flops.operating_items.total_kg,
        ..sample_masses()
    }
}

fn flops_statement(
    masses: &MassBreakdown,
    stations: &ComponentStations,
    flops: &FlopsTransportBreakdown,
    unusable_fuel_items: Vec<MassItem>,
) -> Result<MassStatement, LedgerError> {
    MassStatement::build(MassStatementInputs {
        masses,
        stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items,
        flops: Some(flops),
    })
}

fn group_mass(statement: &MassStatement, group: MassGroup) -> f64 {
    statement
        .ledger()
        .items()
        .iter()
        .filter(|item| item.group == group)
        .map(|item| item.mass_kg)
        .sum()
}

#[test]
fn a_positive_empty_mass_margin_is_an_explicit_systems_row_and_the_ledger_closes_exactly() {
    // FLOPS equation 139 adds the empty-mass margin to the three groups, and
    // the ALAS buildup carries it in the systems slot. The eight named
    // systems items cannot account for it, so without an explicit residual
    // row the margin would silently vanish from the ledger.
    let flops = sample_flops();
    let margin_kg = 431.25;
    let masses = flops_masses(&flops, margin_kg);
    let stations = sample_stations();
    let statement = flops_statement(&masses, &stations, &flops, Vec::new())
        .expect("a margin-carrying FLOPS statement validates");

    let residual = statement
        .ledger()
        .items()
        .iter()
        .find(|item| item.id == "systems-empty_mass_margin_and_residual")
        .unwrap_or_else(|| panic!("the margin must be an explicit ledger row"));
    assert!(
        close(residual.mass_kg, margin_kg),
        "residual row {} vs margin {margin_kg}",
        residual.mass_kg
    );
    assert_eq!(residual.position_m, stations.systems.position_m);
    assert_eq!(residual.group, MassGroup::Systems);

    // Exact closure of every group the FLOPS path writes.
    assert!(close(
        group_mass(&statement, MassGroup::Systems),
        masses.systems
    ));
    let furnishings_and_items = group_mass(&statement, MassGroup::Furnishings)
        + group_mass(&statement, MassGroup::OperatingItems);
    assert!(close(furnishings_and_items, masses.furnishings));
    // And of the whole zero-fuel state against the breakdown it came from.
    let zero_fuel = statement.state(LoadState::ZeroFuel);
    let expected: f64 = masses.as_pairs()[..9].iter().map(|(_, mass)| mass).sum();
    assert!(
        close(zero_fuel.mass_kg, expected),
        "ledger ZFW {} vs breakdown {expected}",
        zero_fuel.mass_kg
    );

    // With no margin the residual row is absent rather than zero-valued.
    let no_margin = flops_masses(&flops, 0.0);
    let plain = flops_statement(&no_margin, &stations, &flops, Vec::new())
        .expect("a margin-free FLOPS statement validates");
    assert!(!plain
        .ledger()
        .items()
        .iter()
        .any(|item| item.id == "systems-empty_mass_margin_and_residual"));
}

#[test]
fn a_systems_slot_smaller_than_its_own_components_is_rejected_not_clamped() {
    let flops = sample_flops();
    let masses = flops_masses(&flops, -500.0);
    let stations = sample_stations();
    assert!(matches!(
        flops_statement(&masses, &stations, &flops, Vec::new()),
        Err(LedgerError::InvalidMass { .. })
    ));
}

#[test]
fn supplied_unusable_fuel_rows_replace_the_flops_allocation_instead_of_adding_to_it() {
    // `masses.furnishings` already carries the FLOPS unusable-fuel
    // allocation inside the operating-items total. Placing the resolved tank
    // rows on top without relieving the lumped remainder would count that
    // fuel twice.
    let flops = sample_flops();
    let masses = flops_masses(&flops, 0.0);
    let stations = sample_stations();
    let allocation = flops.operating_items.unusable_fuel_kg;

    let lumped = flops_statement(&masses, &stations, &flops, Vec::new())
        .expect("no supplied rows keeps the allocation lumped");
    let placed = flops_statement(
        &masses,
        &stations,
        &flops,
        vec![
            fuel_item("inner_unusable", 0.6 * allocation, [19.0, 0.0, 0.0]),
            fuel_item("outer_unusable", 0.4 * allocation, [22.0, 0.0, 1.0]),
        ],
    )
    .expect("matching rows reconcile against the allocation");

    // Same total mass either way; only the placement changes.
    assert!(close(
        placed.state(LoadState::ZeroFuel).mass_kg,
        lumped.state(LoadState::ZeroFuel).mass_kg
    ));
    let furnishings_lumped = group_mass(&lumped, MassGroup::Furnishings);
    let furnishings_placed = group_mass(&placed, MassGroup::Furnishings);
    assert!(
        close(furnishings_lumped - furnishings_placed, allocation),
        "the remainder must shed exactly the allocation: {furnishings_lumped} - \
         {furnishings_placed} vs {allocation}"
    );
    assert!(close(group_mass(&placed, MassGroup::Fuel), allocation));
    // The operating-item rows are untouched: no operating item is duplicated.
    assert!(close(
        group_mass(&placed, MassGroup::OperatingItems),
        group_mass(&lumped, MassGroup::OperatingItems)
    ));

    // A disagreement is a typed rejection, not a quiet double count.
    let mismatch = flops_statement(
        &masses,
        &stations,
        &flops,
        vec![fuel_item(
            "inner_unusable",
            allocation * 1.5,
            [19.0, 0.0, 0.0],
        )],
    );
    assert!(matches!(
        mismatch,
        Err(LedgerError::UnusableFuelAllocationMismatch { .. })
    ));
}

#[test]
fn ledger_method_labels_follow_the_authoritative_mass_architecture() {
    use alas_config::MassArchitecture;

    let flops = sample_flops();
    let masses = flops_masses(&flops, 0.0);
    let stations = sample_stations();
    let pure_inputs = || MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: Some(&flops),
    };
    let legacy_inputs = || MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: None,
    };
    let label = |statement: &MassStatement, id: &str| {
        statement
            .ledger()
            .items()
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.method)
            .unwrap_or_else(|| panic!("ledger row {id}"))
    };

    // The pure architecture owns every replaceable group, even if a caller
    // mutates one of the legacy selector mirrors after construction.
    let stale = alas_config::MassModelConfig {
        systems_mass_method: alas_config::SystemsMassMethod::ReferenceCompatibleFractions,
        ..alas_config::MassModelConfig::default()
    };
    let pure =
        MassStatement::build_with_methods(pure_inputs(), LedgerMethods::from_mass_model(&stale))
            .expect("valid pure statement");
    for id in [
        "wing",
        "h_stab",
        "v_stab",
        "fuselage",
        "nose_gear",
        "main_gear",
    ] {
        assert_eq!(
            label(&pure, id),
            MassMethod::Correlation("FLOPS"),
            "row {id}"
        );
    }
    assert_eq!(
        label(&pure, "systems-apu"),
        MassMethod::Correlation("FLOPS")
    );

    // The legacy buildup is still available only when the architecture is
    // explicitly selected by name.
    let mut legacy = alas_config::MassModelConfig {
        mass_architecture: MassArchitecture::LegacyReferenceCompatibleComparison,
        ..alas_config::MassModelConfig::default()
    };
    legacy.apply_architecture();
    let legacy_statement =
        MassStatement::build_with_methods(legacy_inputs(), LedgerMethods::from_mass_model(&legacy))
            .expect("valid legacy comparison statement");
    assert_eq!(
        label(&legacy_statement, "wing"),
        MassMethod::Correlation("Torenbeek")
    );
    assert_eq!(
        label(&legacy_statement, "main_gear"),
        MassMethod::TakeoffMassFraction
    );
    assert_eq!(
        label(&legacy_statement, "propulsion"),
        MassMethod::Correlation("thrust-to-weight")
    );
}

// Building the default product aircraft and running the legacy mass
// analysis are assertions that the default configuration is valid, so a
// failed expect here is that assertion failing, not a library invariant
// being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[test]
fn the_default_aircraft_ledger_tensor_is_physical_and_plausible() {
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;

    use crate::breakdown::run_mass_analysis;
    use crate::stations::component_stations;

    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("the default geometry configuration builds");
    let (masses, _coordinates, _cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let stations = component_stations(
        &plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
    )
    .expect("the default aircraft resolves every station");

    let statement = MassStatement::build(MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: None,
    })
    .expect("the default aircraft ledger validates");

    let zero_fuel = statement.state(LoadState::ZeroFuel);
    assert!(zero_fuel.inertia_cg.is_physical());

    let span_m = plane.wings[0].reference_span();
    let fuselage_length_m = stations.fuselage.extent_m[0];
    let comparison =
        statement.radii_of_gyration_check(LoadState::ZeroFuel, span_m, fuselage_length_m);
    assert!(
        (0.5..=2.0).contains(&comparison.ratio[1]),
        "pitch ratio={} ledger={} reference={}",
        comparison.ratio[1],
        comparison.ledger_radii_m[1],
        comparison.reference_radii_m[1]
    );
    assert!(
        (0.5..=2.0).contains(&comparison.ratio[2]),
        "yaw ratio={} ledger={} reference={}",
        comparison.ratio[2],
        comparison.ledger_radii_m[2],
        comparison.reference_radii_m[2]
    );
}
