// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproduce the external-tool preflight of one registered preset, on the
//! exact boundary `external_preset_audit` uses, without running the other
//! seven.
//!
//! The CFD/external worker recorded the ATR 72-600 as blocked out of every
//! external tool by a mass-model error raised before geometry export:
//!
//! ```text
//! mass-coordinate error: FLOPS transport mass method is unverified:
//! turboprop_propeller_geometry (...); turboprop_nacelle_architecture (...)
//! ```
//!
//! That is a *mass* blocker on an aerodynamic path, so it has to be closed
//! and re-measured from the mass lane. This probe runs one preset through
//! `DesignPipeline` with the same option set and the same disabled mission
//! and structural stages, and records whether the pipeline reaches the
//! geometry export at all and which artifacts it materialises. It is a
//! preflight, not an acceptance gate: tool convergence and comparability
//! remain the external worker's to judge.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-pipeline --example atr72_external_preflight -- \
//!     <output-dir> [preset-name]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]

use alas_config::{presets, AlasConfig};
use alas_exec::ToolLocator;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use serde_json::{json, Value};
use std::{error::Error, fs, path::PathBuf, time::Instant};

fn artifact(root: &std::path::Path, path: &std::path::Path) -> Value {
    let exists = path.exists();
    let bytes = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    json!({
        "path": path.strip_prefix(root).unwrap_or(path).display().to_string(),
        "exists": exists,
        "bytes": bytes,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let output_dir = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: atr72_external_preflight <output-dir> [preset]")?;
    let preset_name = arguments.next().unwrap_or_else(|| "ATR72-600".to_owned());
    fs::create_dir_all(&output_dir)?;

    let preset = presets::get(&preset_name).map_err(std::io::Error::other)?;
    // The same seam `external_preset_audit` uses; a hand-built configuration
    // would drop the cabin seed, the engine binding and the declared FLOPS
    // architecture this probe exists to exercise.
    let mut config = AlasConfig::from_value(&json!({ "preset": preset.name }))?;
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = true;

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();
    let environment = locator.resolve_environment(
        std::path::Path::new(&config.mses.mses_dir),
        std::path::Path::new(&config.structures.nastran_exe_path),
        std::path::Path::new(&config.structures.patran_exe_path),
        std::path::Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        std::path::Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(output_dir.clone()),
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let started = Instant::now();
    let outcome = DesignPipeline::new(config).run(&options, &environment);
    let elapsed_s = started.elapsed().as_secs_f64();

    let record = match outcome {
        Err(error) => json!({
            "preset": preset.name,
            "pipeline": "failed",
            "error": error,
            "elapsed_s": elapsed_s,
        }),
        Ok(result) => {
            let openvsp = result.openvsp_export.as_ref().map_or_else(
                || json!({"status": "not_published"}),
                |value| {
                    json!({
                        "status": value.status.as_str(),
                        "error": value.runtime_error,
                        "vsp3": artifact(&output_dir, &value.vsp3_path),
                        "vspgeom": artifact(&output_dir, &value.vspaero_geometry_path),
                    })
                },
            );
            let vspaero = result.vspaero_result.as_ref().map_or_else(
                || json!({"status": "not_published"}),
                |value| {
                    json!({
                        "status": value.status.as_str(),
                        "error": value.error,
                        "polar": artifact(&output_dir, &value.polar_path),
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
                        "geometry": artifact(&output_dir, &value.geometry_path),
                    })
                },
            );
            let mses = result.mses_result.as_ref().map_or_else(
                || json!({"status": "not_published"}),
                |value| json!({"status": format!("{:?}", value.status)}),
            );
            json!({
                "preset": preset.name,
                "pipeline": "completed",
                "elapsed_s": elapsed_s,
                "openvsp": openvsp,
                "vspaero": vspaero,
                "avl": avl,
                "mses": mses,
            })
        }
    };

    let path = output_dir.join("external_preflight.json");
    fs::write(&path, serde_json::to_vec_pretty(&record)?)?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    println!("{}", path.display());
    Ok(())
}
