// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing masses the structures figure reads are the masses the findings
//! panel compares: the native beam primary structure, the FE deck primary
//! material and the FLOPS complete wing, all for both semi-wings in kg.

// A test asserts on values it built, so a failed unwrap is the assertion
// failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::structural::run_structural_analysis;
use alas_pipeline::RunEnvironment;

#[test]
fn the_structural_result_carries_the_masses_the_findings_compare() {
    for name in ["A320-200", "AVE"] {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = presets::get(name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, false)
            .unwrap();
        let result = run_structural_analysis(&config, &report, None, &RunEnvironment::default());
        assert_eq!(result.status, "ok", "{name}: {:?}", result.error);
        let masses = result.wing_mass.expect("an ok result carries wing masses");
        let sizing = result.sizing.as_ref().unwrap();

        assert_eq!(masses.native_primary_kg, 2.0 * sizing.total_mass_kg);
        assert_eq!(
            masses.flops_complete_wing_kg,
            report.component_masses["Wing"]
        );
        assert_eq!(masses.flops_complete_wing_kg, result.torenbeek_wing_mass_kg);

        // The findings panel takes the same quantities from the native
        // feasibility assessment of the same design.
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let assessment =
            alas_opt::mdo::structural_feasibility::assess_candidate(&config, &design, &plane)
                .unwrap();
        let relative = |a: f64, b: f64| ((a - b) / b).abs();
        assert!(
            relative(masses.native_primary_kg, assessment.primary_mass_kg) < 1e-6,
            "{name}: native {} vs findings {}",
            masses.native_primary_kg,
            assessment.primary_mass_kg
        );
        let fe = masses.fe_primary_kg.expect("the deck mass is available");
        let findings_fe = assessment.mesh_primary_mass_kg.unwrap();
        assert!(
            relative(fe, findings_fe) < 1e-6,
            "{name}: FE {fe} vs findings {findings_fe}"
        );
    }
}
