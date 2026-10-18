// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where one optimizer evaluation spends its time: a repeatable stage split.
//!
//! Runs the real coupled search (screening then refinement, product search)
//! for a fixed evaluation budget with one worker and reports the exclusive
//! wall time of each evaluation stage from `alas_opt::stage_profile`, the
//! event counters (closures, closure passes, lattice assemblies and solves,
//! trip flights) and the per-evaluation means.
//!
//! Usage:
//!   eval_profile PRESET MTOW_MODE EVALUATIONS [OUTPUT_JSON] [SEED] [off]
//! A trailing `off` leaves the profile disabled (the stage table is then
//! empty); the printed `result_digest` hashes every evaluation of the search
//! history plus the winner, so a run with and without the profile can be
//! compared bit for bit.
//! MTOW_MODE is `fixed_requirement`, `mtow_band` or `payload_adjusted` (the
//! three user-facing sizing modes). EVALUATIONS is split evenly between the
//! screening and refinement stages and stops on the evaluation count only, so
//! the same arguments evaluate the same candidates on every machine. Absolute
//! seconds belong to the host that ran them (note the load); the fractions
//! are the finding.
//!
//! Build with `--profile harness`. The profile adds one relaxed atomic load
//! per instrumented scope when off and never touches a numerical value.

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_config::{solver_presets, AlasConfig, MtowSizing};
use alas_opt::stage_profile;
use alas_opt::DesignOptimizer;
use serde_json::{json, Value};
use std::{error::Error, fs, time::Instant};

fn parse_mode(text: &str) -> Result<MtowSizing, String> {
    match text {
        "fixed_requirement" => Ok(MtowSizing::FixedRequirement),
        "mtow_band" => Ok(MtowSizing::MtowBand),
        "payload_adjusted" => Ok(MtowSizing::PayloadAdjusted),
        other => Err(format!("unknown MTOW mode {other:?}")),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: eval_profile PRESET MTOW_MODE EVALUATIONS [OUTPUT_JSON] [SEED]".into());
    }
    let preset = args[1].as_str();
    let mode = parse_mode(&args[2])?;
    let evaluations: i64 = args[3].parse()?;
    let seed: i64 = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(7);
    if evaluations < 2 {
        return Err("EVALUATIONS must be at least 2".into());
    }

    // `ALAS_EVAL_CONFIG=path.json` starts from a full configuration file (for
    // example a matrix cell's route-initialised `input_config.json`) instead
    // of the bare preset; PRESET still names the nominal design vector.
    let mut config = match std::env::var("ALAS_EVAL_CONFIG") {
        Ok(path) => {
            AlasConfig::from_value(&serde_json::from_str::<Value>(&fs::read_to_string(path)?)?)?
        }
        Err(_) => AlasConfig::from_value(&json!({ "preset": preset }))?,
    };
    let mut settings = solver_presets::get("quick_draft")?.settings.clone();
    settings.screening.max_evaluations = evaluations / 2;
    settings.refinement.max_evaluations = evaluations - evaluations / 2;
    settings.stop_on_evaluations_only = true;
    settings.workers = 1;
    settings.seed = Some(seed);
    settings.validate_budgets()?;
    config.optimizer.solver = settings;
    config.optimizer.objective.mtow_sizing = mode;
    let nominal = alas_config::presets::get(&config.preset)?.design_vector;

    let profile_on = args.get(6).map(String::as_str) != Some("off");
    if profile_on {
        stage_profile::enable();
    }
    stage_profile::reset();
    let start = Instant::now();
    // The screening summary line marks the stage boundary: snapshot there so
    // the screening and refinement (+ verification) splits can be reported.
    let mut screening_end: Option<stage_profile::Report> = None;
    let mut on_progress = |line: &str| {
        if line.starts_with("screening |") {
            screening_end = Some(stage_profile::report());
        }
    };
    let result = DesignOptimizer::new(config.clone()).run_diagnostics(
        None,
        Some(&nominal),
        Some(&mut on_progress),
    );
    let wall_s = start.elapsed().as_secs_f64();
    let report = stage_profile::report();
    let outcome = match &result {
        Ok(_) => "ok".to_owned(),
        Err(error) => error.to_string(),
    };

    // FNV-1a over the serialized history (shortest round-trip floats, so equal
    // text means equal bits) and the winner's bits.
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    if let Ok(outcome) = &result {
        let evidence = outcome.evidence();
        feed(serde_json::to_string(&evidence.history)?.as_bytes());
        feed(&evidence.best_cost.to_bits().to_le_bytes());
        for value in evidence.best_design.to_array() {
            feed(&value.to_bits().to_le_bytes());
        }
    }
    // Why candidates were rejected (A6: which failures a cheaper check could
    // have caught before the closure). Tokens are the hard-violation ids a
    // history row joins with `+`; every evaluation of the search is counted.
    let mut rejection_tokens = std::collections::BTreeMap::<String, u64>::new();
    let (mut rows, mut valid_rows, mut sole_reason_rows) = (0_u64, 0_u64, 0_u64);
    if let Ok(outcome) = &result {
        let history = &outcome.evidence().history;
        for (valid, reason) in history.valid.iter().zip(&history.reject_reason) {
            rows += 1;
            valid_rows += u64::from(*valid);
            let tokens: Vec<&str> = reason.split('+').filter(|t| !t.is_empty()).collect();
            sole_reason_rows += u64::from(!valid && tokens.len() == 1);
            for token in tokens {
                *rejection_tokens.entry(token.to_owned()).or_default() += 1;
            }
        }
    }
    let split = |from: Option<&stage_profile::Report>, to: &stage_profile::Report| {
        let seconds = |key: &str, report: &stage_profile::Report| {
            report
                .stages
                .iter()
                .find(|s| s.0 == key)
                .map_or(0.0, |s| s.1)
        };
        let count = |key: &str, report: &stage_profile::Report| {
            report
                .counters
                .iter()
                .find(|c| c.0 == key)
                .map_or(0, |c| c.1)
        };
        let total: f64 = to.total_s() - from.map_or(0.0, |r| r.total_s());
        let evals = count("evaluations", to) - from.map_or(0, |r| count("evaluations", r));
        json!({
            "evaluations": evals,
            "ms_per_evaluation": 1.0e3 * total / (evals.max(1) as f64),
            "fraction": to.stages.iter().map(|s| {
                let value = s.1 - from.map_or(0.0, |r| seconds(s.0, r));
                (s.0.to_string(), json!(value / total.max(f64::MIN_POSITIVE)))
            }).collect::<serde_json::Map<_, _>>(),
            "closures_per_evaluation": (count("closures", to) - from.map_or(0, |r| count("closures", r))) as f64 / (evals.max(1) as f64),
            "retrims_per_evaluation": (count("retrims", to) - from.map_or(0, |r| count("retrims", r))) as f64 / (evals.max(1) as f64),
            "vlm_assemblies_per_evaluation": (count("vlm_assemblies", to) - from.map_or(0, |r| count("vlm_assemblies", r))) as f64 / (evals.max(1) as f64),
        })
    };
    let stage_split = json!({
        "screening": screening_end.as_ref().map(|end| split(None, end)),
        "refinement_and_verification": screening_end.as_ref().map(|end| split(Some(end), &report)),
    });
    let total = report.total_s().max(f64::MIN_POSITIVE);
    let counter = |key: &str| {
        report
            .counters
            .iter()
            .find(|c| c.0 == key)
            .map_or(0, |c| c.1)
    };
    let evals = counter("evaluations").max(1) as f64;
    let stages: Vec<Value> = report
        .stages
        .iter()
        .map(|(key, seconds, calls, inclusive_s)| {
            json!({
                "stage": key, "seconds": seconds, "fraction": seconds / total,
                "inclusive_seconds": inclusive_s,
                "calls": calls, "ms_per_evaluation": 1.0e3 * seconds / evals,
            })
        })
        .collect();
    let document = json!({
        "tool": "alas-opt example eval_profile",
        "preset": preset, "mtow_mode": args[2], "evaluations_requested": evaluations,
        "seed": seed, "workers": 1, "search_outcome": outcome,
        "profile_enabled": profile_on, "result_digest": format!("{digest:016x}"),
        "host_note": "absolute times are from a shared host; use the fractions",
        "search_wall_s": wall_s, "profiled_total_s": report.total_s(),
        "coverage": report.total_s() / wall_s,
        "stages": stages, "stage_split": stage_split,
        "levers": alas_opt::mdo::speedups::Lever::ALL.iter()
            .map(|l| (l.name().to_owned(), json!(alas_opt::mdo::speedups::enabled(*l))))
            .collect::<serde_json::Map<_, _>>(),
        "history_cost": result.as_ref().ok().map(|o| o.evidence().history.cost.clone()),
        "history_valid": result.as_ref().ok().map(|o| o.evidence().history.valid.clone()),
        "history_mass_kg": result.as_ref().ok().map(|o| o.evidence().history.takeoff_mass_kg.clone()),
        "history_fuel_kg": result.as_ref().ok().map(|o| o.evidence().history.block_fuel_kg.clone()),
        "history_designs": result.as_ref().ok().map(|o| o.evidence().history.design_vectors.iter()
            .map(|d| d.as_ref().map(|d| d.to_array())).collect::<Vec<_>>()),
        "best_cost": result.as_ref().ok().map(|o| o.evidence().best_cost),
        "history_rows": rows, "history_valid_rows": valid_rows,
        "history_invalid_with_single_reason": sole_reason_rows,
        "rejection_tokens": rejection_tokens,
        "counters": report.counters.iter().map(|(k, v)| (k.to_string(), json!(v)))
            .collect::<serde_json::Map<_, _>>(),
        "per_evaluation": {
            "ms": 1.0e3 * report.total_s() / evals,
            "closures": counter("closures") as f64 / evals,
            "closure_passes": counter("closure_passes") as f64 / evals,
            "retrims": counter("retrims") as f64 / evals,
            "vlm_assemblies": counter("vlm_assemblies") as f64 / evals,
            "vlm_refactorizations": counter("vlm_refactorizations") as f64 / evals,
            "vlm_solves": counter("vlm_solves") as f64 / evals,
            "trip_flights": counter("trip_flights") as f64 / evals,
            "deck_evals": counter("deck_evals") as f64 / evals,
        },
    });
    let text = serde_json::to_string_pretty(&document)?;
    if let Some(path) = args.get(4).filter(|p| !p.is_empty()) {
        if let Some(parent) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &text)?;
    }
    println!("{text}");
    Ok(())
}
