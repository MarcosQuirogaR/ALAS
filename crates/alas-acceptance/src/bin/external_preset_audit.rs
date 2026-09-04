// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run the installed external adapters against every registered aircraft
//! preset.  This is deliberately separate from the fast model-acceptance
//! matrix: a finite in-process ALAS result is not evidence that a native hook
//! launched, produced fresh files, parsed them, or passed its comparison gate.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use alas_aero::mses::MsesStatus;
use alas_config::presets;
use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_pipeline::{
    AvlAnalysisStatus, DesignPipeline, FlowUnsteadyAnalysisStatus, OpenVspExportStatus,
    PipelineOptions, VspaeroAnalysisStatus,
};
use serde_json::{json, Value};

fn main() -> io::Result<()> {
    let (output_dir, strict) = parse_args(env::args().skip(1))?;
    if output_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite retained external audit at {}",
                output_dir.display()
            ),
        ));
    }
    fs::create_dir_all(&output_dir)?;

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();
    let mut rows = Vec::new();
    for preset_name in presets::available() {
        eprintln!("[external audit] {preset_name}");
        rows.push(evaluate_preset(
            preset_name,
            &output_dir,
            &locator,
            &preferences,
        ));
    }

    let all_passed = rows
        .iter()
        .all(|row| row["external_acceptance"] == Value::Bool(true));
    let document = json!({
        "scope": "OpenVSP/VSPAERO/AVL/MSES/FLOWUnsteady against every registered preset; mission and structural stages are disabled to isolate aerodynamic external hooks",
        "required_statuses": {
            "openvsp": "vsp3_materialized",
            "vspaero": "completed_comparable",
            "avl": "completed_comparable",
            "mses": "ok",
            "flowunsteady": "completed_comparable"
        },
        "external_acceptance": all_passed,
        "presets": rows,
    });
    fs::write(
        output_dir.join("external_preset_matrix.json"),
        serde_json::to_vec_pretty(&document).map_err(io::Error::other)?,
    )?;
    fs::write(
        output_dir.join("external_preset_matrix.txt"),
        format_report(&document),
    )?;
    println!(
        "External preset audit: {} ({})",
        if all_passed { "PASSED" } else { "NOT PASSED" },
        output_dir.display()
    );
    if strict && !all_passed {
        std::process::exit(2);
    }
    Ok(())
}

fn parse_args(arguments: impl Iterator<Item = String>) -> io::Result<(PathBuf, bool)> {
    let mut output_dir = None;
    let mut strict = false;
    let values = arguments.collect::<Vec<_>>();
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--strict" => strict = true,
            "--output-dir" => {
                index += 1;
                let Some(path) = values.get(index) else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--output-dir requires a path",
                    ));
                };
                output_dir = Some(PathBuf::from(path));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: external_preset_audit [--strict] --output-dir <directory>",
                ));
            }
        }
        index += 1;
    }
    output_dir.map_or_else(
        || {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: external_preset_audit [--strict] --output-dir <directory>",
            ))
        },
        |path| Ok((path, strict)),
    )
}

fn evaluate_preset(
    name: &str,
    output_root: &Path,
    locator: &ToolLocator,
    preferences: &alas_exec::ToolPreferences,
) -> Value {
    let result_dir = output_root.join(name);
    let Some(preset) = presets::get(name).ok() else {
        return failed_row(name, &result_dir, "registered preset lookup failed");
    };
    // Selected through the same boundary the program uses; a hand-built
    // configuration here silently drops the cabin seed and the engine binding.
    let mut config = match AlasConfig::from_value(&json!({ "preset": preset.name })) {
        Ok(config) => config,
        Err(error) => return failed_row(name, &result_dir, &error.to_string()),
    };
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = true;

    let environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(result_dir.clone()),
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = match DesignPipeline::new(config).run(&options, &environment) {
        Ok(result) => result,
        Err(error) => return failed_row(name, &result_dir, &error),
    };

    let openvsp = result.openvsp_export.as_ref().map_or_else(
        || json!({"status": "not_published"}),
        |value| {
            json!({
                "status": value.status.as_str(),
                "error": value.runtime_error,
                "vsp3": relative(&result_dir, &value.vsp3_path),
                "vspgeom": relative(&result_dir, &value.vspaero_geometry_path),
            })
        },
    );
    let vspaero = result.vspaero_result.as_ref().map_or_else(
        || json!({"status": "not_published"}),
        |value| {
            json!({
                "status": value.status.as_str(),
                "error": value.error,
                "polar": relative(&result_dir, &value.polar_path),
                "history": relative(&result_dir, &value.case_path.with_extension("history")),
            })
        },
    );
    let avl = result.avl_result.as_ref().map_or_else(
        || json!({"status": "not_published"}),
        |value| {
            json!({
                "status": value.status.as_str(),
                "error": value.error,
                "force_files": value.force_paths.len(),
                "geometry": relative(&result_dir, &value.geometry_path),
            })
        },
    );
    let mses = result.mses_result.as_ref().map_or_else(
        || json!({"status": "not_published"}),
        |value| {
            json!({
                "status": value.status.as_str(),
                "error": value.error,
                "requested_points": value.requested_alpha_count,
                "converged_points": value.converged_alpha_count,
                "diagnostics": relative(&result_dir, Path::new("mses/polar_diagnostics.json")),
            })
        },
    );
    let flowunsteady = result.flowunsteady_result.as_ref().map_or_else(
        || json!({"status": "not_published"}),
        |value| {
            json!({
                "status": value.status.as_str(),
                "error": value.error,
                "request": relative(&result_dir, &value.request_path),
            })
        },
    );
    let external_acceptance = result
        .openvsp_export
        .as_ref()
        .is_some_and(|value| value.status == OpenVspExportStatus::Vsp3Materialized)
        && result
            .vspaero_result
            .as_ref()
            .is_some_and(|value| value.status == VspaeroAnalysisStatus::CompletedComparable)
        && result
            .avl_result
            .as_ref()
            .is_some_and(|value| value.status == AvlAnalysisStatus::CompletedComparable)
        && result
            .mses_result
            .as_ref()
            .is_some_and(|value| value.status == MsesStatus::Ok)
        && result
            .flowunsteady_result
            .as_ref()
            .is_some_and(|value| value.status == FlowUnsteadyAnalysisStatus::CompletedComparable);
    json!({
        "preset": name,
        "external_acceptance": external_acceptance,
        "openvsp": openvsp,
        "vspaero": vspaero,
        "avl": avl,
        "mses": mses,
        "flowunsteady": flowunsteady,
        "structures": "disabled_by_audit_scope",
        "output_dir": result_dir.display().to_string(),
    })
}

fn failed_row(name: &str, result_dir: &Path, error: &str) -> Value {
    json!({
        "preset": name,
        "external_acceptance": false,
        "pipeline_error": error,
        "output_dir": result_dir.display().to_string(),
    })
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn format_report(document: &Value) -> String {
    let mut text = String::new();
    text.push_str("ALAS EXTERNAL PRESET AUDIT\n");
    text.push_str("Scope: OpenVSP, VSPAERO, AVL, MSES, and FLOWUnsteady; mission and structures disabled.\n\n");
    text.push_str("Preset     | OpenVSP | VSPAERO | AVL | MSES | FLOWUnsteady | Verdict\n");
    text.push_str("-----------+---------+---------+-----+------+--------------+--------\n");
    if let Some(rows) = document["presets"].as_array() {
        for row in rows {
            let name = row["preset"].as_str().unwrap_or("?");
            let status = |tool: &str| row[tool]["status"].as_str().unwrap_or("not_run");
            text.push_str(&format!(
                "{name:<10} | {:<7} | {:<7} | {:<3} | {:<4} | {:<12} | {}\n",
                status("openvsp"),
                status("vspaero"),
                status("avl"),
                status("mses"),
                status("flowunsteady"),
                if row["external_acceptance"] == Value::Bool(true) {
                    "PASS"
                } else {
                    "FAIL"
                }
            ));
        }
    }
    text.push_str(&format!(
        "\nExternal acceptance: {}\n",
        if document["external_acceptance"] == Value::Bool(true) {
            "PASSED"
        } else {
            "NOT PASSED"
        }
    ));
    text
}
