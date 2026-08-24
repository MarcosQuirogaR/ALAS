// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Evidence and import-contract tests for the embedded UAV catalogue.

use alas_uav::catalog::ComponentKind;
use alas_uav::{
    apc_performance_catalog, full_catalog, has_reviewed_quote, is_analysis_ready,
    multi_source_catalog, optimization_catalog, reviewed_catalog, reviewed_quote, seed_catalog,
    Catalog, Currency,
};

const REVIEW_MANIFEST: &str = include_str!("../data/rc_innovations_review_manifest.json");
const MULTI_SOURCE_MANIFEST: &str = include_str!("../data/multi_source_manifest.json");

#[test]
fn the_reviewed_seed_catalogue_retains_sources_and_absent_vendor_fields() {
    let catalog = match seed_catalog() {
        Ok(catalog) => catalog,
        Err(error) => panic!("seed catalogue failed validation: {error}"),
    };
    assert_eq!(catalog.schema_version, 1);
    assert_eq!(catalog.records.len(), 7);
    assert!(catalog.records.iter().all(|record| {
        record
            .provenance
            .source_url
            .starts_with("https://rc-innovations.es/")
    }));

    let battery = match &catalog
        .get("gens-ace-b-45c-1800-5s1p")
        .expect("reviewed battery record")
        .kind
    {
        ComponentKind::Battery(spec) => spec,
        _ => panic!("battery id selected a different component family"),
    };
    assert!((battery.nominal_energy_wh().expect("energy") - 33.3).abs() < 1.0e-12);
    assert_eq!(battery.rated_discharge_current_a(), Some(81.0));

    let servo = match &catalog
        .get("kst-ds315mg-v8")
        .expect("reviewed servo record")
        .kind
    {
        ComponentKind::Servo(spec) => spec,
        _ => panic!("servo id selected a different component family"),
    };
    assert_eq!(servo.stall_current_at_voltage(6.0), None);
    assert!(
        (servo
            .stall_torque_at_voltage(6.6)
            .expect("interpolated torque")
            - 0.357942725)
            .abs()
            < 1.0e-12
    );

    let material = match &catalog
        .get("carbon-sheet-3k-500x400x1p5-matte")
        .expect("reviewed material record")
        .kind
    {
        ComponentKind::MaterialStock(spec) => spec,
        _ => panic!("material id selected a different component family"),
    };
    assert_eq!(material.youngs_modulus_pa, None);
    assert_eq!(material.allowable_stress_pa, None);
}

#[test]
fn normalized_catalogues_round_trip_through_the_public_import_seam() {
    let catalog = match seed_catalog() {
        Ok(catalog) => catalog,
        Err(error) => panic!("seed catalogue failed validation: {error}"),
    };
    let json = serde_json::to_string(catalog).expect("catalogue serializes");
    let reparsed = Catalog::from_reader(json.as_bytes()).expect("catalogue reparses");
    assert_eq!(&reparsed, catalog);
}

#[test]
fn the_expanded_catalogue_covers_the_fixed_wing_procurement_categories() {
    let catalog = reviewed_catalog().expect("reviewed catalogue");
    assert_eq!(catalog.records.len(), 64);
    let mut counts = std::collections::BTreeMap::new();
    for record in &catalog.records {
        let family = match &record.kind {
            ComponentKind::Battery(_) => "battery",
            ComponentKind::Motor(_) => "motor",
            ComponentKind::Esc(_) => "esc",
            ComponentKind::Servo(_) => "servo",
            ComponentKind::Propeller(_) => "propeller",
            ComponentKind::MaterialStock(_) => "material_stock",
            ComponentKind::Receiver(_) => "receiver",
            ComponentKind::Electronics(_) => "electronics",
            ComponentKind::LandingGear(_) => "landing_gear",
        };
        *counts.entry(family).or_insert(0usize) += 1;
    }
    assert_eq!(counts["battery"], 3);
    assert_eq!(counts["motor"], 8);
    assert_eq!(counts["esc"], 7);
    assert_eq!(counts["servo"], 13);
    assert_eq!(counts["propeller"], 13);
    assert_eq!(counts["material_stock"], 7);
    assert_eq!(counts["receiver"], 3);
    assert_eq!(counts["electronics"], 4);
    assert_eq!(counts["landing_gear"], 6);

    let title_only_servo = catalog.get("kst-x20-4208").expect("title-reviewed servo");
    let ComponentKind::Servo(spec) = &title_only_servo.kind else {
        panic!("servo id selected a different component family");
    };
    assert_eq!(spec.mass_kg, Some(0.08));
    assert!(spec.operating_points.is_empty());
    assert!(title_only_servo
        .provenance
        .transformations
        .iter()
        .any(|note| {
            note.contains("torque, speed, current, voltage range, and dimensions remain absent")
        }));

    let gear = catalog
        .get("tarot-25kg-90-retract")
        .expect("landing gear record");
    let ComponentKind::LandingGear(spec) = &gear.kind else {
        panic!("landing gear id selected a different component family");
    };
    assert_eq!(spec.max_aircraft_mass_kg, None);
}

#[test]
fn the_review_manifest_counts_the_embedded_catalogue_without_entering_physics_data() {
    let manifest: serde_json::Value =
        serde_json::from_str(REVIEW_MANIFEST).expect("review manifest");
    let catalog = reviewed_catalog().expect("reviewed catalogue");
    assert_eq!(manifest["reviewed_on_utc"], "2026-08-17");
    assert_eq!(manifest["record_count"], catalog.records.len());
    assert_eq!(
        manifest["local_live_collection"]["completed"],
        serde_json::Value::Bool(false)
    );

    let mut actual = std::collections::BTreeMap::new();
    for record in &catalog.records {
        let family = match &record.kind {
            ComponentKind::Battery(_) => "battery",
            ComponentKind::Motor(_) => "motor",
            ComponentKind::Esc(_) => "esc",
            ComponentKind::Servo(_) => "servo",
            ComponentKind::Propeller(_) => "propeller",
            ComponentKind::MaterialStock(_) => "material_stock",
            ComponentKind::Receiver(_) => "receiver",
            ComponentKind::Electronics(_) => "electronics",
            ComponentKind::LandingGear(_) => "landing_gear",
        };
        *actual.entry(family).or_insert(0_u64) += 1;
    }
    for (family, count) in actual {
        assert_eq!(manifest["category_counts"][family], count);
    }
}

#[test]
fn invalid_provenance_and_duplicate_ids_are_rejected() {
    let invalid_source = r#"{
        "schema_version": 1,
        "records": [{
            "id": "unsafe-source",
            "manufacturer": "Example",
            "model": "Example",
            "kind": "propeller",
            "spec": {
                "diameter_m": null,
                "pitch_m": null,
                "blade_count": null,
                "mass_kg": null,
                "bore_diameter_m": null,
                "electric_compatible": null
            },
            "provenance": {
                "publisher": "Example",
                "source_url": "http://example.invalid/product",
                "source_title": "Example",
                "transformations": []
            }
        }]
    }"#;
    let error = Catalog::from_json(invalid_source).expect_err("HTTP provenance must fail");
    assert!(error.to_string().contains("HTTPS provenance"));

    let catalog = seed_catalog().expect("seed catalogue");
    let mut duplicated = catalog.clone();
    duplicated.records.push(duplicated.records[0].clone());
    let json = serde_json::to_string(&duplicated).expect("duplicate catalogue serializes");
    let error = Catalog::from_json(&json).expect_err("duplicate id must fail");
    assert!(error.to_string().contains("duplicate component id"));
}

#[test]
fn the_multi_source_catalogue_keeps_manufacturer_evidence_and_unknowns_explicit() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");
    assert_eq!(catalog.records.len(), 120);
    assert!(catalog.records.iter().all(|record| {
        matches!(
            record.provenance.publisher.as_str(),
            "T-MOTOR"
                | "HOBBYWING"
                | "Holybro"
                | "APC Propellers"
                | "Easy Composites"
                | "Spektrum"
                | "Horizon Hobby"
                | "SunnySky USA"
                | "Gens ace"
                | "Emax"
                | "MATEKSYS"
                | "Tattu"
                | "RadioMaster"
                | "Unmanned Tech Shop"
                | "Hitec"
                | "FrSky"
                | "Sika"
        )
    }));

    let motor = catalog.get("tmotor-at8025-kv160").expect("T-MOTOR record");
    let ComponentKind::Motor(spec) = &motor.kind else {
        panic!("T-MOTOR id selected a different component family");
    };
    assert_eq!(spec.max_static_thrust_n, None);
    assert_eq!(spec.max_power_w, Some(5800.0));

    let controller = catalog
        .get("holybro-pixhawk-6x-rev3-module")
        .expect("Holybro record");
    let ComponentKind::Electronics(spec) = &controller.kind else {
        panic!("Holybro id selected a different component family");
    };
    assert_eq!(spec.mass_kg, Some(0.023));
    assert_eq!(spec.max_current_a, Some(1.5));

    let sheet = catalog
        .get("easy-composites-cfs-ri-1mm-250x225")
        .expect("Easy Composites record");
    let ComponentKind::MaterialStock(spec) = &sheet.kind else {
        panic!("material id selected a different component family");
    };
    assert_eq!(spec.allowable_stress_pa, None);
    assert!((spec.mass_kg.expect("derived sheet mass") - 0.07425).abs() < 1.0e-12);

    let sunnysky = catalog
        .get("sunnysky-x2212-v3-kv1400")
        .expect("SunnySky record");
    let ComponentKind::Motor(spec) = &sunnysky.kind else {
        panic!("SunnySky id selected a different component family");
    };
    assert_eq!(spec.max_static_thrust_n, None);
    assert_eq!(spec.max_current_a, Some(37.0));

    let gens = catalog
        .get("gens-ace-soaring-2200mah-3s-20c")
        .expect("Gens ace record");
    let ComponentKind::Battery(spec) = &gens.kind else {
        panic!("Gens ace id selected a different component family");
    };
    assert_eq!(spec.rated_discharge_current_a(), Some(44.0));

    let competition_motor = catalog
        .get("tmotor-at2814-900kv")
        .expect("source-reviewed ACC motor");
    let ComponentKind::Motor(spec) = &competition_motor.kind else {
        panic!("competition motor id selected a different component family");
    };
    assert_eq!(spec.kv_rpm_per_v, Some(900.0));
    assert_eq!(spec.max_current_a, Some(45.0));

    let reviewed_pack = catalog
        .get("unmannedtech-gensace-gtech-2200-3s-45c-xt60")
        .expect("manually reviewed retailer battery");
    let ComponentKind::Battery(spec) = &reviewed_pack.kind else {
        panic!("reviewed battery id selected a different component family");
    };
    assert!((spec.nominal_energy_wh().expect("nameplate energy") - 24.42).abs() < 1.0e-12);
    assert!(
        (spec
            .rated_discharge_current_a()
            .expect("nameplate discharge current")
            - 99.0)
            .abs()
            < 1.0e-12
    );

    let emax = catalog.get("emax-eco-2306-kv1700").expect("Emax record");
    let ComponentKind::Motor(spec) = &emax.kind else {
        panic!("Emax id selected a different component family");
    };
    assert_eq!(spec.max_current_a, None);

    let skywalker = catalog
        .get("hobbywing-skywalker-50a-6s-v2")
        .expect("source-reviewed Skywalker ESC");
    let ComponentKind::Esc(spec) = &skywalker.kind else {
        panic!("Skywalker id selected a different component family");
    };
    assert_eq!(spec.continuous_current_a, Some(50.0));
    assert_eq!(spec.bec_continuous_current_a, Some(6.0));

    let receiver = catalog
        .get("radiomaster-er6-elrs-pwm")
        .expect("source-reviewed RadioMaster receiver");
    let ComponentKind::Receiver(spec) = &receiver.kind else {
        panic!("RadioMaster id selected a different component family");
    };
    assert_eq!(spec.channel_count, Some(6));
    assert_eq!(spec.telemetry, Some(true));

    let competition_receiver = catalog
        .get("radiomaster-er8gv-elrs-pwm-vario")
        .expect("source-reviewed RadioMaster vario receiver");
    let ComponentKind::Receiver(spec) = &competition_receiver.kind else {
        panic!("RadioMaster vario id selected a different component family");
    };
    assert_eq!(spec.channel_count, Some(8));
    assert_eq!(spec.mass_kg, Some(0.012));

    let high_torque_servo = catalog
        .get("spektrum-a6300")
        .expect("source-reviewed Spektrum high-torque servo");
    let ComponentKind::Servo(spec) = &high_torque_servo.kind else {
        panic!("Spektrum id selected a different component family");
    };
    assert_eq!(spec.stall_current_at_voltage(8.4), None);
    assert_eq!(spec.stall_torque_at_voltage(8.4), Some(3.746_140_3));

    let high_voltage_pack = catalog
        .get("tattu-plus-22000-12s-25c")
        .expect("source-reviewed Tattu 12S battery");
    let ComponentKind::Battery(spec) = &high_voltage_pack.kind else {
        panic!("Tattu id selected a different component family");
    };
    assert_eq!(spec.rated_discharge_current_a(), Some(550.0));

    let apc = catalog
        .get("apc-17x8e")
        .expect("source-reviewed APC electric propeller");
    let ComponentKind::Propeller(spec) = &apc.kind else {
        panic!("APC id selected a different component family");
    };
    assert_eq!(spec.diameter_m, Some(0.4318));
    assert_eq!(spec.pitch_m, Some(0.2032));
    assert_eq!(spec.mass_kg, None);

    let cruise_prop = catalog
        .get("apc-15x10e")
        .expect("source-reviewed higher-pitch APC electric propeller");
    let ComponentKind::Propeller(spec) = &cruise_prop.kind else {
        panic!("APC id selected a different component family");
    };
    assert_eq!(spec.diameter_m, Some(0.381));
    assert_eq!(spec.pitch_m, Some(0.254));
    assert_eq!(spec.mass_kg, None);
}

#[test]
fn the_full_catalogue_merges_sources_without_duplicate_ids() {
    let catalog = full_catalog().expect("full catalogue");
    assert_eq!(catalog.records.len(), 605);
    assert!(catalog.get("gens-ace-b-45c-1800-5s1p").is_some());
    assert!(catalog.get("tmotor-at1050-kv90").is_some());
    assert!(catalog.get("mateksys-f405-wing-v2").is_some());
    assert!(catalog.get("tattu-gtech-22000-4s").is_some());
    let selectable = optimization_catalog().expect("optimization catalogue");
    assert!(selectable.records.len() < catalog.records.len());
    assert!(selectable
        .records
        .iter()
        .all(|record| { is_analysis_ready(record, 6.0) && has_reviewed_quote(&record.id) }));
    let selectable_battery_ids: std::collections::BTreeSet<&str> = selectable
        .records
        .iter()
        .filter_map(|record| match record.kind {
            ComponentKind::Battery(_) => Some(record.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        selectable_battery_ids,
        [
            "gens-ace-b-45c-1800-5s1p",
            "gens-ace-b-45c-1800-3s1p",
            "gens-ace-6800-3s-120c-ec5",
            "spektrum-smart-6s-5000-50c",
            "spektrum-smart-4s-5000-50c-hardcase",
            "spektrum-smart-3s-3200-30c",
            "gens-ace-soaring-2200mah-3s-20c",
            "tattu-plus-22000-6s-25c",
            "tattu-plus-22000-12s-25c",
            "tattu-gtech-22000-4s",
            "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
            "unmannedtech-cnhl-black-2000-3s-100c-xt60",
            "tattu-gtech-10000-6s-30c-ec5",
            "tattu-gtech-16000-6s-30c-xt90s",
            "tattu-16000-6s-30c-as150",
            "tattu-plus-compact-10000-12s-15c-as150u",
            "tattu-gtech-10000-12s-30c-as150u",
            "tattu-pro-22000-14s-25c-as150uf",
            "tattu-semisolid-30000-6s-3c-xt90s",
            "gens-ace-550mah-3s-80c-xt30",
            "gens-ace-gtech-2200-3s-45c-deans",
        ]
        .into_iter()
        .collect()
    );
    assert!(selectable.get("tattu-nmc-811-16000-6s").is_none());
    assert!(selectable.get("tattu-nmc-811-22000-6s").is_none());
}

#[test]
fn the_apc_performance_catalogue_adds_every_nonduplicated_source_table() {
    let catalog = apc_performance_catalog().expect("generated APC performance catalogue");
    assert_eq!(catalog.records.len(), 421);
    let propeller = catalog
        .get("apc-13x4-5ep-f2b")
        .expect("F2B variant must retain its distinct source-table identity");
    let ComponentKind::Propeller(spec) = &propeller.kind else {
        panic!("APC performance record selected a different component family");
    };
    assert_eq!(spec.diameter_m, Some(13.0 * 0.0254));
    assert_eq!(spec.pitch_m, Some(4.5 * 0.0254));
    assert_eq!(spec.mass_kg, None);
}

#[test]
fn the_multi_source_manifest_matches_every_embedded_record() {
    let manifest: serde_json::Value =
        serde_json::from_str(MULTI_SOURCE_MANIFEST).expect("multi-source manifest");
    let catalog = multi_source_catalog().expect("multi-source catalogue");
    assert_eq!(manifest["record_count"], catalog.records.len());
    let mut categories = std::collections::BTreeMap::new();
    let mut publishers = std::collections::BTreeMap::new();
    for record in &catalog.records {
        let category = match &record.kind {
            ComponentKind::Battery(_) => "battery",
            ComponentKind::Motor(_) => "motor",
            ComponentKind::Esc(_) => "esc",
            ComponentKind::Servo(_) => "servo",
            ComponentKind::Propeller(_) => "propeller",
            ComponentKind::MaterialStock(_) => "material_stock",
            ComponentKind::Receiver(_) => "receiver",
            ComponentKind::Electronics(_) => "electronics",
            ComponentKind::LandingGear(_) => "landing_gear",
        };
        *categories.entry(category).or_insert(0_u64) += 1;
        *publishers
            .entry(record.provenance.publisher.as_str())
            .or_insert(0_u64) += 1;
    }
    for (category, count) in categories {
        assert_eq!(manifest["category_counts"][category], count);
    }
    for (publisher, count) in publishers {
        assert_eq!(manifest["publisher_counts"][publisher], count);
    }
}

#[test]
fn dated_procurement_reviews_match_the_visible_price_quote_boundary() {
    let manifest: serde_json::Value =
        serde_json::from_str(MULTI_SOURCE_MANIFEST).expect("multi-source manifest");
    let reviews = manifest["procurement_reviews"]
        .as_array()
        .expect("procurement reviews are an array");
    assert!(!reviews.is_empty());
    let catalog = full_catalog().expect("full catalogue");
    let mut reviewed_ids = std::collections::BTreeSet::new();

    for review in reviews {
        let id = review["component_id"]
            .as_str()
            .expect("review component id");
        assert!(reviewed_ids.insert(id), "duplicate quote for '{id}'");
        assert!(
            catalog.get(id).is_some(),
            "quote has no catalogue record: {id}"
        );
        let quote = reviewed_quote(id).expect("manifest review has a code quote");
        let expected_currency = match review["currency"].as_str().expect("review currency") {
            "EUR" => Currency::Eur,
            "GBP" => Currency::Gbp,
            "USD" => Currency::Usd,
            currency => panic!("unsupported manifest currency: {currency}"),
        };
        assert_eq!(quote.currency, expected_currency);
        assert_eq!(
            quote.unit_price_minor,
            review["unit_price_minor"]
                .as_u64()
                .expect("review minor-unit price")
        );
        assert_eq!(
            quote.source_url,
            review["source_url"].as_str().expect("review source URL")
        );
        assert_eq!(
            quote.source_title,
            review["source_title"]
                .as_str()
                .expect("review source title")
        );
        assert_eq!(
            quote.observed_on_utc,
            review["reviewed_on_utc"].as_str().expect("review date")
        );
        assert!(quote.source_url.starts_with("https://"));
        assert!(!quote.source_title.trim().is_empty());
        assert!(
            is_utc_date(&quote.observed_on_utc),
            "invalid quote date: {}",
            quote.observed_on_utc
        );
    }
}

fn is_utc_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

#[test]
fn the_powertrain_manifest_names_only_complete_priced_selectable_records() {
    let manifest: serde_json::Value =
        serde_json::from_str(MULTI_SOURCE_MANIFEST).expect("multi-source manifest");
    let catalog = full_catalog().expect("full catalogue");
    let selectable = manifest["powertrain_analysis"]["selectable"]
        .as_object()
        .expect("powertrain selectable families");
    let mut selected_ids = std::collections::BTreeSet::new();
    for ids in selectable.values() {
        for value in ids.as_array().expect("selectable ids") {
            let id = value.as_str().expect("selectable component id");
            assert!(selected_ids.insert(id), "duplicate selectable id '{id}'");
            let record = catalog.get(id).expect("selectable record exists");
            assert!(
                is_analysis_ready(record, 5.0),
                "selectable powertrain record '{id}' has missing analysis evidence"
            );
            assert!(
                has_reviewed_quote(id),
                "selectable powertrain record '{id}' lacks a dated source quote"
            );
        }
    }
    assert_eq!(
        selected_ids,
        [
            "apc-12x6e",
            "apc-8x6e",
            "apc-9x6e",
            "apc-10x5e",
            "hobbywing-skywalker-30a-v2-mini",
            "hobbywing-skywalker-15a-v2",
            "hobbywing-skywalker-40a-v2-na",
            "hobbywing-skywalker-60a-v2-na",
            "hobbywing-skywalker-80a-v2-na",
            "hobbywing-platinum-hv-180a-v5",
            "hobbywing-platinum-hv-260a-v5",
            "tmotor-at2814-900kv",
            "hobbywing-skywalker-2814-sl-kv1000",
            "hobbywing-skywalker-2814-sl-kv1250",
            "hobbywing-skywalker-2814-sl-kv1400",
            "tmotor-ax435-a-kv220",
            "tmotor-ax435-a-kv250",
            "tmotor-ax435-b-kv220",
            "tmotor-ax435-b-kv250",
            "tmotor-ax525-a-kv250",
            "tmotor-ax525-b-kv250",
            "tmotor-ax530-a-kv260",
            "tmotor-ax530-b-kv260"
        ]
        .into_iter()
        .collect()
    );
}
