// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in product-path evidence against an installed OpenVSP runtime.

use std::fs;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, OpenVspExportStatus, PipelineOptions};

#[test]
#[ignore = "requires ALAS_OPENVSP_EXE to name an installed vspscript executable"]
fn installed_openvsp_materializes_and_validates_the_native_project() {
    let executable = required_file("ALAS_OPENVSP_EXE");
    let retained = std::env::var_os("ALAS_OPENVSP_OUTPUT").is_some();
    let output = std::env::var_os("ALAS_OPENVSP_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("alas-installed-openvsp-{}", std::process::id()))
        });
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
        openvsp_exe: Some(executable.clone()),
        ..RunEnvironment::default()
    };
    let result = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("installed OpenVSP pipeline: {error}"));
    let export = result
        .openvsp_export
        .unwrap_or_else(|| panic!("pipeline did not publish OpenVSP evidence"));

    assert_eq!(
        export.status,
        OpenVspExportStatus::Vsp3Materialized,
        "{:?}",
        export.runtime_error
    );
    assert_eq!(
        export.runtime_executable.as_deref(),
        Some(executable.as_path())
    );
    let native = fs::read_to_string(&export.vsp3_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", export.vsp3_path.display()));
    assert!(native.starts_with("<?xml"));
    assert!(native.contains("<Vsp_Geometry>"));
    assert!(native.contains("<Name>Main Wing</Name>"));
    assert!(native.contains("<Name>Fuselage</Name>"));
    assert!(export
        .vspaero_geometry_path
        .metadata()
        .is_ok_and(|meta| meta.len() > 100));
    assert!(export
        .vspaero_geometry_path
        .with_extension("vkey")
        .is_file());
    let stdout_path = export
        .runtime_stdout_path
        .unwrap_or_else(|| panic!("runtime stdout path is retained"));
    let stdout = fs::read_to_string(&stdout_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", stdout_path.display()));
    assert!(stdout.contains("ALAS_OPENVSP_EXPORT_COMPLETE"));

    if !retained {
        fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
    }
}

fn required_file(variable: &str) -> PathBuf {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} to the installed executable"));
    assert!(
        path.is_file(),
        "{variable} is not a file: {}",
        path.display()
    );
    path
}
