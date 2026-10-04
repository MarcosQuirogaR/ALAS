// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! First-hand native OpenVSP/VSPAERO lifting-mesh sensitivity protocol.
//!
//! The three cases keep the ALAS aircraft, reference quantities, moment origin,
//! Mach, beta, alpha schedule, and VSPAERO method fixed. They vary only
//! OpenVSP's documented wing tessellation controls. The retained summary has
//! `CL` and `Cm` only; VSPAERO's inviscid drag must not be compared with the
//! product report's hybrid total drag.

use std::fs;
use std::path::{Path, PathBuf};

use alas_config::AlasConfig;
use alas_pipeline::{
    apply_vspaero_mesh_resolution, assess_vspaero_refinement, export_openvsp_script,
    materialize_openvsp_project, run_vspaero_analysis, DesignPipeline, OpenVspExportStatus,
    PipelineOptions, RunEnvironment, VspaeroAnalysisStatus, VspaeroRefinementVerdict,
    VSPAERO_REFINEMENT_LEVELS,
};
use serde_json::{json, Value};

fn main() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let openvsp = std::env::var_os("ALAS_OPENVSP_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("external tools/OpenVSP-3.51.2-win64/vspscript.exe"));
    let vspaero = std::env::var_os("ALAS_VSPAERO_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("external tools/OpenVSP-3.51.2-win64/vspaero.exe"));
    let output = std::env::var_os("ALAS_VSPAERO_REFINEMENT_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("outputs/vspaero_refinement_20260820"));
    if !openvsp.is_file() || !vspaero.is_file() {
        return Err(format!(
            "native refinement requires {} and {}",
            openvsp.display(),
            vspaero.display()
        ));
    }
    if output.exists() {
        return Err(format!(
            "refusing to overwrite retained refinement evidence at {}; choose ALAS_VSPAERO_REFINEMENT_OUTPUT",
            output.display()
        ));
    }

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
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let baseline = DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
        .map_err(|error| format!("build fixed ALAS VSPAERO reference case: {error}"))?;
    let report = baseline
        .optimized_report
        .ok_or_else(|| "fixed ALAS VSPAERO reference case has no analysis report".to_owned())?;
    let wing_sections = report
        .airplane
        .wings
        .iter()
        .map(|wing| wing.xsecs.len().saturating_sub(1))
        .collect::<Vec<_>>();
    if wing_sections.contains(&0) {
        return Err("ALAS lifting geometry has a wing with no physical sections".to_owned());
    }
    fs::create_dir_all(&output).map_err(|error| format!("create {}: {error}", output.display()))?;

    let mut runs = Vec::new();
    let mut summaries = Vec::new();
    for resolution in VSPAERO_REFINEMENT_LEVELS {
        let script_path = output
            .join(resolution.label)
            .join("openvsp")
            .join("optimized_aircraft.vspscript");
        let export = export_openvsp_script(&report, &config, &script_path)
            .map_err(|error| format!("write {} script: {error}", resolution.label))?;
        let source = fs::read_to_string(&script_path)
            .map_err(|error| format!("read {}: {error}", script_path.display()))?;
        let refined = apply_vspaero_mesh_resolution(&source, &wing_sections, resolution)?;
        fs::write(&script_path, refined)
            .map_err(|error| format!("write {}: {error}", script_path.display()))?;
        let export = materialize_openvsp_project(export, &openvsp, 180.0);
        let result = run_vspaero_analysis(&report, &config, &export, Some(&vspaero), 600.0);
        summaries.push(case_summary(&output, resolution, &export, &result));
        runs.push(result);
    }

    let assessment = if runs
        .iter()
        .all(|run| run.status == VspaeroAnalysisStatus::CompletedComparable)
    {
        let polars = runs
            .iter()
            .map(|run| {
                run.polar
                    .as_ref()
                    .ok_or_else(|| "completed comparable VSPAERO run has no polar".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Some(assess_vspaero_refinement(polars[0], polars[1], polars[2])?)
    } else {
        None
    };
    let assessment_json = assessment.map(|assessment| {
        json!({
            "coarse_to_medium_max_abs_cl": assessment.coarse_to_medium.max_abs_cl,
            "coarse_to_medium_max_abs_cm": assessment.coarse_to_medium.max_abs_cm,
            "medium_to_fine_max_abs_cl": assessment.medium_to_fine.max_abs_cl,
            "medium_to_fine_max_abs_cm": assessment.medium_to_fine.max_abs_cm,
            "verdict": match assessment.verdict {
                VspaeroRefinementVerdict::ChangesShrink => "changes_shrink",
                VspaeroRefinementVerdict::ChangesDoNotShrink => "changes_do_not_shrink",
            },
            "meaning": "A shrinking change is a mesh-sensitivity observation, not an absolute-error or experimental-validation claim."
        })
    });
    let completed = assessment_json.is_some();
    let summary = json!({
        "runtime": "OpenVSP 3.51.2 / VSPAERO 7.2.2",
        "protocol": "OpenVSP SectTess_U per wing section and Tess_W per lifting surface vary; geometry, SI references, moment origin, VSPAERO method, Mach, beta, and alpha schedule are fixed.",
        "comparison": "CL(alpha) and Cm(alpha) only, after each native polar passes the existing completed_comparable contract.",
        "not_compared": "VSPAERO drag is inviscid and is not overlaid or evaluated against the ALAS hybrid total-drag result.",
        "wing_sections": wing_sections,
        "cases": summaries,
        "assessment": assessment_json,
        "status": if completed { "completed" } else { "incomplete" }
    });
    let summary_path = output.join("vspaero_refinement_summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("encode refinement summary: {error}"))?,
    )
    .map_err(|error| format!("write {}: {error}", summary_path.display()))?;
    Ok(())
}

fn case_summary(
    output: &Path,
    resolution: alas_pipeline::VspaeroMeshResolution,
    export: &alas_pipeline::OpenVspExportResult,
    result: &alas_pipeline::VspaeroAnalysisResult,
) -> Value {
    let coefficients = result.polar.as_ref().map(|polar| {
        json!({
            "alpha_deg": polar.points.iter().map(|point| point.alpha_deg).collect::<Vec<_>>(),
            "cl": polar.points.iter().map(|point| point.lift_coefficient).collect::<Vec<_>>(),
            "cm_pitch": polar.points.iter().map(|point| point.pitching_moment_coefficient).collect::<Vec<_>>(),
        })
    });
    json!({
        "level": resolution.label,
        "section_spanwise": resolution.section_spanwise,
        "chordwise": resolution.chordwise,
        "openvsp_status": match export.status {
            OpenVspExportStatus::ScriptWrittenRuntimeUnverified => "script_written_runtime_unverified",
            OpenVspExportStatus::RuntimeLaunchFailed => "runtime_launch_failed",
            OpenVspExportStatus::RuntimeTimedOut => "runtime_timed_out",
            OpenVspExportStatus::InvalidTimeout => "invalid_timeout",
            OpenVspExportStatus::RuntimeRejected => "runtime_rejected",
            OpenVspExportStatus::Vsp3Materialized => "vsp3_materialized",
        },
        "vspaero_status": result.status.as_str(),
        "comparison": match &result.comparison {
            alas_pipeline::VspaeroComparisonStatus::Compatible(_) => "completed_comparable",
            alas_pipeline::VspaeroComparisonStatus::NotEvaluated => "not_evaluated",
            alas_pipeline::VspaeroComparisonStatus::Rejected(reason) => reason,
        },
        "error": result.error.clone(),
        "geometry_path": relative(output, &result.geometry_path),
        "setup_path": relative(output, &result.setup_path),
        "polar_path": relative(output, &result.polar_path),
        "coefficients": coefficients,
    })
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
