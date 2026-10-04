// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Retain search counts when reporting fails, without granting product acceptance.

use std::path::Path;

use alas_config::DesignVector;
use alas_opt::{OptimizationHistory, SearchDiagnostics};
use serde::Deserialize;
use serde_json::{json, Value};

const FILES: [&str; 2] = [
    "optimization_evidence.json",
    "solvers/vlm/optimization_search.json",
];
const VALID_DEFINITION: &str = "Distinct design vectors in the full in-loop history with valid=true, zero hard violation, and finite objective and ranking cost. Separate screening scores are excluded. Retained search evidence does not verify any delivered candidate at reporting fidelity.";

/// Remove prior run evidence so an early failure cannot inherit its counts.
pub(super) fn prepare(directory: &Path) -> Option<String> {
    for name in FILES {
        let path = directory.join(name);
        if let Err(error) = std::fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                let message = format!(
                    "cannot clear prior search evidence {}: {error}",
                    path.display()
                );
                eprintln!("[warn] {message}");
                return Some(message);
            }
        }
    }
    None
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct CountHistory {
    design_vectors: Vec<DesignVector>,
    valid: Vec<bool>,
    cost: Vec<Option<f64>>,
    objective_value: Vec<Option<f64>>,
    hard_violation: Vec<Option<f64>>,
}

fn history_for_counting(value: &Value) -> Result<OptimizationHistory, serde_json::Error> {
    let history: CountHistory = serde_json::from_value(value.clone())?;
    let numbers = |values: Vec<Option<f64>>| {
        values
            .into_iter()
            .map(|value| value.unwrap_or(f64::NAN))
            .collect()
    };
    Ok(OptimizationHistory {
        design_vectors: history.design_vectors,
        valid: history.valid,
        cost: numbers(history.cost),
        objective_value: numbers(history.objective_value),
        hard_violation: numbers(history.hard_violation),
        ..Default::default()
    })
}

fn read(path: &Path, search_only: bool) -> Result<(Value, OptimizationHistory), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let record: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let optimization = if search_only {
        if record["record_kind"] != "optimization_search"
            || record["delivered_feasible"] != false
            || record["reporting_verified"] != false
        {
            return Err("invalid search-only evidence scope".into());
        }
        record["optimization"].clone()
    } else {
        record
    };
    let history =
        history_for_counting(&optimization["history"]).map_err(|error| error.to_string())?;
    Ok((optimization, history))
}

/// Fill absent accounting only; pipeline and delivered-candidate verdicts stay authoritative.
pub(super) fn attach(row: &mut Value, directory: &Path, freshness_error: Option<&str>) {
    if let Some(error) = freshness_error {
        row["search_evidence"] = json!({"available":false,"error":error});
        return;
    }
    let mut errors = Vec::new();
    for (index, name) in FILES.into_iter().enumerate() {
        let path = directory.join(name);
        if !path.exists() {
            continue;
        }
        let (optimization, history) = match read(&path, index != 0) {
            Ok(result) => result,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let diagnostics: Option<SearchDiagnostics> =
            match serde_json::from_value(optimization["search_diagnostics"].clone()) {
                Ok(diagnostics) => diagnostics,
                Err(error) => {
                    errors.push(format!("{} stage diagnostics: {error}", path.display()));
                    None
                }
            };
        if !row["optimizer"].is_object() {
            row["optimizer"] = json!({
                "cancelled": optimization["termination"] == "cancelled",
                "converged":false,"delivered_feasible":false,
            });
        }
        let optimizer = &mut row["optimizer"];
        for field in [
            "method",
            "strategy",
            "termination",
            "best_valid",
            "best_cost",
        ] {
            if optimizer[field].is_null() {
                optimizer[field] = optimization[field].clone();
            }
        }
        if optimizer["stages"].is_null() {
            optimizer["stages"] = diagnostics.as_ref().map_or_else(
                || optimization["search_diagnostics"]["stages"].clone(),
                super::exposure::stages_json,
            );
        }
        if optimizer["total_full_fidelity_valid"].is_null() {
            optimizer["total_full_fidelity_valid"] =
                json!(super::exposure::full_fidelity_valid(&history));
        }
        optimizer["valid_candidate_definition"] = VALID_DEFINITION.into();
        optimizer["search_converged"] = optimization["search_diagnostics"]["converged"].clone();
        row["search_evidence"] = json!({
            "available":true,"path":path,"search_only":index != 0,
            "history_evaluations":history.design_vectors.len(),
            "does_not_establish_delivered_feasibility":true,"read_errors":errors,
        });
        return;
    }
    row["search_evidence"] = json!({"available":false,"read_errors":errors});
}

#[cfg(test)]
// JSON fixtures and temporary-file failures are assertions.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn null_or_missing_metrics_never_count_as_finite_full_fidelity_validity() {
        let nominal = DesignVector::default();
        let changed = DesignVector {
            span_m: nominal.span_m + 1.0,
            ..nominal
        };
        let history = history_for_counting(&json!({
            "design_vectors":[nominal,nominal,changed,changed,changed],
            "valid":[true,true,true,true,true],
            "cost":[1.0,1.0,null,1.0,1.0],
            "objective_value":[2.0,2.0,2.0,null,2.0],
            "hard_violation":[0.0,0.0,0.0,0.0,0.1],
        }))
        .expect("nullable history");
        assert_eq!(super::super::exposure::full_fidelity_valid(&history), 1);
        let missing = history_for_counting(&json!({"design_vectors":[nominal],"valid":[true]}))
            .expect("old incomplete history");
        assert_eq!(super::super::exposure::full_fidelity_valid(&missing), 0);
    }

    #[test]
    fn reporting_failure_retains_search_counts_and_never_promotes_delivery() {
        let directory =
            std::env::temp_dir().join(format!("alas-audit-search-evidence-{}", std::process::id()));
        let branch = directory.join("solvers/vlm");
        std::fs::create_dir_all(&branch).expect("branch directory");
        let path = branch.join("optimization_search.json");
        let diagnostics = SearchDiagnostics {
            stages: vec![
                alas_opt::StageSummary {
                    stage: "screening".into(),
                    analysis_evaluations: 2,
                    feasible: 2,
                    ..Default::default()
                },
                alas_opt::StageSummary {
                    stage: "refinement".into(),
                    analysis_evaluations: 1,
                    feasible: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let search = json!({
            "record_kind":"optimization_search", "delivered_feasible":false,
            "reporting_verified":false, "optimization":{
                "method":"differential_evolution","termination":"time_budget",
                "best_valid":true,"history":{
                    "design_vectors":[DesignVector::default()], "valid":[true],
                    "cost":[1.0],"objective_value":[2.0],"hard_violation":[0.0],
                },"search_diagnostics":diagnostics,
            },
        });
        std::fs::write(&path, serde_json::to_vec(&search).expect("evidence JSON"))
            .expect("search evidence");
        let mut row = json!({"status":"error","execution_passed":false});
        attach(&mut row, &directory, None);
        assert_eq!(row["status"], "error");
        assert_eq!(row["execution_passed"], false);
        assert_eq!(row["optimizer"]["delivered_feasible"], false);
        assert_eq!(row["optimizer"]["total_full_fidelity_valid"], 1);
        assert_eq!(row["optimizer"]["stages"][0]["analysis_evaluations"], 2);
        assert_eq!(row["optimizer"]["stages"][1]["feasible"], 1);
        let completed = json!({"status":"success","optimizer":{
            "delivered_feasible":true,"total_full_fidelity_valid":2,
        }});
        let mut preserved = completed.clone();
        attach(&mut preserved, &directory, None);
        assert_eq!(preserved["optimizer"]["delivered_feasible"], true);
        assert_eq!(preserved["optimizer"]["total_full_fidelity_valid"], 2);
        let snapshot_path = directory.join(FILES[0]);
        std::fs::write(
            &snapshot_path,
            serde_json::to_vec(&search["optimization"]).expect("snapshot JSON"),
        )
        .expect("published snapshot");
        let mut snapshot = json!({"status":"error"});
        attach(&mut snapshot, &directory, None);
        assert_eq!(snapshot["search_evidence"]["search_only"], false);
        assert_eq!(snapshot["optimizer"]["delivered_feasible"], false);
        assert!(prepare(&directory).is_none());
        assert!(!path.exists());
        assert!(!snapshot_path.exists());
        let mut empty = json!({"status":"error"});
        attach(&mut empty, &directory, None);
        assert_eq!(empty["search_evidence"]["available"], false);
        std::fs::remove_dir(&branch).expect("empty branch");
        std::fs::remove_dir(directory.join("solvers")).expect("empty solvers directory");
        std::fs::remove_dir(directory).expect("empty run directory");
    }
}
