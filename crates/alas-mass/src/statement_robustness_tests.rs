// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn itemized_payload_must_close_to_the_component_payload() {
    let masses = sample_masses();
    let stations = sample_stations();
    let build = |payload_items: &[PayloadItemSummary]| {
        MassStatement::build(MassStatementInputs {
            masses: &masses,
            stations: &stations,
            payload_items,
            takeoff_fuel_items: Vec::new(),
            landing_fuel_items: Vec::new(),
            unusable_fuel_items: Vec::new(),
            flops: None,
        })
    };
    let mut items = vec![PayloadItemSummary {
        label: "passengers and baggage".to_owned(),
        mass_kg: masses.payload - 100.0,
        position_m: [18.0, 0.0, 0.0],
        extent_m: [10.0, 3.0, 1.0],
    }];
    assert!(matches!(
        build(&items),
        Err(LedgerError::PayloadAllocationMismatch { .. })
    ));
    items[0].mass_kg = masses.payload;
    let statement = build(&items).expect("matching payload closes");
    assert!(close(
        statement.state(LoadState::ZeroFuel).mass_kg
            - statement.state(LoadState::OperatingEmpty).mass_kg,
        masses.payload,
    ));
    // Floating-point summation noise is accepted; a physical mass discrepancy is not.
    items[0].mass_kg += 1.0e-8;
    assert!(build(&items).is_ok());
    items[0].mass_kg = f64::NAN;
    assert!(matches!(
        build(&items),
        Err(LedgerError::PayloadAllocationMismatch { .. })
    ));
}

#[test]
fn lth_cabin_and_declared_turboprop_oil_retain_their_sources() {
    let mut flops = sample_flops();
    flops.cabin_equipment_method = alas_config::CabinEquipmentMethod::LthCivilTransportV1;
    flops.propulsion_sizing = crate::flops_transport::PropulsionSizing::ShaftPower {
        engine_oil_kg: flops.operating_items.engine_oil_kg,
    };
    let masses = flops_masses(&flops, 0.0);
    let statement = flops_statement(&masses, &sample_stations(), &flops, Vec::new())
        .expect("source metadata does not change mass closure");
    let method = |id: &str| {
        statement
            .ledger()
            .items()
            .iter()
            .find(|item| item.id == id)
            .expect("named source row")
            .method
    };
    assert_eq!(
        method("operating-passenger_service").label(),
        "LTH civil transport cabin"
    );
    assert_eq!(method("operating-engine_oil"), MassMethod::Declared);
    assert_eq!(method("systems-apu").label(), "FLOPS");
    assert_eq!(
        method("furnishings").label(),
        "LTH furnishings + FLOPS unusable fuel"
    );
    assert!(close(
        statement.state(LoadState::OperatingEmpty).mass_kg,
        masses.as_pairs()[..8].iter().map(|(_, mass)| mass).sum()
    ));
}

#[test]
fn registered_propulsion_sources_are_not_inferred_from_the_architecture_name() {
    use alas_geom::builder::AircraftBuilder;
    for name in ["A320-200", "ATR72-600"] {
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": name}))
            .expect("registered configuration");
        let preset = alas_config::presets::get(name).expect("registered preset");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let crate::breakdown::ProductMassBuildup::PureFlops(buildup) =
            crate::breakdown::calculate_flops_mass_buildup(
                &plane,
                &config.requirements,
                &config.geometry,
                &config.control_surfaces,
                Some(&config.mass_model),
                &config.landing_gear,
                &config.cabin,
            )
            .expect("preset buildup")
        else {
            panic!("production architecture")
        };
        let label = LedgerMethods::from_buildup(&buildup).propulsion.label();
        if name == "A320-200" {
            assert_eq!(label, "FLOPS + LTH pylons");
        } else {
            assert_eq!(label, "declared engine + GASP/TM-83458 + FLOPS fuel system");
        }
    }
}
