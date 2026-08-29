// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fail-closed regression checks for the Q-0007 open-reference boundary.
//!
//! The benchmark manifest is intentionally not a product input.  These checks
//! make that boundary executable: a future edit cannot silently turn the
//! provenance record into an aircraft-specific validation fixture.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use serde_json::Value;

const MANIFEST: &str = include_str!("../../../docs/benchmarks/q0007_open_benchmarks.json");

const REQUIRED_COVERAGE_KEYS: [&str; 4] = [
    "geometry",
    "engine_deck",
    "component_mass_and_cg",
    "mission_and_operational_rules",
];

#[test]
fn q0007_manifest_is_fail_closed_for_aircraft_specific_claims() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("Q-0007 manifest parses");

    assert_eq!(manifest["schema_version"], "q0007-open-benchmarks-2");
    assert_eq!(manifest["package_status"], "additive_provenance_only");
    assert_eq!(manifest["not_product_truth"], true);
    assert_eq!(manifest["not_used_by_product"], true);
    assert_eq!(
        manifest["availability_gate"]["status"],
        "blocked_by_evidence"
    );
    assert_eq!(
        manifest["availability_gate"]["product_integration_allowed"],
        false
    );
    assert_eq!(
        manifest["availability_gate"]["aircraft_specific_physical_claims_allowed"],
        false
    );
    assert_eq!(
        manifest["availability_gate"]["compatible_complete_package_found"],
        false
    );
    assert_eq!(
        manifest["availability_gate"]["owner_authorization_required"],
        true
    );

    let required_domains = manifest["availability_gate"]["required_domains"]
        .as_array()
        .expect("required Q-0007 domains");
    let domain_ids: Vec<&str> = required_domains
        .iter()
        .map(|domain| {
            assert_eq!(domain["status"], "missing");
            assert!(
                domain["required_evidence"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty()),
                "each blocked domain must state the irreducible evidence requirement"
            );
            domain["id"].as_str().expect("domain id")
        })
        .collect();
    assert_eq!(
        domain_ids,
        vec![
            "geometry",
            "installed_engine_deck",
            "component_mass_and_cg",
            "mission_and_operational_rules",
            "independent_validation",
        ]
    );

    for forbidden in [
        "product_preset_substitution",
        "cross_source_aircraft_composition",
        "installed_engine_deck_promotion",
        "aircraft_specific_mass_or_cg_claim",
        "aircraft_or_operator_mission_claim",
        "certification_or_validation_claim",
    ] {
        assert!(
            manifest["availability_gate"]["forbidden_scope"]
                .as_array()
                .expect("forbidden scope")
                .iter()
                .any(|value| value == forbidden),
            "Q-0007 gate must forbid {forbidden}"
        );
    }

    let benchmarks = manifest["benchmarks"]
        .as_array()
        .expect("benchmark records");
    assert!(!benchmarks.is_empty());
    for benchmark in benchmarks {
        let id = benchmark["id"].as_str().expect("benchmark id");
        let coverage = benchmark["coverage"].as_object().expect("coverage map");
        for key in REQUIRED_COVERAGE_KEYS {
            let status = coverage
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{id} must declare coverage for {key}"));
            assert!(
                !status.contains("complete") && !status.contains("aircraft_truth"),
                "{id} cannot advertise complete aircraft truth for {key}: {status}"
            );
        }

        assert!(
            benchmark["limitations"]
                .as_array()
                .is_some_and(|limitations| !limitations.is_empty()),
            "{id} must retain explicit limitations"
        );
        assert!(
            benchmark["license_and_distribution"]["classification"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
            "{id} must state its license/distribution classification"
        );
        assert!(
            benchmark["license_and_distribution"]["terms_url"]
                .as_str()
                .is_some_and(|url| url.starts_with("https://")),
            "{id} must point at HTTPS license terms"
        );
    }
}
