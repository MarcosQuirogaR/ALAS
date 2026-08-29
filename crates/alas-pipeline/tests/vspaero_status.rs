// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! VSPAERO absence remains explicit on the normal product path.

use std::fs;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions, VspaeroAnalysisStatus};

#[test]
fn configured_solver_without_an_openvsp_mesh_cannot_fall_back_to_internal_aerodynamics() {
    let output = std::env::temp_dir().join(format!(
        "alas-vspaero-status-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
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
    let environment = RunEnvironment {
        vspaero_exe: Some(output.join("vspaero.exe")),
        ..RunEnvironment::default()
    };

    let pipeline = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("VSPAERO absence pipeline: {error}"));
    let result = pipeline
        .vspaero_result
        .unwrap_or_else(|| panic!("output-enabled pipeline must publish VSPAERO status"));
    assert_eq!(result.status, VspaeroAnalysisStatus::GeometryUnavailable);
    assert!(result.polar.is_none());
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("did not produce")));

    fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}
