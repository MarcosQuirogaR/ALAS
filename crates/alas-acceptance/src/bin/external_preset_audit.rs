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

use alas_aero::mses::{MsesPointClass, MsesStatus};
use alas_config::presets;
use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_pipeline::{
    AvlAnalysisStatus, DesignPipeline, FlowUnsteadyAnalysisStatus, OpenVspExportStatus,
    PipelineOptions, VspaeroAnalysisStatus,
};
use serde_json::{json, Value};

fn main() -> io::Result<()> {
    let (output_dir, strict, only, selection) = parse_args(env::args().skip(1))?;
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
        if !only.is_empty() && !only.iter().any(|name| name == preset_name) {
            continue;
        }
        eprintln!("[external audit] {preset_name}");
        rows.push(evaluate_preset(
            preset_name,
            &output_dir,
            &locator,
            &preferences,
            &selection,
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

const USAGE: &str = "usage: external_preset_audit [--strict] [--sequential]      [--preset <name>]... [--tool openvsp|vspaero|avl|mses|flowunsteady|nastran95]... [--set dotted.path=json]... --output-dir <directory>";

/// Which external tools run and whether they run one after another.
#[derive(Default)]
struct ToolSelection {
    /// Empty means every tool.
    tools: Vec<String>,
    /// Run the downstream stages one at a time (bounds concurrent processes).
    sequential: bool,
    /// `dotted.path=json` overrides applied on top of the preset (diagnostic
    /// experiments, for example `geometry.empennage.hstab_z_m=1.5`).
    overrides: Vec<(String, serde_json::Value)>,
}

impl ToolSelection {
    fn wants(&self, tool: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|name| name == tool)
    }

    /// Whether `tool` was named explicitly. `nastran95` runs only when asked
    /// for: it needs `ALAS_NASTRAN95_DIR` and is much slower than the others.
    fn named(&self, tool: &str) -> bool {
        self.tools.iter().any(|name| name == tool)
    }
}

fn parse_args(
    arguments: impl Iterator<Item = String>,
) -> io::Result<(PathBuf, bool, Vec<String>, ToolSelection)> {
    let mut output_dir = None;
    let mut strict = false;
    let mut only = Vec::new();
    let mut selection = ToolSelection::default();
    let values = arguments.collect::<Vec<_>>();
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--strict" => strict = true,
            "--sequential" => selection.sequential = true,
            "--set" => {
                index += 1;
                let parsed = values.get(index).and_then(|text| {
                    let (path, value) = text.split_once('=')?;
                    let value = serde_json::from_str(value)
                        .unwrap_or_else(|_| Value::String(value.to_owned()));
                    Some((path.to_owned(), value))
                });
                let Some(entry) = parsed else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--set requires dotted.path=value",
                    ));
                };
                selection.overrides.push(entry);
            }
            "--tool" => {
                index += 1;
                let Some(name) = values.get(index) else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--tool requires a tool name",
                    ));
                };
                selection.tools.push(name.clone());
            }
            "--preset" => {
                index += 1;
                let Some(name) = values.get(index) else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--preset requires a preset name",
                    ));
                };
                only.push(name.clone());
            }
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
                return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
            }
        }
        index += 1;
    }
    output_dir.map_or_else(
        || Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE)),
        |path| Ok((path, strict, only, selection)),
    )
}

fn evaluate_preset(
    name: &str,
    output_root: &Path,
    locator: &ToolLocator,
    preferences: &alas_exec::ToolPreferences,
    selection: &ToolSelection,
) -> Value {
    let result_dir = output_root.join(name);
    let Some(preset) = presets::get(name).ok() else {
        return failed_row(name, &result_dir, "registered preset lookup failed");
    };
    // Selected through the same boundary the program uses; a hand-built
    // configuration here silently drops the cabin seed and the engine binding.
    let mut document = json!({ "preset": preset.name });
    for (path, value) in &selection.overrides {
        let mut slot = &mut document;
        for key in path.split('.') {
            slot = &mut slot[key];
        }
        *slot = value.clone();
    }
    let mut config = match AlasConfig::from_value(&document) {
        Ok(config) => config,
        Err(error) => return failed_row(name, &result_dir, &error.to_string()),
    };
    config.mission.enabled = false;
    config.structures.enabled = selection.named("nastran95");
    if config.structures.enabled {
        // Static NASTRAN-95 only: the SOL 103 search and the vibration solves
        // are separate defects with their own checks.
        config.structures.run_nastran = true;
        config.structures.run_sol_static = true;
        config.structures.run_sol_modes = false;
        config.structures.run_sol_vibration_sine = false;
        config.structures.run_sol_vibration_random = false;
        config.structures.run_patran_export = false;
    }
    config.mses.enabled = selection.wants("mses");
    config.downstream.vspaero = selection.wants("vspaero");
    config.downstream.openvsp = selection.wants("openvsp") || selection.wants("vspaero");
    config.downstream.avl = selection.wants("avl");
    config.downstream.flowunsteady = selection.wants("flowunsteady");

    let mut environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );
    if selection.named("nastran95") {
        // No MSC Nastran: the structural stage then falls back to the local
        // NASTRAN-95 solver as its primary solve.
        environment.nastran_exe = None;
        environment.nastran_solver = None;
        environment.patran_exe = None;
    }
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: !selection.sequential,
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
                "points": value.polar.as_ref().map(|polar| polar.points.len()),
                "outside_validity_alpha_deg": value.outside_validity_alpha_deg,
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
                "takeoff_vs_vlm": avl_deltas(value),
                "linear_cruise": value.linear_cruise.as_ref().map(|cruise| json!({
                    "status": cruise.status.as_str(),
                    "error": cruise.error,
                    "phase": cruise.comparison_reference.as_ref().map(|r| r.phase.clone()),
                    "mach": cruise.comparison_reference.as_ref().map(|r| r.mach),
                    "vs_vlm": avl_deltas(cruise),
                })),
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
                "design_alpha_deg": value.design_alpha_deg,
                "design_point_class": value.design_point_class().map(|class| class.as_str()),
                "outside_validity_domain_points": value
                    .count_class(MsesPointClass::OutsideValidityDomain),
                "solver_failure_points": value.count_class(MsesPointClass::SolverFailure),
                "diagnostics": relative(&result_dir, Path::new("mses/polar_diagnostics.json")),
                "sections": value.span_sections.iter().map(|section| json!({
                    "label": section.label,
                    "status": section.polar.status.as_str(),
                    "chord_m": section.chord_m,
                    "requested_points": section.polar.requested_alpha_count,
                    "converged_points": section.polar.converged_alpha_count,
                    "design_point_class":
                        section.polar.design_point_class().map(|class| class.as_str()),
                    "outside_validity_domain_points":
                        section.polar.count_class(MsesPointClass::OutsideValidityDomain),
                    "solver_failure_points":
                        section.polar.count_class(MsesPointClass::SolverFailure),
                })).collect::<Vec<_>>(),
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
    let nastran95 = result
        .structural_result
        .as_ref()
        .and_then(|structural| structural.nastran.as_ref())
        .map(|solve| &solve.static_solve)
        .map_or_else(
            || json!({"status": "not_published"}),
            |solve| {
                json!({
                    "status": solve.status.as_str(),
                    "error": solve.error,
                    "load_cases": solve.tip_deflection_m.len(),
                    "tip_deflection_m": solve.tip_deflection_m.iter().collect::<Vec<_>>(),
                    "assessed_von_mises_max_pa":
                        solve.assessed_von_mises_max_pa.iter().collect::<Vec<_>>(),
                    "root_von_mises_max_pa":
                        solve.root_von_mises_max_pa.iter().collect::<Vec<_>>(),
                })
            },
        );
    let nastran95_ok = !selection.named("nastran95") || nastran95["status"] == "ok";
    let external_acceptance = nastran95_ok
        && (!selection.wants("openvsp")
        || result
            .openvsp_export
            .as_ref()
            .is_some_and(|value| value.status == OpenVspExportStatus::Vsp3Materialized))
        && (!selection.wants("vspaero")
            || result
                .vspaero_result
                .as_ref()
                .is_some_and(|value| value.status == VspaeroAnalysisStatus::CompletedComparable))
        && (!selection.wants("avl") || result.avl_result.as_ref().is_some_and(|value| {
            value.status == AvlAnalysisStatus::CompletedComparable
                && value.linear_cruise.as_ref().is_some_and(|cruise| {
                    cruise.status == AvlAnalysisStatus::CompletedComparable
                })
        }))
        // Root, kink and tip must all be Ok. outside_validity_domain is a
        // separate, reported outcome and is never an acceptance.
        && (!selection.wants("mses") || result.mses_result.as_ref().is_some_and(|value| {
            value.status == MsesStatus::Ok
                && !value.span_sections.is_empty()
                && value
                    .span_sections
                    .iter()
                    .all(|section| section.polar.status == MsesStatus::Ok)
        }))
        && (!selection.wants("flowunsteady")
            || result.flowunsteady_result.as_ref().is_some_and(|value| {
                value.status == FlowUnsteadyAnalysisStatus::CompletedComparable
            }));
    json!({
        "preset": name,
        "external_acceptance": external_acceptance,
        "openvsp": openvsp,
        "vspaero": vspaero,
        "avl": avl,
        "mses": mses,
        "flowunsteady": flowunsteady,
        "nastran95": nastran95,
        "vspaero_points": result.vspaero_result.as_ref().and_then(|v| v.polar.as_ref()).map(|p| p.points.iter().map(|q| json!({"alpha_deg": q.alpha_deg, "cl": q.lift_coefficient, "cdi": q.induced_drag_coefficient, "e": q.span_efficiency})).collect::<Vec<_>>()),
        "native_polar": result.optimized_report.as_ref().or(result.baseline_analysis.as_ref()).map(|r| json!({"alpha_deg": r.polar.alpha_deg, "geometric_alpha_deg": r.polar.geometric_alpha_deg, "cl": r.polar.cl, "cd_induced": r.polar.cd_induced})),
        "native_beam": result.structural_result.as_ref().and_then(|s| s.analysis.as_ref()).map(|a| a.load_cases.iter().map(|c| json!({"case": c.name, "tip_deflection_m": c.tip_deflection_m})).collect::<Vec<_>>()),
        "structures": if selection.named("nastran95") {
            "nastran95_static_only"
        } else {
            "disabled_by_audit_scope"
        },
        "output_dir": result_dir.display().to_string(),
    })
}

/// AVL minus the ALAS VLM at the same geometric angles, for the comparison
/// reference the result was classified against: the largest absolute CL and
/// Cm differences and the CL difference relative to the VLM CL where
/// |CL| >= 0.1.
fn avl_deltas(result: &alas_pipeline::AvlAnalysisResult) -> Value {
    let (Some(polar), Some(reference)) = (&result.polar, &result.comparison_reference) else {
        return Value::Null;
    };
    let vlm = &reference.vlm_polar;
    if polar.points.len() != vlm.cl.len() || polar.points.len() != vlm.cm.len() {
        return Value::Null;
    }
    let (mut dcl, mut dcm, mut rel) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (index, point) in polar.points.iter().enumerate() {
        dcl = dcl.max((point.lift_coefficient - vlm.cl[index]).abs());
        dcm = dcm.max((point.pitching_moment_coefficient - vlm.cm[index]).abs());
        if vlm.cl[index].abs() >= 0.1 {
            rel = rel.max(((point.lift_coefficient - vlm.cl[index]) / vlm.cl[index]).abs());
        }
    }
    json!({
        "phase": reference.phase,
        "mach": reference.mach,
        "points": polar.points.len(),
        "max_abs_dcl": dcl,
        "max_abs_dcm": dcm,
        "max_rel_dcl_where_cl_ge_0p1": rel,
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
