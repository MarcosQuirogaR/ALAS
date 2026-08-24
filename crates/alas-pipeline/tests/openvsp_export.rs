// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product-path evidence for the OpenVSP geometry interchange artifact.

use std::fs;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{
    materialize_openvsp_project, DesignPipeline, OpenVspExportStatus, PipelineOptions,
};

#[test]
fn an_output_run_writes_a_supported_openvsp_script_without_claiming_runtime_success() {
    let output = std::env::temp_dir().join(format!("alas-openvsp-product-{}", std::process::id()));
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
        .unwrap_or_else(|error| panic!("output pipeline: {error}"));
    let export = result
        .openvsp_export
        .unwrap_or_else(|| panic!("OpenVSP artifact missing"));

    assert_eq!(
        export.status,
        OpenVspExportStatus::ScriptWrittenRuntimeUnverified
    );
    assert!(export.script_path.is_file());
    assert!(!export.vsp3_path.exists());
    assert_eq!(export.component_count, 6);
    assert!(export.wheel_count >= 3);
    assert!(!export.unsupported.is_empty());

    let script = fs::read_to_string(&export.script_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", export.script_path.display()));
    assert!(script.contains("ClearVSPModel"));
    assert!(script.contains("int main()"));
    assert!(script.contains("return 0;"));
    assert!(script.contains("return 1;"));
    assert!(script.contains("SetAirfoilPnts"));
    assert!(script.contains("SetXSecWidthHeight"));
    assert!(script.contains("SetGeomName( wheel_0, \"NLG wheel 1\" )"));
    assert!(script.contains("WriteVSPFile"));
    assert!(!script.contains("NaN"));

    fs::write(&export.vsp3_path, b"stale result")
        .unwrap_or_else(|error| panic!("write stale fixture: {error}"));
    let attempted = materialize_openvsp_project(export, &output.join("missing-vspscript.exe"), 1.0);
    assert_eq!(attempted.status, OpenVspExportStatus::RuntimeLaunchFailed);
    assert!(attempted
        .runtime_error
        .as_deref()
        .is_some_and(|error| error.contains("failed to launch")));
    assert!(!attempted.vsp3_path.exists());

    fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}
