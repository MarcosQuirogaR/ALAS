// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The v1 to v2 converter on the repository's real v1 catalogues.

use alas_uav_db::v1::{convert_v1, V1Options};
use alas_uav_db::{Body, ComponentId, IsoDate, Method};

fn options() -> V1Options {
    V1Options {
        retrieved_at: None,
        today: IsoDate::parse("2026-10-12").unwrap(),
    }
}

fn v1(name: &str) -> String {
    let path = format!("{}/../alas-uav/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn every_v1_catalogue_converts_and_validates() {
    for name in [
        "multi_source_catalog.json",
        "rc_innovations_reviewed.json",
        "rc_innovations_seed.json",
    ] {
        let text = v1(name);
        let records = serde_json::from_str::<serde_json::Value>(&text).unwrap()["records"]
            .as_array()
            .unwrap()
            .len();
        let conversion = convert_v1(&text, &options()).unwrap();
        let skipped: Vec<_> = conversion
            .warnings
            .iter()
            .filter(|w| w.contains("skipped"))
            .collect();
        assert!(skipped.is_empty(), "{name}: {skipped:?}");
        assert_eq!(conversion.database.components.len(), records, "{name}");
        let report = conversion.database.validate();
        assert!(report.is_ok(), "{name}: {}", report.error_text());
        // Retailer data is for local analysis only.
        assert!(!conversion.database.validate_for_shipping().is_ok());
    }
}

#[test]
fn motor_fields_are_si_with_per_field_provenance() {
    let conversion = convert_v1(&v1("multi_source_catalog.json"), &options()).unwrap();
    let id = ComponentId::new("tmotor-at1050-kv90").unwrap();
    let component = conversion.database.current(&id).unwrap();
    let Body::Motor(motor) = &component.body else {
        panic!("not a motor");
    };
    let kv = motor.kv_rad_s_per_v.as_ref().unwrap();
    assert!((kv.value - 90.0 * std::f64::consts::TAU / 60.0).abs() < 1e-9);
    assert_eq!(kv.prov.confidence, 0.5);
    assert_eq!(kv.prov.method, Method::Datasheet);
    assert!(kv.prov.retrieved_at.is_none());
    assert!((motor.diameter_m.as_ref().unwrap().value - 0.1106).abs() < 1e-12);
    assert!((motor.length_m.as_ref().unwrap().value - 0.1198).abs() < 1e-12);
    assert!(
        motor.i_max_cont_a.is_none(),
        "peak current must not become a continuous rating"
    );
    assert_eq!(motor.i_max_burst_a.as_ref().unwrap().value, 135.0);
    assert_eq!(component.missing_for("motor_circuit").len(), 4);
}
