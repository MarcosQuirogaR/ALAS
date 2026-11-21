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

/// The preset pipeline runs on a 256 MiB stack (the GUI's analysis worker and
/// the benchmark use the same size): on the 1 MiB Windows main thread the
/// A320-200 run overflowed before its first result.
const AUDIT_STACK_BYTES: usize = 256 << 20;

fn main() -> io::Result<()> {
    std::thread::Builder::new()
        .name("external-audit".to_owned())
        .stack_size(AUDIT_STACK_BYTES)
        .spawn(run)?
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn run() -> io::Result<()> {
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
    let lfl_resolution = (
        config.analysis.fine_spanwise_resolution,
        config.analysis.fine_chordwise_resolution,
    );
    let wake_config = config.clone();
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
                "wake_model": value.wake_model(),
                "reduced_fidelity": value.is_reduced_fidelity(),
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
    let lfl = if selection.named("lfl") {
        result
            .optimized_report
            .as_ref()
            .or(result.baseline_analysis.as_ref())
            .map_or(Value::Null, |report| {
                let mut value = like_for_like(&report.airplane, lfl_resolution, &result_dir);
                value["tail_eta_study"] =
                    tail_eta_study(&report.airplane, &wake_config, lfl_resolution, &result_dir);
                value
            })
    } else {
        Value::Null
    };
    json!({
        "preset": name,
        "lfl": lfl,
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
        "points_detail": polar.points.iter().enumerate().map(|(index, point)| json!({
            "alpha_deg": point.alpha_deg,
            "cl_avl": point.lift_coefficient,
            "cl_vlm": vlm.cl[index],
            "cm_avl": point.pitching_moment_coefficient,
            "cm_vlm": vlm.cm[index],
            "cdi_avl": point.induced_drag_coefficient,
            "cdi_vlm": vlm.cd_induced.get(index),
            "e_avl": point.span_efficiency,
        })).collect::<Vec<_>>(),
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

/// Incompressible, lifting-surface-only ALAS VLM quantities for a like-for-like
/// comparison with AVL, plus the matching Mach-0 AVL deck (`deck_m0.avl`).
///
/// Per alpha in {1, 2, 3} deg: CL, Cm about `xyz_ref` (lattice only, no body
/// slope), Trefftz CDi without the installation term. At 2 deg also the strip
/// span load cl*c/cref of the main wing. The neutral point is
/// `x_ref - (dCm/dalpha)/(dCL/dalpha) * c_ref` from the 1 and 3 deg solves.
fn like_for_like(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    resolution: (i64, i64),
    result_dir: &Path,
) -> Value {
    // AVL has no tail dynamic-pressure ratio, so the like-for-like lattice
    // flies the tail at the free-stream dynamic pressure. `tail_eta_study`
    // reports the DATCOM 4.4.1 wake ratio and the superseded constant 0.78
    // (with eta = 0.78 the tail-on neutral point sat 5-13 % MAC forward of
    // AVL's, with eta = 1 within 0.5-3.3 %).
    let mut free_stream_tail = airplane.clone();
    free_stream_tail.set_horizontal_tail_dynamic_pressure_ratio(1.0);
    let airplane = &free_stream_tail;
    let mut wing_only = airplane.clone();
    wing_only.wings.retain(|wing| wing.name == "Main Wing");
    // Same planform, twist and incidence with a symmetric thin section:
    // isolates camber handling from twist handling.
    let mut flat = wing_only.clone();
    let foil = alas_geom::aircraft::airfoil::Airfoil::from_coordinates(
        "flat",
        vec![
            (1.0, 0.0),
            (0.5, 0.01),
            (0.0, 0.0),
            (0.5, -0.01),
            (1.0, 0.0),
        ],
    );
    for wing in &mut flat.wings {
        for xsec in &mut wing.xsecs {
            xsec.airfoil = foil.clone();
        }
    }
    // Untwisted rectangular wing (AR 40, chord 1 m) with the root section:
    // thin-airfoil theory gives CL = CLa * (alpha - alpha_L0) for any AR, so
    // CL(0)/CLa is an analytic check of each tool's camber handling.
    let make_rect = |root_airfoil: &alas_geom::aircraft::airfoil::Airfoil, deck: &str| {
        use alas_geom::aircraft::wing::{Wing, WingXSec};
        let mut plane = airplane.clone();
        plane.wings = vec![Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, root_airfoil.clone()),
                WingXSec::new([0.0, 20.0, 0.0], 1.0, 0.0, root_airfoil.clone()),
            ],
            true,
        )];
        plane.xyz_ref = [0.25, 0.0, 0.0];
        plane.s_ref = 40.0;
        plane.c_ref = 1.0;
        plane.b_ref = 40.0;
        let mut value = like_for_like_one(&plane, resolution, result_dir, deck);
        // Thin-airfoil alpha_L0 = -(1/pi) int_0^pi z'(x) (cos t - 1) dt of the
        // section's own mean camber line (Anderson, Fundamentals of
        // Aerodynamics, eq. 4.61), 4000 midpoint stations.
        let n = 4000_usize;
        let h = 1.0e-5;
        let mut sum = 0.0;
        for k in 0..n {
            let theta = (k as f64 + 0.5) * std::f64::consts::PI / n as f64;
            let x = 0.5 * (1.0 - theta.cos());
            let z = root_airfoil.local_camber(&[x - h, x + h]);
            sum +=
                (z[1] - z[0]) / (2.0 * h) * (theta.cos() - 1.0) * std::f64::consts::PI / n as f64;
        }
        value["thin_airfoil_alpha_l0_deg"] = json!(-(sum / std::f64::consts::PI).to_degrees());
        value
    };
    let rect = airplane
        .wings
        .first()
        .and_then(|wing| wing.xsecs.first())
        .map(|root| make_rect(&root.airfoil, "deck_rect_m0.avl"));
    let rect_naca = alas_geom::aircraft::airfoil::Airfoil::from_name("naca4412")
        .map(|foil| make_rect(&foil, "deck_rect_naca4412_m0.avl"));
    json!({
        "rect": rect,
        "rect_naca4412": rect_naca,
        "wing_flat": like_for_like_one(&flat, resolution, result_dir, "deck_flat_m0.avl"),
        "full": like_for_like_one(airplane, resolution, result_dir, "deck_m0.avl"),
        "wing_only": like_for_like_one(&wing_only, resolution, result_dir, "deck_wing_m0.avl"),
    })
}

/// Tail dynamic-pressure ratio study: the DATCOM 4.4.1 wake ratio at the
/// cruise state against the superseded constant 0.78 and the free stream, each
/// with the lattice-only neutral point and Cm_alpha (rad^-1).
fn tail_eta_study(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    config: &alas_config::AlasConfig,
    resolution: (i64, i64),
    result_dir: &Path,
) -> Value {
    let req = &config.requirements;
    let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * velocity * velocity;
    let cl = req.required_cruise_cl(q, airplane.s_ref);
    let sweep = alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(airplane, 25.0);
    let aero = alas_aero::analysis::AeroAnalysis::new(
        airplane,
        sweep,
        Some(config.geometry.clone()),
        Some(config.resolved_drag_model()),
        Some(config.analysis.clone()),
    );
    let wake = aero.horizontal_tail_wake(req.cruise_mach, req.cruise_altitude_m, cl);
    let mut rows = Vec::new();
    let mut etas = vec![("free_stream", 1.0), ("old_constant", 0.78)];
    if let Some(wake) = wake {
        etas.push(("datcom_wake", wake.ratio));
    }
    for (label, eta) in etas {
        let mut plane = airplane.clone();
        plane.set_horizontal_tail_dynamic_pressure_ratio(eta);
        let one = like_for_like_one(&plane, resolution, result_dir, "deck_eta.avl");
        rows.push(json!({
            "label": label, "eta": eta, "x_np_m": one["x_np_m"],
            "cm_alpha_per_rad": one["cm_alpha_per_rad"], "cl_alpha_per_rad": one["cl_alpha_per_rad"],
            "x_ref_m": one["x_ref_m"], "c_ref_m": one["c_ref_m"],
        }));
    }
    json!({
        "cruise_cl": cl,
        "wake": wake.map(|w| json!({
            "ratio": w.ratio, "wing_cd0": w.wing_cd0, "centre_line_deficit": w.centre_line_deficit,
            "half_width_m": w.half_width_m, "height_above_wake_m": w.height_above_wake_m,
            "arm_m": w.arm_m, "alpha_deg": w.alpha_deg,
            "centre_line_downwash_deg": w.centre_line_downwash_deg,
        })),
        "rows": rows,
    })
}

fn like_for_like_one(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    resolution: (i64, i64),
    result_dir: &Path,
    deck_name: &str,
) -> Value {
    use alas_aero::avl::{fit_vortex_budget, render_geometry, AvlDeckRequest};
    use alas_aero::operating_point::OperatingPoint;
    use alas_aero::vlm::VlmSystem;
    let to_usize = |v: i64| usize::try_from(v).unwrap_or(0);
    let (span_res, chord_res) = (to_usize(resolution.0).max(1), to_usize(resolution.1).max(1));
    let system = match VlmSystem::assemble(airplane, span_res, chord_res) {
        Ok(system) => system,
        Err(error) => return json!({"error": error.to_string()}),
    };
    let atmosphere = alas_atmo::Atmosphere::new(0.0);
    let mut points = Vec::new();
    let mut loads = Value::Null;
    for alpha in [1.0_f64, 2.0, 3.0] {
        let op = OperatingPoint::new(atmosphere, 60.0, alpha, 0.0, 0.0, 0.0, 0.0);
        let Ok(solved) = system.solve(&op) else {
            return json!({"error": "VLM solve failed"});
        };
        let cdi = system.trefftz_induced_drag_coefficient(&solved, &op);
        points.push(json!({
            "alpha_deg": alpha, "cl": solved.cl_lift, "cm": solved.cm_pitch,
            "cdi_trefftz_lattice": cdi,
        }));
        if (alpha - 2.0).abs() < 1.0e-9 {
            let q = op.dynamic_pressure();
            loads = json!(alas_aero::strips::wing_strips(&solved, &op, 0)
                .iter()
                .map(|strip| json!({
                    "y_m": strip.y_m,
                    "cl_c_over_cref": strip.lift_n / (q * strip.width_m * airplane.c_ref),
                }))
                .collect::<Vec<_>>());
        }
    }
    let slope = |key: &str| {
        (points[2][key].as_f64().unwrap_or(f64::NAN) - points[0][key].as_f64().unwrap_or(f64::NAN))
            / 2.0
    };
    let (cla, cma) = (slope("cl"), slope("cm"));
    let x_np = airplane.xyz_ref[0] - cma / cla * airplane.c_ref;
    // The AVL deck at Mach 0 with the pipeline's lattice sizing.
    let (chord, span) = fit_vortex_budget(airplane, chord_res.max(8), (span_res * 10).max(20), 8);
    let deck = render_geometry(AvlDeckRequest {
        airplane,
        mach: 0.0,
        chordwise_vortices: chord,
        spanwise_vortices: span,
    });
    let deck_written = match deck {
        Ok(text) => fs::write(result_dir.join(deck_name), text).is_ok(),
        Err(_) => false,
    };
    json!({
        "points": points, "cl_alpha_per_rad": cla.to_degrees(), "cm_alpha_per_rad": cma.to_degrees(),
        "x_np_m": x_np, "x_ref_m": airplane.xyz_ref[0], "c_ref_m": airplane.c_ref,
        "s_ref_m2": airplane.s_ref, "b_ref_m": airplane.b_ref,
        "span_load_2deg": loads, "deck_written": deck_written,
        "avl_lattice": [chord, span],
    })
}
