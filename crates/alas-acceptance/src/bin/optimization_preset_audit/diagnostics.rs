// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Complete stage evidence, separate from optimizer and physical validation.
use super::*;
use alas_pipeline::MissionFuelStatus;

/// Native analysis completion is independent of search convergence and of
/// whether optional installed external tools succeeded. Both are reported.
/// The mission is complete when the route's trip was flown on the unified
/// segment mission model; the native pseudospectral flight is telemetry and
/// does not decide completion.
pub(super) fn full_analysis_completed(result: &PipelineResult) -> bool {
    result.optimized_report.is_some()
        && result.baseline_analysis.is_some()
        && (!result.config.mission.enabled || route_flown(result))
        && (!result.config.structures.enabled
            || result
                .structural_result
                .as_ref()
                .is_some_and(|s| s.status == "ok" && s.sizing.is_some() && s.analysis.is_some()))
}

/// Whether the route's trip was flown on the fuel carried, on the unified
/// segment mission model.
fn route_flown(result: &PipelineResult) -> bool {
    result.feasibility.fuel_loading.mission.status == MissionFuelStatus::Completed
}

pub(super) fn evidence(result: &PipelineResult) -> Value {
    let mission = result.mission_result.as_ref();
    let summary = mission.and_then(|m| m.completed_summary());
    let structures = result.structural_result.as_ref();
    json!({
        "completion_definition": "full_analysis_completed requires optimized and baseline native reports, the route's trip flown on the unified segment mission model when the mission is enabled (the native mission is telemetry), and successful analytical structural sizing when enabled; it does not assert optimizer convergence, physical validation, or external-solver completion",
        "all_error_findings": all_error_findings(&result.feasibility.findings),
        "baseline_analysis_present": result.baseline_analysis.is_some(),
        "baseline_analysis_error": result.baseline_analysis_error,
        "optimized_analysis_present": result.optimized_report.is_some(),
        "mission": {
            "requested": result.config.mission.enabled,
            "present": mission.is_some(),
            "route_flown": route_flown(result),
            "route_trip_fuel_kg": result.feasibility.fuel_loading.mission.required_trip_fuel_kg,
            "native_completed": summary.is_some(),
            "completion_refusal": mission.and_then(|m| m.completion_refusal()),
            "scheduled_segments": mission.map(|m| m.scheduled_segment_count),
            "returned_segments": mission.map(|m| m.segments.len()),
            "converged_segments": mission.map(|m| m.solutions.iter().filter(|s| s.converged).count()),
            "fuel_exhaustion": mission.and_then(|m| m.fuel_exhaustion.as_ref()).map(|f| json!({
                "segment": f.segment_tag, "available_fuel_kg": f.available_fuel_kg,
                "burned_fuel_kg": f.burned_fuel_kg})),
            "fuel_within_available": route_flown(result)
                && !result.feasibility.contains(FindingCode::MissionFuelShortfall)
                && !result.feasibility.contains(FindingCode::InvalidMissionFuelBurn)
                && !result.feasibility.contains(FindingCode::ReserveFuelShortfall),
            "native_trip_fuel_kg": summary.map(|s| s.trip_fuel_kg),
            "block_time_s": summary.map(|s| s.block_time_s),
            "distance_flown_m": summary.map(|s| s.distance_flown_m),
        },
        "structures": {
            "requested": result.config.structures.enabled,
            "status": structures.map(|s| s.status.as_str()),
            "error": structures.and_then(|s| s.error.as_ref()),
            "sizing_present": structures.is_some_and(|s| s.sizing.is_some()),
            "analytical_report_present": structures.is_some_and(|s| s.analysis.is_some()),
            "nastran": structures.and_then(|s| s.nastran.as_ref()).map(nastran_evidence),
            "nastran95": structures.and_then(|s| s.nastran95.as_ref()).map(nastran_evidence),
            "patran": structures.and_then(|s| s.patran.as_ref()).map(|p| json!({
                "status": p.status, "error": p.error, "images": p.png_paths.len()})),
        },
        "external_solvers": external_evidence(result),
        "gui_equivalence": {
            "pipeline_entry_point": "DesignPipeline::run_with_design_space_events_and_snapshots with the same resolved preset bounds and immutable snapshot observers as the GUI launch",
            "configuration_origin": "canonical AlasConfig preset with the explicitly reported solver preset",
            "mission_profile": "Canonical preset initialized as in the GUI: same resolved endpoint records, great-circle radius, shared route proposal and configure_cruise_legs call. Aircraft speeds and rates remain canonical.",
            "machine_paths": "Persisted machine preferences applied as at GUI startup; unsaved GUI session path edits are not part of this headless run. OpenVSP, AVL and FLOWUnsteady discovery consult preferences.",
        },
    })
}

fn all_error_findings(findings: &[PhysicalFinding]) -> Vec<Value> {
    findings
        .iter()
        .filter(|f| f.severity == FindingSeverity::Error)
        .map(|f| {
            json!({"code": format!("{:?}", f.code), "message": f.message,
            "actual": f.actual, "limit": f.limit, "unit": f.unit})
        })
        .collect()
}

fn nastran_evidence(result: &alas_struct::nastran::NastranResults) -> Value {
    json!({
        "static": {"status": result.static_solve.status.as_str(), "error": result.static_solve.error},
        "modes": {"status": result.modes.status.as_str(), "error": result.modes.error},
        "vibration": {"status": result.vibration.status.as_str(), "error": result.vibration.error,
            "random_response_error": result.vibration.random_response_error},
    })
}

fn external_evidence(result: &PipelineResult) -> Value {
    json!({
        "mses": result.mses_result.as_ref().map(|r| json!({
            "status": r.status.as_str(), "error": r.error,
            "requested_points": r.requested_alpha_count, "converged_points": r.converged_alpha_count})),
        "openvsp": result.openvsp_export.as_ref().map(|r| json!({
            "status": r.status.as_str(), "error": r.runtime_error,
            "stdout": r.runtime_stdout_path, "stderr": r.runtime_stderr_path})),
        "vspaero": result.vspaero_result.as_ref().map(|r| json!({
            "status": format!("{:?}", r.status), "error": r.error, "polar_present": r.polar.is_some(),
            "stdout": r.stdout_path, "stderr": r.stderr_path})),
        "avl": result.avl_result.as_ref().map(|r| json!({
            "status": r.status.as_str(), "error": r.error, "polar_present": r.polar.is_some(),
            "stdout": r.stdout_path, "stderr": r.stderr_path})),
        "flowunsteady": result.flowunsteady_result.as_ref().map(|r| json!({
            "status": r.status.as_str(), "error": r.error, "polar_present": r.polar.is_some(),
            "stdout": r.stdout_path, "stderr": r.stderr_path})),
    })
}

/// Downstream external-tool stages this preset's config reached but for which
/// no executable was discovered on this host, distinct from a stage that ran
/// and failed. Only the "not discovered" variants are gaps; a launch failure
/// or solver failure is a real error surfaced elsewhere in the row.
pub(super) fn dependency_gaps(result: &PipelineResult) -> Vec<String> {
    let mut gaps = Vec::new();
    if let Some(openvsp) = &result.openvsp_export {
        if openvsp.status == OpenVspExportStatus::ScriptWrittenRuntimeUnverified {
            gaps.push("openvsp: script written, no installed runtime discovered".to_owned());
        }
    }
    if let Some(vspaero) = &result.vspaero_result {
        if vspaero.status == VspaeroAnalysisStatus::NotConfigured {
            gaps.push("vspaero: no native executable configured or discovered".to_owned());
        }
    }
    if let Some(avl) = &result.avl_result {
        if avl.status == AvlAnalysisStatus::NotConfigured {
            gaps.push("avl: no solver executable available".to_owned());
        }
    }
    if let Some(mses) = &result.mses_result {
        use alas_aero::mses::MsesStatus;
        match mses.status {
            MsesStatus::Absent => gaps.push("mses: no MSES directory found".to_owned()),
            MsesStatus::Incomplete => {
                gaps.push("mses: directory present but missing one or more programs".to_owned())
            }
            _ => {}
        }
    }
    if let Some(flowunsteady) = &result.flowunsteady_result {
        if flowunsteady.status == FlowUnsteadyAnalysisStatus::NotConfigured {
            gaps.push("flowunsteady: no adapter executable configured".to_owned());
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_errors_remain_visible_in_complete_evidence() {
        let findings: Vec<_> = [
            FindingCode::MissionUnavailable,
            FindingCode::MissionNotConverged,
            FindingCode::InvalidMissionFuelBurn,
            FindingCode::MissionFuelShortfall,
        ]
        .into_iter()
        .map(|code| PhysicalFinding {
            code,
            severity: FindingSeverity::Error,
            message: "mission failed".into(),
            actual: Some(2.0),
            limit: Some(1.0),
            unit: "kg",
        })
        .collect();
        let rows = all_error_findings(&findings);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[3]["code"], "MissionFuelShortfall");
        assert_eq!(rows[3]["actual"], 2.0);
    }

    #[test]
    fn structural_solver_failure_is_not_hidden_by_other_successful_cases() {
        let mut result = alas_struct::nastran::NastranResults::default();
        result.static_solve.status = alas_struct::nastran::ResultStatus::Ok;
        result.modes.status = alas_struct::nastran::ResultStatus::Error;
        result.modes.error = Some("missing OP2".into());
        let evidence = nastran_evidence(&result);
        assert_eq!(evidence["static"]["status"], "ok");
        assert_eq!(evidence["modes"]["status"], "error");
        assert_eq!(evidence["modes"]["error"], "missing OP2");
        assert_eq!(evidence["vibration"]["status"], "not_run");
    }
}
