// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Produces a deterministic, self-describing audit bundle for every real-aircraft preset.

// This audit command reports progress and its retained artifact locations.
#![allow(clippy::print_stdout)]

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use alas_acceptance::{format_matrix_json, run_acceptance_matrix};
use alas_config::presets;
use alas_config::AlasConfig;

const SEED: u64 = 42;

fn main() -> io::Result<()> {
    let output_dir = parse_output_dir(env::args().skip(1))?;
    fs::create_dir_all(output_dir.join("configs"))?;

    let report = run_acceptance_matrix();
    fs::write(
        output_dir.join("model_results.json"),
        format_matrix_json(&report).map_err(io::Error::other)?,
    )?;

    let mut preset_settings = Vec::new();
    for name in presets::available()
        .into_iter()
        .filter(|name| *name != "AVE")
    {
        let preset = presets::get(name).map_err(io::Error::other)?;
        let config = config_for_preset(preset);
        let slug = slug(name);
        fs::write(
            output_dir.join("configs").join(format!("{slug}.json")),
            serde_json::to_string_pretty(&config).map_err(io::Error::other)?,
        )?;
        preset_settings.push(reference_record(preset, &config, &slug));
    }

    let commit = command_output("git", &["rev-parse", "HEAD"]);
    let dirty = !command_output("git", &["status", "--porcelain"]).is_empty();
    let settings = serde_json::json!({
        "artifact_contract": "public_correlation_audit_v1",
        "generated_by": "cargo run --profile test -p alas-acceptance --bin correlation_audit -- --output-dir <directory>",
        "repository_commit": commit,
        "worktree_dirty": dirty,
        "global_run_settings": {
            "optimizer_enabled": false,
            "baseline_comparison_enabled": true,
            "parallel_pipeline": true,
            "aerodynamic_solver": "default product solver mode",
            "optimization_solver": "default product solver mode",
            "seed": SEED,
            "external_nastran_required": false,
            "mission_interpretation": "interactive route diagnostic, not a manufacturer design-range mission"
        },
        "presets": preset_settings,
    });
    fs::write(
        output_dir.join("audit_settings.json"),
        serde_json::to_string_pretty(&settings).map_err(io::Error::other)?,
    )?;
    fs::write(
        output_dir.join("REPLICATION.md"),
        replication_markdown(&output_dir, &settings),
    )?;

    println!("Audit bundle written to {}", output_dir.display());
    Ok(())
}

/// Select `preset` exactly as the program does.
///
/// Reassembling the configuration field-by-field here used to drop the cabin
/// seed and the engine binding, so the audit measured a preset the product
/// never evaluates. Going through the selection boundary is what keeps this
/// bundle a correlation artifact rather than a description of the harness.
fn config_for_preset(preset: &alas_config::AircraftPreset) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
        .unwrap_or_else(|error| panic!("selecting preset {}: {error}", preset.name));
    config.optimizer.solver.seed = Some(SEED as i64);
    config
}

fn reference_record(
    preset: &alas_config::AircraftPreset,
    config: &AlasConfig,
    slug: &str,
) -> serde_json::Value {
    serde_json::json!({
        "preset": preset.name,
        "display_name": preset.display_name,
        "identity": {
            "model": preset.identity.model,
            "weight_variant": preset.identity.weight_variant,
            "engine_model": preset.identity.engine_model,
            "modification_state": preset.identity.modification_state,
            "tank_configuration": preset.identity.tank_configuration,
        },
        "design_vector": preset.design_vector,
        "effective_config_file": format!("configs/{slug}.json"),
        "optimizer_method_saved_but_not_run": config.optimizer.solver.method,
        "public_reference": {
            "mtow_kg": preset.reference.mtow_kg,
            "mlw_kg": preset.reference.mlw_kg,
            "mzfw_kg": preset.reference.mzfw_kg,
            "oew_kg": preset.reference.oew_kg,
            "usable_fuel_volume_l": preset.reference.usable_fuel_volume_l,
            "usable_fuel_mass_kg": preset.reference.usable_fuel_mass_kg,
            "reference_wing_area_m2": preset.reference.reference_wing_area_m2,
            "planning_seats": preset.reference.planning_seats,
            "certified_max_seats": preset.reference.certified_max_seats,
            "sources": preset.reference.sources,
        }
    })
}

fn replication_markdown(output_dir: &Path, settings: &serde_json::Value) -> String {
    let commit = settings["repository_commit"]
        .as_str()
        .unwrap_or("unavailable");
    let dirty = settings["worktree_dirty"].as_bool().unwrap_or(true);
    format!(
        "# All-real-preset correlation audit replication\n\n\
This directory is a first-hand ALAS model artifact, not certification evidence. \
It evaluates every registered real-aircraft preset with optimization disabled so the \
result measures preset/model correlation rather than optimizer behavior.\n\n\
## Repository state\n\n\
- Commit: `{commit}`\n\
- Worktree dirty when generated: `{dirty}`\n\
- Random seed: `{SEED}`\n\
- Optimizer: disabled\n\
- Baseline full analysis: enabled\n\
- Pipeline parallelism: enabled\n\
- External NASTRAN: not required; analytical wingbox results are retained\n\
- Mission: current interactive diagnostic route, not a manufacturer payload-range mission\n\n\
## Reproduce the bundle\n\n\
```powershell\n\
$env:PATH = \"$env:USERPROFILE\\.cargo\\bin;$env:PATH\"\n\
Set-Location C:\\Proyectos\\ALAS-rust\n\
cargo run --profile test -p alas-acceptance --bin correlation_audit -- `\n\
  --output-dir outputs\\all_preset_correlation_20260824\n\
```\n\n\
The exact effective configuration for each aircraft is under `configs/`. \
`audit_settings.json` records the variant identity, source list, seed, and public \
reference values. `model_results.json` records built geometry, polar and trim, \
mass and CG, payload assumptions, internal wingbox sizing, analytical deflection, \
stress, modes, mission diagnostics, and physical findings.\n\n\
Generated directory: `{}`\n",
        output_dir.display()
    )
}

fn command_output(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn slug(name: &str) -> String {
    name.to_ascii_lowercase().replace(['-', ' '], "_")
}

fn parse_output_dir(arguments: impl Iterator<Item = String>) -> io::Result<PathBuf> {
    let values = arguments.collect::<Vec<_>>();
    match values.as_slice() {
        [flag, path] if flag == "--output-dir" => Ok(PathBuf::from(path)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: correlation_audit --output-dir <directory>",
        )),
    }
}
