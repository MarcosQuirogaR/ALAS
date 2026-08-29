// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pipeline-stage ownership of the CPACS 3.5 export artifact.

use std::fs;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};

#[test]
fn output_enabled_pipeline_writes_the_cpacs35_interchange_artifact() {
    let output =
        std::env::temp_dir().join(format!("alas-pipeline-cpacs-export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: Some(output.clone()),
        save_plots: false,
        seed: None,
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| panic!("CPACS output pipeline: {error}"));
    let export = result
        .cpacs_export
        .unwrap_or_else(|| panic!("pipeline did not publish a CPACS export"));
    let document = fs::read_to_string(&export.path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", export.path.display()));

    assert_eq!(export.cpacs_version, "3.5");
    assert!(export.wing_count >= 3);
    assert!(export.engine_position_count >= 2);
    assert!(document.contains("<cpacsVersion>3.5</cpacsVersion>"));
    assert!(document.contains("<analyses>"));
    assert!(document.contains("<aeroPerformance>"));
    assert!(document.contains("<weightAndBalance>"));
    assert!(document.contains("<thrust00>467000</thrust00>"));
    assert!(!document.contains("<massBreakdown>"));
    assert!(!document.contains("<toolspecific>"));
    let adapter_manifest = output.join("cpacs/adapter_manifest.json");
    assert!(adapter_manifest.is_file());
    let manifest = output.join("cpacs/run_manifest.json");
    assert!(manifest.is_file());
    let manifest_text = fs::read_to_string(&manifest)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", manifest.display()));
    assert!(manifest_text.contains("\"cpacs_export\": \"completed\""));
    assert!(manifest_text.contains("\"cpacs_adapter_manifest\": \"completed\""));

    let _ = fs::remove_dir_all(output);
}
