// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! CSV import, validation, shipping gate, overlay and propeller-map import.

use alas_uav_db::import::{
    import_components, import_offers, import_prop_perf, ImportSpec, OfferImportSpec, PerfImportSpec,
};
use alas_uav_db::overlay;
use alas_uav_db::{
    Body, Category, ComponentId, Database, IsoDate, Method, PerfKind, Redistribution, Source,
    SourceId, SourceKind,
};

fn date(text: &str) -> IsoDate {
    IsoDate::parse(text).unwrap()
}

fn source(id: &str, redistribution: Redistribution, kind: SourceKind) -> Source {
    Source {
        id: SourceId::new(id).unwrap(),
        url: "https://example.org/spec".into(),
        kind,
        licence: "none stated".into(),
        redistribution,
        tos_checked_at: None,
        robots_checked_at: None,
        content_hash: None,
        citation: "Example Motors, product page".into(),
    }
}

fn spec(category: Category, source: &str) -> ImportSpec {
    ImportSpec {
        category,
        source_id: SourceId::new(source).unwrap(),
        retrieved_at: Some(date("2026-10-12")),
        method: Method::Datasheet,
        today: date("2026-10-12"),
        delimiter: ',',
    }
}

const MOTOR_CSV: &str = "mpn,manufacturer,kv [rpm/V],rm [mOhm],i0 [A],i0.test_voltage [V],i_max_cont [A],mass [g],diameter [mm],length [mm],cells_max,p_max [W]\n\
Demo 900,Example Motors,900,95,0.6,10,35,98,35,40,4,420\n\
Bad row,Example Motors,fast,95,0.6,10,35,98,35,40,4,420\n";

fn motor_db() -> Database {
    let out = import_components(MOTOR_CSV, &spec(Category::Motor, "example-motor")).unwrap();
    assert_eq!(out.components.len(), 1);
    assert_eq!(out.rejected.len(), 1);
    assert_eq!(out.rejected[0].row, 3);
    let mut db = Database::new();
    db.sources.push(source(
        "example-motor",
        Redistribution::FactsWithCitation,
        SourceKind::Manufacturer,
    ));
    db.components = out.components;
    db
}

#[test]
fn csv_import_converts_units_keeps_raw_and_test_conditions() {
    let db = motor_db();
    let Body::Motor(motor) = &db.components[0].body else {
        panic!("not a motor");
    };
    let kv = motor.kv_rad_s_per_v.as_ref().unwrap();
    assert!((kv.value - 900.0 * std::f64::consts::TAU / 60.0).abs() < 1e-9);
    assert_eq!(kv.prov.raw.as_deref(), Some("900 rpm/V"));
    assert_eq!(motor.rm_ohm.as_ref().unwrap().value, 0.095);
    assert_eq!(
        motor
            .i0_a
            .as_ref()
            .unwrap()
            .prov
            .test
            .as_ref()
            .unwrap()
            .voltage_v,
        Some(10.0)
    );
    assert_eq!(db.components[0].id.as_str(), "example-motors-demo-900");
    let report = db.validate();
    assert!(report.is_ok(), "{}", report.error_text());
    assert!(db.validate_for_shipping().is_ok());
}

#[test]
fn wrong_unit_class_and_unknown_columns_fail_the_header() {
    let csv = "mpn,manufacturer,kv [g]\nx,y,1\n";
    let error = import_components(csv, &spec(Category::Motor, "s"));
    assert!(error.unwrap().rejected.len() == 1);
    let unknown = import_components(
        "mpn,manufacturer,flux\nx,y,1\n",
        &spec(Category::Motor, "s"),
    );
    assert!(unknown.is_err());
}

#[test]
fn validation_catches_range_unit_and_source_errors() {
    let mut db = motor_db();
    let Body::Motor(motor) = &mut db.components[0].body else {
        panic!("not a motor");
    };
    motor.mass_kg.as_mut().unwrap().value = 98.0; // grams entered as kilograms
    motor.rm_ohm.as_mut().unwrap().unit_si = "mOhm".into();
    motor.i0_a.as_mut().unwrap().prov.source_id = SourceId::new("nowhere").unwrap();
    let text = db.validate().error_text();
    assert!(text.contains("mass_kg"), "{text}");
    assert!(text.contains("unit_si must be 'Ohm'"), "{text}");
    assert!(text.contains("unknown source_id 'nowhere'"), "{text}");
}

#[test]
fn shipping_gate_refuses_unreviewed_sources_and_offers() {
    let mut db = motor_db();
    db.sources[0].redistribution = Redistribution::Unreviewed;
    assert!(!db.validate_for_shipping().is_ok());
    db.sources[0].redistribution = Redistribution::FactsWithCitation;
    let csv = "component_id,vendor,url,price,currency\nexample-motors-demo-900,Shop,https://shop.example/p,24.5,EUR\n";
    let offers = import_offers(
        csv,
        &OfferImportSpec {
            source_id: SourceId::new("example-motor").unwrap(),
            observed_at: date("2026-10-12"),
            delimiter: ',',
        },
    )
    .unwrap();
    db.offers = offers;
    assert!(db.validate().is_ok());
    assert!(!db.validate_for_shipping().is_ok());
}

#[test]
fn json_round_trip_is_lossless_and_revisions_must_not_overlap() {
    let db = motor_db();
    let again = Database::from_json(&db.to_json_pretty().unwrap()).unwrap();
    assert_eq!(again, db);
    let mut dup = db.clone();
    let mut second = dup.components[0].clone();
    second.valid_from = date("2026-11-01");
    dup.components.push(second);
    assert!(dup.validate().error_text().contains("overlap"));
}

#[test]
fn overlay_closes_replaces_and_adds() {
    let base = motor_db();
    let mut user = Database::new();
    user.sources.push(source(
        "my-bench",
        Redistribution::UserOwned,
        SourceKind::User,
    ));
    let mut revised = base.components[0].clone();
    revised.valid_from = date("2026-12-01");
    revised.updated_at = date("2026-12-01");
    if let Body::Motor(motor) = &mut revised.body {
        let rm = motor.rm_ohm.as_mut().unwrap();
        rm.value = 0.1;
        rm.prov.source_id = SourceId::new("my-bench").unwrap();
        rm.prov.method = Method::Measured;
    }
    user.components.push(revised);
    let (merged, report) = overlay::apply(&base, user).unwrap();
    assert_eq!(report.closed.len(), 1);
    let id = ComponentId::new("example-motors-demo-900").unwrap();
    let old = merged.component_at(&id, &date("2026-10-20")).unwrap();
    let new = merged.component_at(&id, &date("2026-12-02")).unwrap();
    assert_ne!(old.valid_from, new.valid_from);
    assert!(
        !merged.validate_for_shipping().is_ok(),
        "user data must not ship"
    );
    // The base is untouched.
    assert!(base.components[0].valid_to.is_none());
}

#[test]
fn prop_map_import_converts_rpm_and_validates() {
    let csv = "J CT CP eta\n0.0 0.12 0.05 0\n0.4 0.09 0.05 0.72\n";
    let set = import_prop_perf(
        csv,
        &PerfImportSpec {
            prop_id: ComponentId::new("p").unwrap(),
            source_id: SourceId::new("s").unwrap(),
            kind: PerfKind::Computed,
            retrieved_at: None,
            delimiter: ' ',
            rpm: Some(6000.0),
        },
    )
    .unwrap();
    assert_eq!(set.rows.len(), 2);
    assert!((set.rows[1].omega_rad_s - 6000.0 * std::f64::consts::TAU / 60.0).abs() < 1e-9);
    assert_eq!(set.rows[1].eta, Some(0.72));
}
