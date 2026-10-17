// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Equivalence harness for the evaluation speed-ups of phase 2 lane L5a.
//!
//! Evaluates, at full fidelity, the registered nominal design of each preset
//! and, optionally, the first N design vectors of a search history written by
//! `eval_profile` (the screening population), and prints one JSON record per
//! evaluation: closed takeoff mass, block fuel, cost, hard feasibility and
//! every residual's achieved value (the takeoff field length is the
//! `takeoff_field` residual).
//!
//! Which levers are active comes from `ALAS_SPEEDUPS_OFF` (comma-separated
//! `nominal_key,gear_reuse,warm_start` or `all`); run it once per variant and
//! compare the JSON files with `lever_compare.py`.
//!
//! Usage:
//!   lever_replay PRESET|all MTOW_MODE OUTPUT_JSON [HISTORY_JSON ROWS]

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_config::{AlasConfig, DesignVector, MtowSizing};
use alas_opt::mdo::assess_product_candidate;
use alas_opt::mdo::speedups::{self, Lever};
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

fn record(label: &str, config: &AlasConfig, design: &DesignVector) -> Value {
    let start = Instant::now();
    let assessed = assess_product_candidate(config, design);
    let wall_s = start.elapsed().as_secs_f64();
    match assessed {
        Ok(a) => json!({
            "label": label, "ok": true, "wall_s": wall_s,
            "takeoff_mass_kg": a.sized.takeoff_mass_kg,
            "block_fuel_kg": a.sized.block_fuel_kg,
            "cost": a.cost, "hard_feasible": a.hard_feasible,
            "hard_violation_sum": a.hard_violation_sum,
            "hard_violations": a.violated_hard_ids(),
            "residuals": a.residuals.iter()
                .map(|r| (r.id.to_owned(), json!(r.actual)))
                .collect::<serde_json::Map<_, _>>(),
        }),
        Err(reason) => json!({"label": label, "ok": false, "reason": reason, "wall_s": wall_s}),
    }
}

/// A seeded Latin-hypercube sample of the reference-adaptation design box
/// (the screening population's domain): the same points for every lever
/// variant, so the variants can be ranked against each other.
fn sampled_designs(
    config: &AlasConfig,
    preset: &str,
    count: usize,
) -> Result<Vec<Vec<f64>>, Box<dyn Error>> {
    let nominal = alas_config::presets::get(preset)?.design_vector;
    let bounds = config.optimizer.design_space.bounds(&nominal);
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut uniform = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 11) as f64) / ((1_u64 << 53) as f64)
    };
    let mut columns: Vec<Vec<f64>> = Vec::new();
    for (low, high) in &bounds {
        let mut strata: Vec<usize> = (0..count).collect();
        for i in (1..count).rev() {
            let j = (uniform() * (i + 1) as f64) as usize;
            strata.swap(i, j.min(i));
        }
        columns.push(
            strata
                .iter()
                .map(|k| low + (high - low) * ((*k as f64 + uniform()) / count as f64))
                .collect(),
        );
    }
    Ok((0..count)
        .map(|row| columns.iter().map(|column| column[row]).collect())
        .collect())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err(
            "usage: lever_replay PRESET|all MTOW_MODE OUTPUT_JSON [HISTORY_JSON ROWS]".into(),
        );
    }
    let mode = parse_mode(&args[2])?;
    let presets: Vec<String> = if args[1] == "all" {
        alas_config::presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        vec![args[1].clone()]
    };
    let mut records = Vec::new();
    for preset in &presets {
        let mut config = AlasConfig::from_value(&json!({ "preset": preset }))?;
        config.optimizer.objective.mtow_sizing = mode;
        let nominal = alas_config::presets::get(preset)?.design_vector;
        records.push(json!({"preset": preset, "nominal": record("nominal", &config, &nominal)}));
        println!("{preset}: nominal done");
    }
    let mut rows = Vec::new();
    if let (Some(path), Some(count)) = (args.get(4), args.get(5)) {
        let count: usize = count.parse()?;
        let mut config = AlasConfig::from_value(&json!({ "preset": args[1] }))?;
        config.optimizer.objective.mtow_sizing = mode;
        let designs: Vec<Vec<f64>> = if path == "sample" {
            sampled_designs(&config, &args[1], count)?
        } else {
            let document: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
            document["history_designs"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .take(count)
                        .map(|design| {
                            design
                                .as_array()
                                .map(|a| a.iter().filter_map(Value::as_f64).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        for (index, values) in designs.iter().enumerate() {
            let Ok(dv) = DesignVector::from_array(values) else {
                continue;
            };
            rows.push(record(&format!("row{index}"), &config, &dv));
        }
    }
    let levers: serde_json::Map<String, Value> = Lever::ALL
        .iter()
        .map(|lever| (lever.name().to_owned(), json!(speedups::enabled(*lever))))
        .collect();
    let text = serde_json::to_string_pretty(&json!({
        "mode": args[2], "levers": levers, "nominals": records, "rows": rows,
    }))?;
    fs::write(&args[3], &text)?;
    Ok(())
}
