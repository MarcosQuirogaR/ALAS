// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The curated kit must load, validate and be shippable.

use alas_uav_db::kit::kit;
use alas_uav_db::{Body, Category, ComponentId, IsoDate, Method, SourceKind};

#[test]
fn the_kit_loads_validates_and_is_shippable() {
    let db = kit().unwrap();
    let report = db.validate_for_shipping();
    assert!(report.is_ok(), "{}", report.error_text());
    assert_eq!(db.sources.len(), 3);
    assert!(db
        .sources
        .iter()
        .all(|s| s.kind == SourceKind::Manufacturer));
    assert!(db.offers.is_empty() && db.prop_perf.is_empty());
    let today = IsoDate::parse("2026-10-12").unwrap();
    assert_eq!(db.by_category(Category::Motor, &today).count(), 11);
    assert_eq!(db.by_category(Category::Receiver, &today).count(), 2);
}

#[test]
fn every_field_is_a_cited_datasheet_value_with_a_raw_string() {
    let db = kit().unwrap();
    for component in &db.components {
        let json = serde_json::to_value(component).unwrap();
        let mut stack = vec![&json["spec"]];
        while let Some(value) = stack.pop() {
            match value {
                serde_json::Value::Object(map) if map.contains_key("source_id") => {
                    assert_eq!(map["method"], "datasheet", "{}", component.id.as_str());
                    assert!(map["raw"].is_string(), "{}", component.id.as_str());
                    assert!(map["retrieved_at"].is_string());
                }
                serde_json::Value::Object(map) => stack.extend(map.values()),
                _ => {}
            }
        }
    }
}

#[test]
fn hobbywing_2826_values_match_the_cited_table() {
    let db = kit().unwrap();
    let id = ComponentId::new("hobbywing-skywalker-2826-sl-1100kv").unwrap();
    let Body::Motor(motor) = &db.current(&id).unwrap().body else {
        panic!("not a motor");
    };
    let kv = motor.kv_rad_s_per_v.as_ref().unwrap();
    assert!((kv.value - 1100.0 * std::f64::consts::TAU / 60.0).abs() < 1e-9);
    assert_eq!(kv.prov.method, Method::Datasheet);
    assert!((motor.rm_ohm.as_ref().unwrap().value - 0.01408).abs() < 1e-12);
    assert_eq!(motor.i0_a.as_ref().unwrap().value, 3.7);
    assert_eq!(motor.i_max_burst_a.as_ref().unwrap().value, 82.4);
    assert_eq!(motor.burst_s.as_ref().unwrap().value, 135.0);
    assert!((motor.mass_kg.as_ref().unwrap().value - 0.1758).abs() < 1e-12);
    // The vendor calls 82.4 A "continuous" for 2 min 15 s; it is stored as a burst
    // rating, so the continuous rating stays unknown and the circuit model reports it.
    let component = db.current(&id).unwrap();
    assert_eq!(component.missing_for("motor_circuit"), vec!["i_max_cont_a"]);
}

#[test]
fn the_kit_has_no_cross_field_warnings() {
    let report = kit().unwrap().validate();
    let warnings: Vec<String> = report
        .warnings()
        .map(|f| format!("{}: {}", f.path, f.message))
        .collect();
    assert!(warnings.is_empty(), "{warnings:?}");
}
