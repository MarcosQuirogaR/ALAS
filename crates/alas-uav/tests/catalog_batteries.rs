// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap or expect in this test target is the assertion reporting
// malformed evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Battery evidence-gate tests for the embedded UAV catalogue.

use alas_uav::catalog::ComponentKind;
use alas_uav::{
    full_catalog, has_reviewed_quote, is_analysis_ready, multi_source_catalog,
    optimization_catalog, reviewed_quote,
};

const MULTI_SOURCE_MANIFEST: &str = include_str!("../data/multi_source_manifest.json");

#[test]
fn every_complete_battery_has_a_dated_quote_and_each_gap_is_explicit() {
    let manifest: serde_json::Value =
        serde_json::from_str(MULTI_SOURCE_MANIFEST).expect("multi-source manifest");
    let ineligible = manifest["battery_analysis"]["ineligible"]
        .as_array()
        .expect("battery ineligibility list");
    let ineligible_ids: std::collections::BTreeSet<&str> = ineligible
        .iter()
        .map(|entry| entry["component_id"].as_str().expect("battery gap id"))
        .collect();
    assert_eq!(
        ineligible_ids,
        ["tattu-nmc-811-16000-6s", "tattu-nmc-811-22000-6s"]
            .into_iter()
            .collect()
    );

    let selectable_ids: std::collections::BTreeSet<&str> = manifest["battery_analysis"]
        ["selectable_ids"]
        .as_array()
        .expect("selectable battery id list")
        .iter()
        .map(|entry| entry.as_str().expect("selectable battery id"))
        .collect();
    let expected_multi_source_ids = [
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
    .collect();
    assert_eq!(selectable_ids, expected_multi_source_ids);

    let multi_source = multi_source_catalog().expect("multi-source catalogue");
    let actual_multi_source_ids: std::collections::BTreeSet<&str> = multi_source
        .records
        .iter()
        .filter(|record| {
            matches!(record.kind, ComponentKind::Battery(_))
                && is_analysis_ready(record, 6.0)
                && has_reviewed_quote(&record.id)
        })
        .map(|record| record.id.as_str())
        .collect();
    assert_eq!(actual_multi_source_ids, selectable_ids);

    let catalog = full_catalog().expect("full catalogue");
    let selectable = optimization_catalog().expect("optimization catalogue");
    for record in catalog
        .records
        .iter()
        .filter(|record| matches!(record.kind, ComponentKind::Battery(_)))
    {
        let ComponentKind::Battery(spec) = &record.kind else {
            unreachable!("the filter keeps only battery records");
        };
        let complete = spec.series_cells.is_some()
            && spec.nominal_voltage_v.is_some()
            && spec.capacity_ah.is_some()
            && spec.discharge_rating_c.is_some()
            && spec.mass_kg.is_some()
            && spec.dimensions.is_some()
            && spec.connector.is_some();
        if complete {
            let cells = spec.series_cells.expect("complete battery cell count");
            let nominal_voltage = spec.nominal_voltage_v.expect("complete battery voltage");
            assert!(
                (nominal_voltage - f64::from(cells) * 3.7).abs() < 1.0e-12,
                "battery '{}' has an inconsistent nominal voltage",
                record.id
            );
            assert!(spec.capacity_ah.expect("complete battery capacity") > 0.0);
            assert!(spec.discharge_rating_c.expect("complete battery C rating") > 0.0);
            assert!(spec.mass_kg.expect("complete battery mass") > 0.0);
            assert!(record.provenance.source_url.starts_with("https://"));
            assert!(!record.provenance.source_title.trim().is_empty());
            assert!(
                reviewed_quote(&record.id).is_some(),
                "complete battery '{}' lacks a dated price quote",
                record.id
            );
            assert!(!ineligible_ids.contains(record.id.as_str()));
        } else {
            assert!(
                ineligible_ids.contains(record.id.as_str()),
                "battery '{}' has missing fields but no documented gap",
                record.id
            );
            assert!(
                selectable.get(&record.id).is_none(),
                "ineligible battery '{}' entered the selectable catalogue",
                record.id
            );
        }
    }

    let ComponentKind::Motor(motor) = &catalog
        .get("tmotor-at2814-900kv")
        .expect("default motor record")
        .kind
    else {
        panic!("default motor id selected a different component family");
    };
    let ComponentKind::Esc(esc) = &catalog
        .get("hobbywing-skywalker-30a-v2-mini")
        .expect("default ESC record")
        .kind
    else {
        panic!("default ESC id selected a different component family");
    };
    for cells in [3_u16, 4_u16] {
        let mut compatible = false;
        for id in &selectable_ids {
            let record = catalog.get(id).expect("manifest battery in full catalogue");
            let ComponentKind::Battery(spec) = &record.kind else {
                panic!("manifest battery id selected a different component family");
            };
            if spec.series_cells != Some(cells) {
                continue;
            }
            compatible = true;
            assert!(
                motor.min_series_cells.expect("motor minimum cells") <= cells
                    && cells <= motor.max_series_cells.expect("motor maximum cells"),
                "{}S pack '{}' is outside the default motor cell range",
                cells,
                id
            );
            assert!(
                esc.min_series_cells.expect("ESC minimum cells") <= cells
                    && cells <= esc.max_series_cells.expect("ESC maximum cells"),
                "{}S pack '{}' is outside the default ESC cell range",
                cells,
                id
            );
            assert!(
                spec.rated_discharge_current_a()
                    .expect("battery nameplate current")
                    >= esc.continuous_current_a.expect("ESC continuous current"),
                "{}S pack '{}' does not cover the default ESC continuous current",
                cells,
                id
            );
            assert!(
                reviewed_quote(id).is_some(),
                "battery '{}' has no price quote",
                id
            );
        }
        assert!(compatible, "no selectable {cells}S pack is available");
    }
}
