// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap or expect in this test target is the assertion reporting
// malformed evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Focused checks for newly reviewed airframe, landing-gear, servo, and avionics evidence.

use alas_uav::catalog::ComponentKind;
use alas_uav::{has_reviewed_quote, is_analysis_ready, multi_source_catalog};

const MULTI_SOURCE_MANIFEST: &str = include_str!("../data/multi_source_manifest.json");

#[test]
fn primary_manual_evidence_is_normalized_without_filling_unpublished_dimensions() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");

    let m10 = catalog
        .get("mateksys-m10-l4-3100")
        .expect("M10-L4-3100 record");
    let ComponentKind::Electronics(m10_spec) = &m10.kind else {
        panic!("M10-L4-3100 id selected a different component family");
    };
    assert_eq!(m10_spec.mass_kg, Some(0.016));
    assert!(m10
        .provenance
        .transformations
        .iter()
        .any(|note| note.contains("published 16 g")));

    let eflg500 = catalog
        .get("eflite-eflg500-retract")
        .expect("EFLG500 record");
    let ComponentKind::LandingGear(eflg500_spec) = &eflg500.kind else {
        panic!("EFLG500 id selected a different component family");
    };
    assert_eq!(eflg500_spec.max_aircraft_mass_kg, Some(6.8));
    assert_eq!(eflg500_spec.dimensions, None);

    let eflg700 = catalog
        .get("eflite-eflg700-retract")
        .expect("EFLG700 record");
    let ComponentKind::LandingGear(eflg700_spec) = &eflg700.kind else {
        panic!("EFLG700 id selected a different component family");
    };
    assert_eq!(eflg700_spec.max_aircraft_mass_kg, Some(15.9));
    assert_eq!(eflg700_spec.dimensions, None);
}

#[test]
fn servo_current_evidence_stays_at_the_published_voltage_point() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");
    let servo = catalog.get("spektrum-a6380").expect("A6380 record");
    let ComponentKind::Servo(spec) = &servo.kind else {
        panic!("A6380 id selected a different component family");
    };

    assert_eq!(spec.stall_current_at_voltage(8.4), Some(2.5));
    assert_eq!(spec.stall_current_at_voltage(6.0), None);
    assert!(servo
        .provenance
        .transformations
        .iter()
        .any(|note| note.contains("no 6 V stall current is inferred")));
}

#[test]
fn carbon_stock_and_gear_geometry_gaps_remain_explicit() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");

    let sheet = catalog
        .get("easy-composites-cfs-ri-1mm-250x225")
        .expect("carbon sheet record");
    let ComponentKind::MaterialStock(sheet_spec) = &sheet.kind else {
        panic!("carbon sheet id selected a different component family");
    };
    assert!(sheet_spec.youngs_modulus_pa.is_some());
    assert_eq!(sheet_spec.allowable_stress_pa, None);

    for id in ["dubro-micro-profile-landing-gear", "eflite-eflg500-retract"] {
        let gear = catalog.get(id).expect("landing gear record");
        let ComponentKind::LandingGear(spec) = &gear.kind else {
            panic!("landing gear id selected a different component family");
        };
        assert_eq!(spec.dimensions, None);
    }
}

#[test]
fn robart_121_tailwheel_meets_the_complete_landing_gear_gate() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");
    let record = catalog
        .get("robart-121-retractable-tailwheel")
        .expect("Robart #121 record");
    let ComponentKind::LandingGear(spec) = &record.kind else {
        panic!("Robart #121 id selected a different component family");
    };

    assert_eq!(spec.form, "retract_tailwheel");
    assert_eq!(spec.mass_kg, Some(0.0226796185));
    assert_eq!(spec.max_aircraft_mass_kg, Some(4.5359237));
    assert_eq!(
        spec.dimensions,
        Some(alas_uav::catalog::Dimensions {
            length_m: 0.079375,
            width_m: 0.0365125,
            height_m: 0.0619125,
        })
    );
    assert!(is_analysis_ready(record, 6.0));
    assert!(has_reviewed_quote(&record.id));
    assert!(record
        .provenance
        .transformations
        .iter()
        .any(|note| note.contains("121.pdf") && note.contains("retracted assembly envelope")));

    let manifest: serde_json::Value =
        serde_json::from_str(MULTI_SOURCE_MANIFEST).expect("multi-source manifest");
    assert_eq!(
        manifest["landing_gear_analysis"]["selectable_ids"],
        serde_json::json!(["robart-121-retractable-tailwheel"])
    );
}

#[test]
fn newly_admitted_material_and_supporting_records_have_all_model_inputs_and_quotes() {
    let catalog = multi_source_catalog().expect("multi-source catalogue");

    for id in ["sika-carbodur-s512-1m", "sika-carbodur-s812-1m"] {
        let record = catalog.get(id).expect("Sika material record");
        assert!(
            is_analysis_ready(record, 6.0),
            "material '{id}' is incomplete"
        );
        assert!(
            has_reviewed_quote(id),
            "material '{id}' lacks a dated quote"
        );
    }

    for id in [
        "frsky-archer-plus-sr10-stabilized",
        "radiomaster-er6-elrs-pwm",
        "holybro-sik-v3-100mw-air-radio",
        "holybro-sik-v3-1w-air-radio",
        "holybro-micro-m9n-gps",
        "holybro-m10-gps-v2-standard",
        "holybro-pm08-can-14s-200a",
        "holybro-h-rtk-neo-f9p-uart",
    ] {
        let record = catalog.get(id).expect("supporting equipment record");
        assert!(
            is_analysis_ready(record, 6.0),
            "supporting component '{id}' is incomplete"
        );
        assert!(
            has_reviewed_quote(id),
            "supporting component '{id}' lacks a dated quote"
        );
    }

    let s512 = catalog.get("sika-carbodur-s512-1m").expect("S512 record");
    let ComponentKind::MaterialStock(spec) = &s512.kind else {
        panic!("S512 id selected a different component family");
    };
    assert_eq!(spec.youngs_modulus_pa, Some(160_000_000_000.0));
    assert_eq!(spec.allowable_stress_pa, Some(2_800_000_000.0));
}
