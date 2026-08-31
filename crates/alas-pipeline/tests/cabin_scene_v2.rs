// SPDX-License-Identifier: AGPL-3.0-or-later
//! Contract and fidelity tests for the resolved cabin-scene interchange.

use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::{CabinScene, FullAnalysis, CABIN_SCENE_SCHEMA_VERSION};

fn scene() -> CabinScene {
    let config = AlasConfig::default();
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), false)
        .expect("default full analysis");
    CabinScene::from_run(&config, &report).expect("resolved cabin scene")
}

#[test]
fn v2_scene_preserves_units_frame_and_resolved_entities() {
    let scene = scene();
    assert_eq!(scene.schema_version, CABIN_SCENE_SCHEMA_VERSION);
    assert_eq!(scene.units.length, "m");
    assert_eq!(scene.frame.y, "starboard");
    assert!(!scene.stations.is_empty());
    assert!(!scene.seat_rows.is_empty());
    assert!(scene.seats.len() >= scene.seat_rows.len());
    assert!(scene.seats.iter().all(|seat| seat.width_m > 0.0));
}

#[test]
fn v2_scene_never_promotes_nominal_geometry_to_authoritative() {
    let scene = scene();
    assert_eq!(scene.windows.status, "nominal_fallback");
    assert!(scene
        .windows
        .apertures
        .iter()
        .all(|window| window.fidelity.contains("nominal")));
    assert!(scene
        .overhead
        .topology
        .iter()
        .all(|topology| topology.status == "missing_supplier_topology"));
    assert!(!scene.cargo.empty_slot_inventory.available);
    assert!(scene
        .missing_inputs
        .iter()
        .any(|entry| entry.field == "cargo.items[].orientation"));
}

#[test]
fn v2_json_round_trip_keeps_individual_seat_ids() {
    let original = scene();
    let json = serde_json::to_string(&original).expect("serialize scene");
    let decoded: CabinScene = serde_json::from_str(&json).expect("deserialize scene");
    assert_eq!(decoded.seats, original.seats);
    assert!(decoded
        .seats
        .windows(2)
        .all(|pair| pair[0].id != pair[1].id));
}

#[test]
fn checked_in_contract_fixture_deserializes() {
    let fixture = include_str!("../../../docs/schemas/fixtures/alas.cabin-scene.v2.contract.json");
    let scene: CabinScene = serde_json::from_str(fixture).expect("deserialize contract fixture");
    assert_eq!(scene.schema_version, CABIN_SCENE_SCHEMA_VERSION);
    assert!(scene.missing_inputs[0]
        .reason
        .contains("no aircraft result"));
}

#[test]
fn recommendations_select_occupied_cabin_and_hold_stations() {
    let scene = scene();
    let cabin = scene
        .recommended_sections
        .iter()
        .find(|section| section.purpose == "occupied_cabin_with_overhead")
        .expect("recommended passenger section");
    assert!(cabin.intersects.iter().any(|kind| kind == "seat_row"));
    assert!(cabin.intersects.iter().any(|kind| kind == "overhead_bin"));
    assert!(scene
        .stations
        .iter()
        .any(|station| station.id == cabin.station_id));

    if !scene.cargo.items.is_empty() {
        let hold = scene
            .recommended_sections
            .iter()
            .find(|section| section.purpose == "occupied_hold")
            .expect("recommended cargo section");
        assert!(hold
            .intersects
            .iter()
            .any(|kind| kind == "uld" || kind == "bag"));
        assert!(scene
            .stations
            .iter()
            .any(|station| station.id == hold.station_id));
    }
}

#[test]
fn narrowbody_recommendations_prefer_the_central_half_of_the_occupied_cabin() {
    for name in ["A220-300", "A320-200"] {
        let preset = alas_config::presets::get(name).expect("registered narrowbody preset");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("preset configuration");
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, false)
            .expect("preset full analysis");
        let scene = CabinScene::from_run(&config, &report).expect("preset cabin scene");
        let recommendation = scene
            .recommended_sections
            .iter()
            .find(|section| {
                section.purpose == "occupied_cabin_with_overhead" && section.deck_id == "main"
            })
            .expect("main-deck recommendation");
        let x_min = scene
            .seat_rows
            .iter()
            .filter(|row| row.deck_id == "main")
            .map(|row| row.envelope.center_x_m - row.envelope.length_m * 0.5)
            .fold(f64::INFINITY, f64::min);
        let x_max = scene
            .seat_rows
            .iter()
            .filter(|row| row.deck_id == "main")
            .map(|row| row.envelope.center_x_m + row.envelope.length_m * 0.5)
            .fold(f64::NEG_INFINITY, f64::max);
        let quarter = (x_max - x_min) * 0.25;
        assert!(
            (x_min + quarter..=x_max - quarter).contains(&recommendation.x_m),
            "{name}: recommended x={} outside central half [{}, {}]",
            recommendation.x_m,
            x_min + quarter,
            x_max - quarter
        );
        assert!(recommendation
            .intersects
            .iter()
            .any(|kind| kind == "overhead_bin"));
    }
}
