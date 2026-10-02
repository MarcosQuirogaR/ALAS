// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Like-for-like evidence for `optimization_preset_audit`: the preset
//! nominal and the delivered design scored by the same product model, the
//! native-mission trip fuel of both, the per-constraint residual tables, and
//! the geometry and field quantities the reports already compute.
//!
//! Nothing here is a new model. The product assessment is
//! `alas_opt::assess_product_candidate`, the optimizer's own evaluation of a
//! candidate, run at the run's effective configuration for both designs, so a
//! difference between the two columns is a difference of design and not of
//! model. The mission trip fuel is read from the pipeline's own outputs: the
//! winner from the optimization run, the baseline from a second pipeline run
//! on the preset nominal with `optimize = false`. Masses are kg, lengths m,
//! angles degrees, wing loadings kg/m^2 (mass, not weight, per unit area).

use alas_config::{AlasConfig, DesignVector};
use alas_opt::{CandidateAssessment, ConstraintResidual, SearchDiagnostics};
use alas_pipeline::{AnalysisReport, PipelineResult};
use serde_json::{json, Value};

/// ICAO Annex 14, Vol. I, Table 1-1: aerodrome reference code letter from
/// wingspan. Returns the code letter and the exclusive upper span bound of
/// that code in metres (`None` above code F).
pub(super) fn icao_code_for_span(span_m: f64) -> Option<(char, Option<f64>)> {
    const CODES: [(char, f64); 4] = [('C', 36.0), ('D', 52.0), ('E', 65.0), ('F', 80.0)];
    if !span_m.is_finite() || span_m <= 0.0 {
        return None;
    }
    if span_m < 15.0 {
        return Some(('A', Some(15.0)));
    }
    if span_m < 24.0 {
        return Some(('B', Some(24.0)));
    }
    Some(
        CODES
            .iter()
            .find(|(_, upper)| span_m < *upper)
            .map_or(('F', None), |(letter, upper)| (*letter, Some(*upper))),
    )
}

/// What the harness knows about the two designs beyond the optimization run.
pub(super) struct Exposure<'a> {
    /// The registered preset's own design vector, the search's nominal.
    pub preset_design: &'a DesignVector,
    /// The pipeline run on the nominal with `optimize = false`.
    pub baseline_run: Option<&'a PipelineResult>,
    /// Why there is no baseline run, when there is none.
    pub baseline_run_error: Option<&'a str>,
}

pub(super) fn exposure_json(result: &PipelineResult, exposure: &Exposure<'_>) -> Value {
    let config = &result.config;
    let winner_design = result.optimized_design.as_ref();
    let baseline = assessment_json(config, Some(exposure.preset_design));
    let winner = assessment_json(config, winner_design);
    json!({
        "same_model_baseline": {
            "model": "alas_opt::assess_product_candidate at the run's effective configuration, applied to the preset nominal and to the delivered design alike; each design is its own design-space nominal there, so the preset window is not re-checked and the ranking cost (normalized by a nominal) is omitted",
            "objective_kind": config.optimizer.objective.kind.as_str(),
            "mtow_mode": config.optimizer.objective.mtow_sizing.as_str(),
            "baseline": baseline,
            "winner": winner,
            "delta_winner_minus_baseline": assessment_delta(&baseline, &winner),
        },
        "native_mission": {
            "note": "trip fuel of the pipeline's native mission, kg; the baseline comes from a separate pipeline run on the preset nominal with optimize=false",
            "baseline": exposure.baseline_run.map_or_else(
                || json!({"unavailable": exposure.baseline_run_error.unwrap_or("not run")}),
                mission_json,
            ),
            "winner": mission_json(result),
        },
        "geometry_and_performance": {
            "note": "values the reports already compute, per side; wing loadings are mass over reference area",
            "baseline": exposure.baseline_run.and_then(side_json).unwrap_or(Value::Null),
            "winner": side_json(result).unwrap_or(Value::Null),
        },
    })
}

/// The search's per-stage counts (`replay_evaluations`: pre-gate-passed
/// candidates including repeats, which `--replay-evaluations S,R,P,Q` replays
/// with P the refinement's `planned_evaluations` and Q its
/// `restoration_evaluations`;
/// `analysis_evaluations`: coupled analyses), each with its pre-gate cap,
/// rejections by reason and hard-constraint or closure failures.
pub(super) fn stages_json(diagnostics: &SearchDiagnostics) -> Value {
    diagnostics
        .stages
        .iter()
        .map(|stage| {
            let mut row = json!(stage);
            row["replay_evaluations"] = json!(stage.evaluations);
            if let Some(rejections) = diagnostics
                .rejections
                .iter()
                .find(|rejections| rejections.stage == stage.stage)
            {
                row["max_pregate_rejects"] = json!(rejections.max_pregate_rejects);
                row["pre_gate_reasons"] = json!(rejections.pre_gate);
                row["analysed_failures"] = json!(rejections.analysed_failures);
            }
            row
        })
        .collect()
}

fn residual_json(residual: &ConstraintResidual) -> Value {
    json!({
        "id": residual.id,
        "family": format!("{:?}", residual.family),
        "policy": format!("{:?}", residual.policy),
        "value": residual.actual,
        "limit": residual.limit,
        "unit": residual.unit,
        "raw_residual": residual.raw_residual,
        "normalized_violation": residual.normalized_violation,
        "violated": residual.normalized_violation > 0.0,
    })
}

fn assessment_json(config: &AlasConfig, design: Option<&DesignVector>) -> Value {
    let Some(design) = design else {
        return json!({"error": "no design delivered"});
    };
    match alas_opt::assess_product_candidate(config, design) {
        Ok(assessment) => assessed(&assessment),
        Err(error) => json!({"error": error}),
    }
}

fn assessed(assessment: &CandidateAssessment) -> Value {
    json!({
        "objective_value": assessment.objective_value,
        "block_fuel_kg": assessment.sized.block_fuel_kg,
        "takeoff_mass_kg": assessment.sized.takeoff_mass_kg,
        "operating_empty_mass_kg": assessment.sized.operating_empty_mass_kg,
        "strictly_feasible": assessment.is_strictly_feasible(),
        "hard_feasible": assessment.hard_feasible,
        "hard_violation_sum": assessment.hard_violation_sum,
        "soft_violation_sum": assessment.soft_violation_sum,
        "violated_hard_ids": assessment.violated_hard_ids(),
        "residuals": assessment.residuals.iter().map(residual_json).collect::<Vec<_>>(),
    })
}

fn assessment_delta(baseline: &Value, winner: &Value) -> Value {
    let difference = |key: &str| match (baseline[key].as_f64(), winner[key].as_f64()) {
        (Some(before), Some(after)) => json!(after - before),
        _ => Value::Null,
    };
    let relative = match (
        baseline["objective_value"].as_f64(),
        winner["objective_value"].as_f64(),
    ) {
        (Some(before), Some(after)) if before.is_finite() && before != 0.0 => {
            json!((after - before) / before)
        }
        _ => Value::Null,
    };
    json!({
        "objective_value": difference("objective_value"),
        "relative_objective_change": relative,
        "block_fuel_kg": difference("block_fuel_kg"),
        "takeoff_mass_kg": difference("takeoff_mass_kg"),
        "operating_empty_mass_kg": difference("operating_empty_mass_kg"),
    })
}

fn mission_json(result: &PipelineResult) -> Value {
    let summary = result
        .mission_result
        .as_ref()
        .and_then(alas_mission::MissionResult::completed_summary);
    let mission = &result.feasibility.fuel_loading.mission;
    json!({
        "status": format!("{:?}", mission.status),
        "trip_fuel_kg": summary.map(|s| s.trip_fuel_kg),
        "required_trip_fuel_kg": mission.required_trip_fuel_kg,
        "takeoff_mass_kg": summary.map(|s| s.takeoff_mass_kg),
        "landing_mass_kg": summary.map(|s| s.landing_mass_kg),
        "block_time_s": summary.map(|s| s.block_time_s),
        "distance_flown_m": summary.map(|s| s.distance_flown_m),
    })
}

/// Geometry, wing loading, ICAO code, field performance and CG range of one
/// reported design.
fn side_json(result: &PipelineResult) -> Option<Value> {
    let report = result.optimized_report.as_ref()?;
    let config = &result.config;
    let s_ref = report.airplane.s_ref;
    let span_m = report.airplane.b_ref;
    let mtow_kg = config.requirements.mtow_kg;
    let mlw_kg = report.design_landing_mass_kg().or_else(|| {
        alas_config::presets::get(&config.preset)
            .ok()
            .and_then(|preset| preset.reference.mlw_kg)
    });
    let per_area = |mass: Option<f64>| {
        mass.filter(|m| m.is_finite() && s_ref.is_finite() && s_ref > 0.0)
            .map(|m| m / s_ref)
    };
    let field = mlw_kg.and_then(|mlw| {
        alas_pipeline::field_reference::report_isa_sea_level_field_reference(
            config, report, s_ref, mtow_kg, mlw,
        )
        .ok()
    });
    let code = icao_code_for_span(span_m);
    let cg = &result.feasibility.cg_envelope;
    Some(json!({
        "span_m": span_m,
        "wing_area_m2": s_ref,
        "aspect_ratio": report.geometry_summary.get("aspect_ratio"),
        "taper_ratio": report.geometry_summary.get("taper_ratio"),
        "quarter_chord_sweep_deg": quarter_chord_sweep_deg(report),
        "wing_loading_at_mtow_kg_m2": per_area(Some(mtow_kg)),
        "wing_loading_at_mlw_kg_m2": per_area(mlw_kg),
        "mtow_kg": mtow_kg,
        "mlw_kg": mlw_kg,
        "analysis_takeoff_mass_kg": report.analysis_takeoff_mass_kg(mtow_kg),
        "icao_code_letter": code.map(|(letter, _)| letter.to_string()),
        "icao_code_span_limit_m": code.and_then(|(_, upper)| upper),
        "icao_code_span_margin_m": code.and_then(|(_, upper)| upper).map(|upper| upper - span_m),
        "configured_max_design_span_m": config.max_design_span_m(),
        "field_reference_isa_sea_level": field.map(|f| json!({
            "takeoff_field_length_m": f.takeoff_field_length_m,
            "model_balanced_field_length_m": f.model_balanced_field_length_m,
            "landing_field_length_m": f.landing_field_length_m,
            "vref_m_s": f.vref_m_s,
            "static_tw": f.static_tw,
        })),
        "cg_envelope": {
            "planning_status": format!("{:?}", cg.planning_status),
            "cg_pct_mac": cg.cg_pct_mac,
            "forward_limit_pct_mac": cg.forward_limit_pct_mac,
            "aft_limit_pct_mac": cg.aft_limit_pct_mac,
            "static_margin": report.static_margin,
        },
    }))
}

/// Quarter-chord sweep of the main wing, degrees, positive swept back; the
/// geometry's own `Wing::mean_sweep_angle(0.25)`.
fn quarter_chord_sweep_deg(report: &AnalysisReport) -> Option<f64> {
    let wing = report.airplane.wings.first()?;
    (wing.xsecs.len() >= 2).then(|| wing.mean_sweep_angle(0.25))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icao_code_follows_annex_14_span_bands() {
        assert_eq!(icao_code_for_span(34.1), Some(('C', Some(36.0))));
        assert_eq!(icao_code_for_span(36.0), Some(('D', Some(52.0))));
        assert_eq!(icao_code_for_span(64.9), Some(('E', Some(65.0))));
        assert_eq!(icao_code_for_span(79.75), Some(('F', Some(80.0))));
        assert_eq!(icao_code_for_span(80.0), Some(('F', None)));
        assert_eq!(icao_code_for_span(f64::NAN), None);
    }

    #[test]
    fn each_stage_row_carries_its_rejection_reasons_and_analysed_failures() {
        let stage = json!({
            "stage": "screening", "max_evaluations": 200, "time_limit_s": 30.0,
            "evaluations": 200, "pre_gate_rejects": 5, "analysis_evaluations": 200,
            "generations": 4, "feasible": 12, "elite_size": 11, "wall_time_s": 9.0,
            "termination": "evaluation_budget",
        });
        let diagnostics: SearchDiagnostics = serde_json::from_value(json!({
            "converged": false, "analysis_evaluations": 0, "cache_hits": 0,
            "poll_iterations": 0, "screening_evaluations": 200, "screening_feasible": 12,
            "verification_evaluations": 0, "scan_wall_time_s": 9.0,
            "search_wall_time_s": 0.0, "workers": 8, "poll_block_size": 0,
            "first_feasible_cost": null, "relative_improvement": null,
            "stages": [stage],
            "rejections": [{
                "stage": "screening", "max_pregate_rejects": 4000,
                "pre_gate": {"design_box": 0, "planform": 1, "trailing_edge_angle": 4, "span_code": 0},
                "analysed_failures": {"root_to_kink_te_angle": 0, "cg_envelope": 7},
            }],
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        let rows = stages_json(&diagnostics);
        let row = &rows[0];
        assert_eq!(row["evaluations"], 200);
        assert_eq!(row["replay_evaluations"], row["evaluations"]);
        assert_eq!(row["max_pregate_rejects"], 4000);
        let reasons = &row["pre_gate_reasons"];
        let total: u64 = ["design_box", "planform", "trailing_edge_angle", "span_code"]
            .iter()
            .filter_map(|key| reasons[key].as_u64())
            .sum();
        assert_eq!(json!(total), row["pre_gate_rejects"]);
        assert_eq!(row["analysed_failures"]["cg_envelope"], 7);
    }

    #[test]
    fn the_delta_is_winner_minus_baseline_and_null_when_either_side_failed() {
        let baseline = json!({"objective_value": 10.0, "block_fuel_kg": 9.0});
        let winner = json!({"objective_value": 9.0, "block_fuel_kg": 8.0});
        let delta = assessment_delta(&baseline, &winner);
        assert_eq!(delta["objective_value"], json!(-1.0));
        assert_eq!(delta["block_fuel_kg"], json!(-1.0));
        assert!(
            (delta["relative_objective_change"]
                .as_f64()
                .unwrap_or(f64::NAN)
                + 0.1)
                .abs()
                < 1e-12
        );
        let failed = assessment_delta(&baseline, &json!({"error": "x"}));
        assert!(failed["objective_value"].is_null());
    }
}
