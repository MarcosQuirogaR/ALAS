// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 electrical-extension contracts for reviewed UAV selections.

// Fixture construction failures identify a broken source-backed integration
// contract, while the public renderer itself returns typed errors.
#![allow(clippy::expect_used)]

use alas_uav::propulsion_electric::{
    enrich_cpacs35_with_uav_electrical, render_uav_cpacs35_toolspecific,
    simulate_catalogue_mission, solve_catalogue_powertrain, CataloguePowertrainSelection,
    ElectricFlightCondition, ElectricMissionPhase, ElectricMissionPlan, UavCpacs35ElectricalData,
    UAV_ELECTRICAL_NAMESPACE,
};
use alas_uav::{estimate_procurement_cost, full_catalog, ProcurementSelection};

#[test]
fn selected_electrical_data_renders_as_a_namespace_bound_cpacs_extension() {
    let data = electrical_data();
    let xml = render_uav_cpacs35_toolspecific(&data).expect("reviewed data renders");

    assert!(xml.contains("<toolspecific>"));
    assert!(xml.contains(&format!(
        "<uavElectrical xmlns=\"{UAV_ELECTRICAL_NAMESPACE}\">"
    )));
    assert!(xml.contains("<batteryId>unmannedtech-gensace-gtech-2200-3s-45c-xt60</batteryId>"));
    assert!(xml.contains("<unitPriceMinor>2474</unitPriceMinor>"));
    assert!(xml.contains("<observedOnUtc>2026-08-23</observedOnUtc>"));
    assert!(xml.contains("<mission>"));
    assert!(xml.contains("<phase>"));
    assert!(xml.contains("<totalDurationS>900</totalDurationS>"));
    assert!(!xml.contains("NaN"));
}

#[test]
fn extension_is_inserted_without_taking_ownership_of_the_cpacs_document() {
    let document = "<?xml version=\"1.0\"?><cpacs><header/></cpacs>";
    let enriched =
        enrich_cpacs35_with_uav_electrical(document, "alas_uav_cpacs35.xsd", &electrical_data())
            .expect("a CPACS root without toolspecific data can be enriched");

    assert!(enriched.contains("xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\""));
    assert!(enriched.contains(&format!(
        "xsi:schemaLocation=\"{UAV_ELECTRICAL_NAMESPACE} alas_uav_cpacs35.xsd\""
    )));
    assert!(
        enriched.find("<toolspecific>").expect("extension exists")
            < enriched.find("</cpacs>").expect("CPACS close exists")
    );
    assert!(enrich_cpacs35_with_uav_electrical(
        "<cpacs><toolspecific/></cpacs>",
        "alas_uav_cpacs35.xsd",
        &electrical_data(),
    )
    .is_err());
    assert!(enrich_cpacs35_with_uav_electrical(
        "<notCpacs><cpacs/></notCpacs>",
        "alas_uav_cpacs35.xsd",
        &electrical_data(),
    )
    .is_err());
}

#[test]
fn renderer_escapes_catalogue_text_and_rejects_an_empty_propulsor_count() {
    let mut data = electrical_data();
    data.selection.battery_id = "pack&<unsafe>".to_owned();
    let escaped = render_uav_cpacs35_toolspecific(&data).expect("text is escaped");
    assert!(escaped.contains("<batteryId>pack&amp;&lt;unsafe&gt;</batteryId>"));

    data.selection.motor_count = 0;
    assert!(render_uav_cpacs35_toolspecific(&data).is_err());
}

fn electrical_data() -> UavCpacs35ElectricalData {
    let catalog = full_catalog().expect("reviewed catalogue parses");
    let selection = CataloguePowertrainSelection::single_motor(
        "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
        "tmotor-at2814-900kv",
        "hobbywing-skywalker-30a-v2-mini",
        "apc-12x6e",
    );
    let operating_point = solve_catalogue_powertrain(
        &catalog,
        &selection,
        ElectricFlightCondition::full_power(20.0, 1.225),
    )
    .expect("reviewed selection solves");
    let procurement = estimate_procurement_cost(&[
        ProcurementSelection {
            component_id: selection.battery_id.clone(),
            quantity: 1,
        },
        ProcurementSelection {
            component_id: selection.motor_id.clone(),
            quantity: 1,
        },
    ]);
    let mission = simulate_catalogue_mission(
        &catalog,
        &selection,
        &ElectricMissionPlan {
            evidence: "CPACS source-bounded mission example".to_owned(),
            phases: vec![
                phase("departure", 30.0, 10.0, 1.0),
                phase("climb", 90.0, 15.0, 0.95),
                phase("cruise", 720.0, 20.0, 0.95),
                phase("approach reserve", 60.0, 10.0, 0.95),
            ],
        },
    )
    .expect("reviewed phase mission solves");

    UavCpacs35ElectricalData {
        selection,
        operating_point,
        mission: Some(mission),
        procurement: Some(procurement),
    }
}

fn phase(name: &str, duration_s: f64, speed_m_s: f64, throttle: f64) -> ElectricMissionPhase {
    ElectricMissionPhase {
        name: name.to_owned(),
        duration_s,
        condition: ElectricFlightCondition {
            speed_m_s,
            air_density_kg_m3: 1.225,
            throttle,
        },
    }
}
