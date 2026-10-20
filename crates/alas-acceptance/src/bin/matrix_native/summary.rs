// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resume test, per-cell extraction and the summary CSV/JSON.
//!
//! The summary is rebuilt from the cell directories (`measurement_result.json`,
//! `runner_cell.json`, and the `[stage]` lines of `stderr.log`), so it is
//! idempotent and a resumed run reports the cells of earlier invocations too.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{json, Map, Value};

use super::budget::BudgetTable;
use super::Cell;

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn cell_dir(root: &Path, cell: &Cell) -> std::path::PathBuf {
    root.join("cells").join(cell.id())
}

/// Statuses that are final, deterministic outcomes of a completed search: a
/// search that ended without a feasible design (`infeasible`) repeats exactly
/// under the same evaluation budget, so resume does not rerun it. `error`,
/// `timeout` and a missing result are rerun.
fn is_completed(status: &str) -> bool {
    matches!(status, "success" | "infeasible")
}

/// A cell is done when it has a parseable result with a completed status.
pub fn cell_done(root: &Path, cell: &Cell) -> bool {
    read_json(&cell_dir(root, cell).join("measurement_result.json"))
        .is_some_and(|row| row["status"].as_str().is_some_and(is_completed))
}

/// 64-bit FNV-1a, a stable digest (unlike `DefaultHasher`, whose algorithm is unspecified).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The load-independent part of a result row: search counts and terminations,
/// objective, masses, performance, comparison tables and findings. Wall times,
/// utilization, paths, worker counts and cancellation telemetry are excluded.
/// Two cells with the same budget, seed and mode must give the same digest at
/// any `P x W`.
fn determinism_digests(row: &Value) -> (String, String) {
    let stages: Vec<Value> = row["optimizer"]["stages"]
        .as_array()
        .map(|stages| {
            stages
                .iter()
                .map(|stage| {
                    let mut kept = Map::new();
                    for key in [
                        "stage",
                        "evaluations",
                        "analysis_evaluations",
                        "restoration_evaluations",
                        "pre_gate_rejects",
                        "generations",
                        "feasible",
                        "elite_size",
                        "termination",
                    ] {
                        kept.insert(key.to_owned(), stage[key].clone());
                    }
                    Value::Object(kept)
                })
                .collect()
        })
        .unwrap_or_default();
    let optimizer = &row["optimizer"];
    let search = json!({
        "status": row["status"],
        "mtow_mode": row["mtow_mode"],
        "termination": optimizer["termination"],
        "best_cost": optimizer["best_cost"],
        "best_valid": optimizer["best_valid"],
        "delivered_feasible": optimizer["delivered_feasible"],
        "total_full_fidelity_valid": optimizer["total_full_fidelity_valid"],
        "stages": stages,
    });
    let report = json!({
        "search": search,
        "baseline_vs_optimized": row["baseline_vs_optimized"],
        "like_for_like": row["like_for_like"],
        "mtow_kg": row["mtow_kg"],
        "oew_kg": row["oew_kg"],
        "cruise_l_over_d": row["cruise_l_over_d"],
        "neutral_point_x": row["neutral_point_x"],
        "static_margin": row["static_margin"],
        "feasibility_blockers": row["feasibility_blockers"],
        "governing_error_findings": row["governing_error_findings"],
    });
    let hex = |value: &Value| format!("{:016x}", fnv1a(value.to_string().as_bytes()));
    (hex(&search), hex(&rounded(&report)))
}

/// Floats as fixed 8-decimal strings (negative zero folded): the report side
/// sums masses over a `HashMap` (`design_gross_mass_kg`, `oew_kg`), whose
/// iteration order moves the last bit between processes (and a difference of
/// two such sums is then about 1e-11 instead of 0), so the report digest
/// compares to an absolute 1e-8 rather than to the bit. The search digest
/// stays bit-exact. A value within 1e-11 of a rounding boundary can still flip.
fn rounded(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => {
            let text = format!("{:.8}", number.as_f64().unwrap_or(f64::NAN));
            json!(text
                .strip_prefix('-')
                .filter(|t| t.chars().all(|c| c == '0' || c == '.'))
                .unwrap_or(&text))
        }
        Value::Array(items) => Value::Array(items.iter().map(rounded).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), rounded(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn stage<'a>(row: &'a Value, name: &str) -> Option<&'a Value> {
    row["optimizer"]["stages"]
        .as_array()?
        .iter()
        .find(|s| s["stage"] == name)
}

/// Wall time by pipeline stage of the two pipeline runs a cell makes (the
/// optimization run, then the native baseline run), read from the child's
/// `[stage]` event lines in `stderr.log`. A run starts at its `baseline` stage.
/// Returns `(optimize, baseline)`, each `null` when absent.
fn stage_timings(stderr: &Path) -> (Value, Value) {
    let text = fs::read_to_string(stderr).unwrap_or_default();
    let mut runs: Vec<(f64, BTreeMap<String, u64>)> = Vec::new();
    for line in text.lines().filter_map(|l| l.strip_prefix("[stage] ")) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["kind"] != "stage_completed" {
            continue;
        }
        let stage = event["stage"].as_str().unwrap_or("?").to_owned();
        if stage == "baseline" {
            runs.push((0.0, BTreeMap::new()));
        }
        let Some((elapsed, totals)) = runs.last_mut() else {
            continue;
        };
        *elapsed = elapsed.max(event["elapsed_ms"].as_f64().unwrap_or(0.0) / 1000.0);
        *totals.entry(stage).or_default() += event["duration_ms"].as_u64().unwrap_or(0);
    }
    let json_of = |run: Option<&(f64, BTreeMap<String, u64>)>| {
        run.map_or(
            Value::Null,
            |(elapsed, totals)| json!({"total_wall_time_s": elapsed, "stage_totals_ms": totals}),
        )
    };
    (json_of(runs.first()), json_of(runs.get(1)))
}

/// `primary` unless it is null, else `fallback`.
fn first_present(primary: &Value, fallback: &Value) -> Value {
    if primary.is_null() {
        fallback.clone()
    } else {
        primary.clone()
    }
}

/// One corner value of the model-mass payload-range chart of a reported
/// design (`validation.payload_range.model_masses.corners`), or null.
fn corner(side: &Value, name: &str, field: &str) -> Value {
    side["validation"]["payload_range"]["model_masses"]["corners"][name][field].clone()
}

fn cell_row(root: &Path, cell: &Cell, table: &BudgetTable) -> Value {
    let dir = cell_dir(root, cell);
    let runner = read_json(&dir.join("runner_cell.json")).unwrap_or(Value::Null);
    let row = read_json(&dir.join("measurement_result.json")).unwrap_or(Value::Null);
    let (optimize_pipeline, baseline_pipeline) = stage_timings(&dir.join("stderr.log"));
    let stage_json = |name: &str| {
        stage(&row, name).map_or(Value::Null, |s| {
            json!({
                "evaluations": s["evaluations"],
                "analysis_evaluations": s["analysis_evaluations"],
                "wall_time_s": s["wall_time_s"],
                "lane_utilization": s["lane_utilization"],
                "candidate_time_s": s["candidate_time_s"],
                "termination": s["termination"],
            })
        })
    };
    let status = if row.is_null() {
        runner["runner_status"]
            .as_str()
            .unwrap_or("not_run")
            .to_owned()
    } else {
        row["status"].as_str().unwrap_or("?").to_owned()
    };
    let baseline = &row["baseline_vs_optimized"];
    let like = &row["like_for_like"];
    let like_perf = &like["geometry_and_performance"];
    let only_perf = &row["baseline_only"]["geometry_and_performance"];
    let digests = (!row.is_null()).then(|| determinism_digests(&row));
    json!({
        "preset": cell.preset,
        "mode": cell.mode,
        "status": status,
        "runner_status": runner["runner_status"],
        "budget": {"screening": cell.screening, "refinement": cell.refinement,
                    "table_entry": table.has_entry(&cell.preset)},
        "workers": runner["workers"],
        "cell_wall_s": runner["child_wall_s"],
        "audit_wall_s": row["wall_time_s"],
        "optimize_pipeline": optimize_pipeline,
        "baseline_pipeline": baseline_pipeline,
        "screening": stage_json("screening"),
        "refinement": stage_json("refinement"),
        "termination": row["optimizer"]["termination"],
        "best_cost": row["optimizer"]["best_cost"],
        "delivered_feasible": row["optimizer"]["delivered_feasible"],
        // Mission fuel, never the weight-closure residual. `*_fuel_kg` is the
        // block fuel of the same sized mission on one mass basis (both designs
        // evaluated at the delivered design's takeoff mass by the same
        // model); `*_trip_fuel_kg` is the native segment mission's trip fuel at
        // each side's own mass.
        "baseline_fuel_kg": like["same_model_baseline"]["baseline"]["block_fuel_kg"],
        "optimized_fuel_kg": like["same_model_baseline"]["winner"]["block_fuel_kg"],
        "baseline_trip_fuel_kg": first_present(&like["native_mission"]["baseline"]["trip_fuel_kg"], &row["baseline_only"]["native_mission"]["trip_fuel_kg"]),
        "optimized_trip_fuel_kg": like["native_mission"]["winner"]["trip_fuel_kg"],
        // The closure residual MTOW - (OEW + payload) at each side's own
        // design gross mass: it moves with the MTOW the search chose, so it
        // is a mass-closure quantity and not a fuel comparison.
        "baseline_closure_residual_kg": first_present(&baseline["baseline"]["fuel_kg"], &row["baseline_only"]["fuel_kg"]),
        "optimized_closure_residual_kg": baseline["optimized"]["fuel_kg"],
        "baseline_design_gross_mass_kg": baseline["baseline"]["design_gross_mass_kg"],
        "optimized_design_gross_mass_kg": baseline["optimized"]["design_gross_mass_kg"],
        // Payload-range capability, reported next to the baseline's (no
        // requirement is imposed): corners B (max payload at MTOW) and C (max
        // fuel at MTOW) of the model-mass chart, nmi and kg.
        "baseline_range_b_nmi": first_present(&corner(&like_perf["baseline"], "B", "range_nmi"), &corner(only_perf, "B", "range_nmi")),
        "optimized_range_b_nmi": corner(&like_perf["winner"], "B", "range_nmi"),
        "baseline_range_c_nmi": first_present(&corner(&like_perf["baseline"], "C", "range_nmi"), &corner(only_perf, "C", "range_nmi")),
        "optimized_range_c_nmi": corner(&like_perf["winner"], "C", "range_nmi"),
        "baseline_payload_c_kg": first_present(&corner(&like_perf["baseline"], "C", "payload_kg"), &corner(only_perf, "C", "payload_kg")),
        "optimized_payload_c_kg": corner(&like_perf["winner"], "C", "payload_kg"),
        "baseline_mlw_kg": first_present(&like_perf["baseline"]["mlw_kg"], &only_perf["mlw_kg"]),
        "optimized_mlw_kg": like_perf["winner"]["mlw_kg"],
        "baseline_mtow_kg": first_present(&like_perf["baseline"]["mtow_kg"], &only_perf["mtow_kg"]),
        "optimized_mtow_kg": like_perf["winner"]["mtow_kg"],
        "baseline_l_over_d": first_present(&baseline["baseline"]["cruise_l_over_d"], &row["baseline_only"]["cruise_l_over_d"]),
        "optimized_l_over_d": baseline["optimized"]["cruise_l_over_d"],
        "physical_passed": row["physical_passed"],
        "feasibility_blockers": row["feasibility_blockers"],
        "informational_notes": row["informational_notes"],
        "governing_error_findings": row["governing_error_findings"],
        "digest": digests.as_ref().map_or(Value::Null, |d| json!(d.0)),
        "report_digest": digests.as_ref().map_or(Value::Null, |d| json!(d.1)),
    })
}

fn csv_field(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
            .collect::<Vec<_>>()
            .join(" | "),
        other => other.to_string(),
    };
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn csv(rows: &[Value]) -> String {
    const COLUMNS: [(&str, &str); 47] = [
        ("preset", "/preset"),
        ("mode", "/mode"),
        ("status", "/status"),
        ("cell_wall_s", "/cell_wall_s"),
        ("baseline_wall_s", "/baseline_pipeline/total_wall_time_s"),
        (
            "optimize_pipeline_wall_s",
            "/optimize_pipeline/total_wall_time_s",
        ),
        (
            "baseline_mission_ms",
            "/baseline_pipeline/stage_totals_ms/downstream~1mission",
        ),
        ("budget_screening", "/budget/screening"),
        ("budget_refinement", "/budget/refinement"),
        ("screening_evals", "/screening/evaluations"),
        ("screening_analyses", "/screening/analysis_evaluations"),
        ("screening_wall_s", "/screening/wall_time_s"),
        ("screening_lane_util", "/screening/lane_utilization"),
        ("screening_termination", "/screening/termination"),
        ("refinement_evals", "/refinement/evaluations"),
        ("refinement_analyses", "/refinement/analysis_evaluations"),
        ("refinement_wall_s", "/refinement/wall_time_s"),
        ("refinement_lane_util", "/refinement/lane_utilization"),
        ("refinement_termination", "/refinement/termination"),
        ("termination", "/termination"),
        ("best_cost", "/best_cost"),
        ("delivered_feasible", "/delivered_feasible"),
        ("baseline_fuel_kg", "/baseline_fuel_kg"),
        ("optimized_fuel_kg", "/optimized_fuel_kg"),
        ("baseline_trip_fuel_kg", "/baseline_trip_fuel_kg"),
        ("optimized_trip_fuel_kg", "/optimized_trip_fuel_kg"),
        (
            "baseline_closure_residual_kg",
            "/baseline_closure_residual_kg",
        ),
        (
            "optimized_closure_residual_kg",
            "/optimized_closure_residual_kg",
        ),
        (
            "baseline_design_gross_mass_kg",
            "/baseline_design_gross_mass_kg",
        ),
        (
            "optimized_design_gross_mass_kg",
            "/optimized_design_gross_mass_kg",
        ),
        ("baseline_mtow_kg", "/baseline_mtow_kg"),
        ("optimized_mtow_kg", "/optimized_mtow_kg"),
        ("baseline_mlw_kg", "/baseline_mlw_kg"),
        ("optimized_mlw_kg", "/optimized_mlw_kg"),
        ("baseline_range_b_nmi", "/baseline_range_b_nmi"),
        ("optimized_range_b_nmi", "/optimized_range_b_nmi"),
        ("baseline_range_c_nmi", "/baseline_range_c_nmi"),
        ("optimized_range_c_nmi", "/optimized_range_c_nmi"),
        ("baseline_payload_c_kg", "/baseline_payload_c_kg"),
        ("optimized_payload_c_kg", "/optimized_payload_c_kg"),
        ("baseline_l_over_d", "/baseline_l_over_d"),
        ("optimized_l_over_d", "/optimized_l_over_d"),
        ("physical_passed", "/physical_passed"),
        ("findings", "/feasibility_blockers"),
        ("digest", "/digest"),
        ("report_digest", "/report_digest"),
        ("notes", "/informational_notes"),
    ];
    let mut out = COLUMNS.map(|(name, _)| name).join(",");
    out.push('\n');
    for row in rows {
        let line: Vec<String> = COLUMNS
            .iter()
            .map(|(_, pointer)| csv_field(row.pointer(pointer).unwrap_or(&Value::Null)))
            .collect();
        let _ = writeln!(out, "{}", line.join(","));
    }
    out
}

/// Counts returned to the caller.
pub struct Counts {
    pub total: usize,
    pub success: usize,
}

/// Rebuild `matrix_summary.json` and `.csv` from the cell directories.
/// `invocation` (utilization and wall of this run) is kept from the previous
/// summary when `None`.
pub fn write(
    root: &Path,
    cells: &[Cell],
    table: &BudgetTable,
    invocation: Option<Value>,
) -> io::Result<Counts> {
    let mut ordered: Vec<&Cell> = cells.iter().collect();
    ordered.sort_by_key(|cell| cell.id());
    let rows: Vec<Value> = ordered.iter().map(|c| cell_row(root, c, table)).collect();
    let success = rows
        .iter()
        .filter(|r| r["status"].as_str().is_some_and(is_completed))
        .count();
    let mut by_status = BTreeMap::<String, usize>::new();
    for row in &rows {
        let status = row["status"].as_str().unwrap_or("?").to_owned();
        *by_status.entry(status).or_default() += 1;
    }
    let json_path = root.join("matrix_summary.json");
    let invocation = invocation
        .or_else(|| read_json(&json_path).map(|old| old["invocation"].clone()))
        .unwrap_or(Value::Null);
    let document = json!({
        "kind": "native_matrix_summary",
        "budget_table": table.name,
        "budget_status": table.status,
        "budget_provenance": table.provenance,
        "seed": table.seed,
        "cells_total": rows.len(),
        "cells_completed": success,
        "cells_by_status": by_status,
        "completed_definition": "status success or infeasible: the search ran to its evaluation budget; an infeasible outcome is a result, not a runner failure",
        "invocation": invocation,
        "digest_definition": "digest: FNV-1a over the bit-exact search result (status, termination, best cost, per-stage counts); report_digest: FNV-1a over the whole load-independent row with floats rounded to 8 decimals (the report sums masses over a HashMap, so its last bit varies between processes). Equal digests at different P x W confirm identical results",
        "cells": rows,
    });
    fs::write(
        &json_path,
        serde_json::to_vec_pretty(&document).map_err(io::Error::other)?,
    )?;
    fs::write(
        root.join("matrix_summary.csv"),
        csv(document["cells"].as_array().map_or(&[][..], Vec::as_slice)),
    )?;
    Ok(Counts {
        total: ordered.len(),
        success,
    })
}

/// `matrix_native compare`: cells whose digest differs between two summaries.
pub fn compare(a: &str, b: &str) -> io::Result<()> {
    let load = |path: &str| {
        read_json(Path::new(path)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("cannot read {path}"))
        })
    };
    let (a, b) = (load(a)?, load(b)?);
    let key = |c: &Value| {
        format!(
            "{}@{}",
            c["preset"].as_str().unwrap_or("?"),
            c["mode"].as_str().unwrap_or("?")
        )
    };
    let mut differing = 0;
    let mut compared = 0;
    for cell in a["cells"].as_array().into_iter().flatten() {
        let other = b["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|o| key(o) == key(cell));
        let Some(other) = other else { continue };
        // A cell without a result (timeout, error) has no digest to compare.
        if cell["digest"].is_null() || other["digest"].is_null() {
            println!("SKIPPED {} (no result in one summary)", key(cell));
            continue;
        }
        compared += 1;
        if cell["digest"] != other["digest"] || cell["report_digest"] != other["report_digest"] {
            differing += 1;
            println!(
                "DIFFERS {} search {} vs {}, report {} vs {}",
                key(cell),
                cell["digest"],
                other["digest"],
                cell["report_digest"],
                other["report_digest"]
            );
        }
    }
    println!("compared {compared} cells, {differing} differ");
    if differing > 0 || compared == 0 {
        std::process::exit(1);
    }
    Ok(())
}

// Tests assert on fixtures they built, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_ignores_timing_and_paths_but_not_results() {
        let base = json!({"status": "success", "optimizer": {"best_cost": 1.5, "stages": [
            {"stage": "screening", "evaluations": 10, "wall_time_s": 1.0}]}, "output_dir": "a"});
        let mut other = base.clone();
        other["optimizer"]["stages"][0]["wall_time_s"] = json!(9.0);
        other["output_dir"] = json!("b");
        assert_eq!(determinism_digests(&base), determinism_digests(&other));
        other["optimizer"]["best_cost"] = json!(1.5000000000000002);
        assert_ne!(determinism_digests(&base).0, determinism_digests(&other).0);
    }

    #[test]
    fn the_report_digest_tolerates_the_last_bit_of_a_float() {
        let base = json!({"status": "success", "oew_kg": 40880.36854876989});
        let mut other = base.clone();
        other["oew_kg"] = json!(40880.368548769904);
        assert_ne!(base["oew_kg"], other["oew_kg"]);
        assert_eq!(determinism_digests(&base), determinism_digests(&other));
        other["oew_kg"] = json!(40880.4);
        assert_ne!(determinism_digests(&base).1, determinism_digests(&other).1);
    }

    #[test]
    fn fuel_columns_are_mission_fuel_and_the_closure_residual_is_named_apart() {
        let corners = |b: f64, c: f64, pc: f64| {
            json!({"validation": {"payload_range": {"model_masses": {"corners": {
                "B": {"range_nmi": b}, "C": {"range_nmi": c, "payload_kg": pc}}}}}})
        };
        let mut baseline_side = corners(2249.0, 3533.0, 13186.0);
        baseline_side["mlw_kg"] = json!(67800.0);
        let mut winner_side = corners(996.0, 3990.0, 6452.0);
        winner_side["mlw_kg"] = json!(67770.0);
        let row = json!({
            "status": "success",
            "baseline_vs_optimized": {
                "baseline": {"fuel_kg": 16568.0, "design_gross_mass_kg": 75100.0},
                "optimized": {"fuel_kg": 9885.0, "design_gross_mass_kg": 67770.0}},
            "like_for_like": {
                "same_model_baseline": {
                    "baseline": {"block_fuel_kg": 3987.0},
                    "winner": {"block_fuel_kg": 3920.0}},
                "native_mission": {
                    "baseline": {"trip_fuel_kg": 3945.0},
                    "winner": {"trip_fuel_kg": 3842.0}},
                "geometry_and_performance": {"baseline": baseline_side, "winner": winner_side}}
        });
        let root = std::env::temp_dir().join("matrix_native_summary_test");
        let cell = Cell {
            preset: "X".to_owned(),
            mode: "mtow_band".to_owned(),
            screening: 1,
            refinement: 1,
            weight: 1.0,
        };
        let dir = cell_dir(&root, &cell);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("measurement_result.json"), row.to_string()).unwrap();
        let table = BudgetTable::load("matrix").unwrap();
        let out = cell_row(&root, &cell, &table);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(out["baseline_fuel_kg"], json!(3987.0));
        assert_eq!(out["optimized_fuel_kg"], json!(3920.0));
        assert_eq!(out["baseline_trip_fuel_kg"], json!(3945.0));
        assert_eq!(out["optimized_trip_fuel_kg"], json!(3842.0));
        assert_eq!(out["baseline_closure_residual_kg"], json!(16568.0));
        assert_eq!(out["optimized_closure_residual_kg"], json!(9885.0));
        assert_eq!(out["baseline_range_b_nmi"], json!(2249.0));
        assert_eq!(out["optimized_range_b_nmi"], json!(996.0));
        assert_eq!(out["optimized_payload_c_kg"], json!(6452.0));
        assert_eq!(out["optimized_mlw_kg"], json!(67770.0));
    }

    #[test]
    fn csv_quotes_fields_with_commas() {
        assert_eq!(csv_field(&json!("a,b")), "\"a,b\"");
        assert_eq!(csv_field(&json!(["x", "y"])), "x | y");
        assert_eq!(csv_field(&Value::Null), "");
    }
}
