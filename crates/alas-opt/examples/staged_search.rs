// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run the product search alone (no pipeline, no reporting analysis) on a run
//! configuration and print what each stage did.
//!
//! ```text
//! staged_search CONFIG.json OUTPUT.json [--screening N] [--refinement N]
//!     [--workers N] [--seed N] [--set dotted.path=JSON]...
//! ```
//!
//! The search stops on its evaluation counts only, so the same arguments
//! examine the same candidates on every machine and at every worker count;
//! the printed `result_digest` hashes the whole history and the winner for a
//! bit-for-bit comparison between two runs. Wall times belong to the host.

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_opt::DesignOptimizer;
use serde_json::{json, Value};
use std::{error::Error, fs, time::Instant};

fn overlay(document: &mut Value, path: &str, value: Value) {
    let mut node = document;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if !node.is_object() {
            *node = json!({});
        }
        let Some(map) = node.as_object_mut() else {
            return;
        };
        if parts.peek().is_none() {
            map.insert(part.to_owned(), value);
            return;
        }
        node = map.entry(part.to_owned()).or_insert_with(|| json!({}));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: staged_search CONFIG.json OUTPUT.json [flags]".into());
    }
    let mut document: Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let (mut screening, mut refinement, mut workers, mut seed) = (None, None, None, None);
    let mut index = 2;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--screening" => screening = Some(value.parse::<i64>()?),
            "--refinement" => refinement = Some(value.parse::<i64>()?),
            "--workers" => workers = Some(value.parse::<i64>()?),
            "--seed" => seed = Some(value.parse::<i64>()?),
            "--set" => {
                let (path, text) = value
                    .split_once('=')
                    .ok_or("--set requires dotted.path=JSON")?;
                overlay(&mut document, path, serde_json::from_str(text)?);
            }
            other => return Err(format!("unknown flag {other}").into()),
        }
        index += 2;
    }
    let (mut config, _) = alas_mission::preset_load::load_document(&document)
        .map_err(|error| format!("invalid configuration: {error:?}"))?;
    let solver = &mut config.optimizer.solver;
    solver.stop_on_evaluations_only = true;
    if let Some(count) = screening {
        solver.screening.max_evaluations = count;
    }
    if let Some(count) = refinement {
        solver.refinement.max_evaluations = count;
    }
    if let Some(count) = workers {
        solver.workers = count;
    }
    if let Some(value) = seed {
        solver.seed = Some(value);
    }
    solver.validate_budgets()?;
    let nominal = alas_config::presets::get(&config.preset)?.design_vector;
    let started = Instant::now();
    let mut lines = Vec::new();
    let mut on_progress = |line: &str| {
        println!("[{:7.1} s] {line}", started.elapsed().as_secs_f64());
        lines.push(line.to_owned());
    };
    let outcome = DesignOptimizer::new(config.clone()).run_diagnostics(
        None,
        Some(&nominal),
        Some(&mut on_progress),
    )?;
    let wall_s = started.elapsed().as_secs_f64();
    let result = outcome.evidence();
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(serde_json::to_string(&result.history)?.as_bytes());
    feed(&result.best_cost.to_bits().to_le_bytes());
    for value in result.best_design.to_array() {
        feed(&value.to_bits().to_le_bytes());
    }
    let diagnostics = result.search_diagnostics.as_ref();
    println!(
        "wall {wall_s:.1} s | termination {} | best_valid {} | digest {digest:016x}",
        result.termination, result.best_valid
    );
    if let Some(diagnostics) = diagnostics {
        for stage in &diagnostics.stages {
            println!(
                "  {:10} evaluations {:4} analyses {:4} feasible {:4} pre-gate {:4} generations {:3} wall {:7.1} s lane {:6.2} s utilization {:.2} termination {}",
                stage.stage, stage.evaluations, stage.analysis_evaluations, stage.feasible,
                stage.pre_gate_rejects, stage.generations, stage.wall_time_s,
                stage.candidate_time_s, stage.lane_utilization, stage.termination
            );
        }
        if let Some(baseline) = &diagnostics.baseline {
            println!(
                "  baseline feasible {} objective {:.4} -> winner {:.4} ({:+.3} %)",
                baseline.baseline_feasible,
                baseline.baseline_objective_value,
                baseline.winner_objective_value,
                baseline
                    .relative_change
                    .map_or(f64::NAN, |change| 100.0 * change)
            );
        }
    }
    let nominal_values = nominal.to_array();
    let winner = result.best_design.to_array();
    for ((spec, base), value) in alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .zip(&nominal_values)
        .zip(&winner)
    {
        if (value - base).abs() > 1e-12 {
            println!(
                "  {:28} {:12.5} -> {:12.5} ({:+.2} %)",
                spec.name,
                base,
                value,
                if base.abs() > 1e-9 {
                    100.0 * (value / base - 1.0)
                } else {
                    f64::NAN
                }
            );
        }
    }
    let document = json!({
        "tool": "alas-opt example staged_search", "config": args[0],
        "wall_s": wall_s, "result_digest": format!("{digest:016x}"),
        "progress": lines, "result": result,
    });
    if let Some(parent) = std::path::Path::new(&args[1]).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args[1], serde_json::to_string(&document)?)?;
    Ok(())
}
