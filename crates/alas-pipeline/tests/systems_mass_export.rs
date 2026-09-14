// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product-path evidence for systems-method provenance in exported databases.

use std::fs;

use alas_config::{AlasConfig, MassArchitecture};
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};

#[test]
fn exported_compatibility_mass_groups_keep_fraction_provenance_and_absolute_scaling() {
    let output =
        std::env::temp_dir().join(format!("alas-systems-mass-export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);

    let mut config = AlasConfig::default();
    // This fixture audits the retained Torenbeek/fraction export. Make the
    // comparison architecture explicit now that pure FLOPS owns the default
    // production path.
    config.mass_model.mass_architecture = MassArchitecture::LegacyReferenceCompatibleComparison;
    config.mass_model.apply_architecture();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = false;
    let systems_fraction = config.mass_model.systems_mass_fraction;
    let furnishings_fraction = config.mass_model.furnishings_mass_fraction;
    let mtow_kg = config.requirements.mtow_kg;
    let result = DesignPipeline::new(config)
        .run(
            &PipelineOptions {
                optimize: false,
                compare_baseline: false,
                parallel: false,
                aerodynamic_solver: Default::default(),
                optimization_solver: Default::default(),
                output_dir: Some(output.clone()),
                save_plots: false,
                seed: None,
                quiet: true,
            },
            &RunEnvironment::default(),
        )
        .unwrap_or_else(|error| panic!("systems-mass export pipeline: {error}"));

    let database = result
        .design_database
        .unwrap_or_else(|| panic!("design database was not exported"));
    let weights = &database.weights;
    assert_eq!(
        weights["systems_mass_method"],
        "reference_compatible_fractions"
    );
    assert_eq!(weights["systems_mass_status"], "compatibility_only");

    let masses = weights["component_masses_kg"]
        .as_object()
        .unwrap_or_else(|| panic!("component mass export is not an object"));
    let systems_kg = masses["Systems"]
        .as_f64()
        .unwrap_or_else(|| panic!("Systems mass is not numeric"));
    let furnishings_kg = masses["Furnishings"]
        .as_f64()
        .unwrap_or_else(|| panic!("Furnishings mass is not numeric"));
    assert!((systems_kg - systems_fraction * mtow_kg).abs() < 1.0e-9);
    assert!((furnishings_kg - furnishings_fraction * mtow_kg).abs() < 1.0e-9);
    assert_ne!(systems_kg, furnishings_kg);
    assert_eq!(
        database.feasibility["fuel_loading"]["usable_capacity_evidence"],
        "geometry_estimate"
    );
    assert_eq!(
        database.feasibility["cruise_equilibrium"]["status"],
        "not_evaluated"
    );

    fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}
