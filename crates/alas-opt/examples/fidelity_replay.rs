// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Re-score recorded design vectors at a chosen evaluation fidelity.
//!
//! Reads a run configuration document and a list of design vectors (for
//! example the history of a finished search), evaluates every vector with one
//! evaluation model and writes each candidate's cost, feasibility, objective,
//! residual table, wall time and closure work, plus the stage-time split of
//! the whole replay. Two replays of the same vectors at two fidelities give
//! the rank correlation, the per-row false-feasible and false-infeasible
//! rates and the bias a fidelity ladder is judged on. This is a
//! model-to-model consistency measurement, not physical validation.
//!
//! ```text
//! fidelity_replay CONFIG.json DESIGNS.json OUTPUT.json [--chord N] [--steps N]
//!     [--induced screening|full] [--engine held|sized] [--fuselage held|sized] [--engine-start FACTOR] [--engine-slope SLOPE] [--rotated-tail on|off] [--tol KG] [--workers N] [--uncapped]
//!     [--set dotted.path=JSON]... [--limit N]
//! ```
//!
//! `DESIGNS.json` is a JSON array of design vectors (arrays of numbers, in
//! design-vector order). `--set` overlays one configuration field on the
//! document before it is loaded (for both the work-cap nominal and the
//! replay model). Times are wall-clock seconds on the host that ran them;
//! with one worker the stage table is a wall-clock split, with several the
//! fractions only are meaningful.

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_opt::{anchored_box, score_designs, stage_profile, ScreeningFidelity};
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
    if args.len() < 3 {
        return Err("usage: fidelity_replay CONFIG.json DESIGNS.json OUTPUT.json [flags]".into());
    }
    let mut document: Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let mut designs: Vec<Vec<f64>> = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let mut fidelity = ScreeningFidelity::full();
    let mut workers = 1usize;
    let mut capped = true;
    let mut sets = Vec::new();
    let mut index = 3;
    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--uncapped" {
            capped = false;
            index += 1;
            continue;
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--chord" => fidelity.chordwise_resolution = Some(value.parse()?),
            "--steps" => fidelity.steps_per_segment = Some(value.parse()?),
            "--tol" => fidelity.sizing_tolerance_kg = Some(value.parse()?),
            "--wing" => fidelity.wing_subdivisions = Some(value.parse()?),
            "--emp" => fidelity.empennage_subdivisions = Some(value.parse()?),
            "--induced" => fidelity.reduced_induced_checks = value == "screening",
            "--engine" => fidelity.held_engine = value == "held",
            "--fuselage" => fidelity.held_fuselage = value == "held",
            "--engine-start" => fidelity.engine_start = Some(value.parse()?),
            "--engine-slope" => fidelity.engine_slope = Some(value.parse()?),
            "--rotated-tail" => fidelity.rotated_tail_nodes = value == "on",
            "--workers" => workers = value.parse()?,
            "--limit" => designs.truncate(value.parse()?),
            "--set" => {
                let (path, text) = value
                    .split_once('=')
                    .ok_or("--set requires dotted.path=JSON")?;
                overlay(&mut document, path, serde_json::from_str(text)?);
                sets.push(value.clone());
            }
            other => return Err(format!("unknown flag {other}").into()),
        }
        index += 2;
    }
    let (config, _) = alas_mission::preset_load::load_document(&document)
        .map_err(|error| format!("invalid configuration: {error:?}"))?;
    let (bounds, nominal) = anchored_box(&config)?;
    if designs.is_empty() {
        // No vectors: the search box alone, for a study that proposes its own.
        designs.push(nominal.clone());
    }
    stage_profile::enable();
    stage_profile::reset();
    let started = Instant::now();
    let scores = score_designs(&config, fidelity, &designs, workers, capped)?;
    let wall_s = started.elapsed().as_secs_f64();
    let report = stage_profile::report();
    let total_s = report.total_s().max(f64::MIN_POSITIVE);
    let candidates: Vec<Value> = designs
        .iter()
        .zip(&scores)
        .map(|(design, score)| {
            json!({
                "design": design,
                "cost": score.cost,
                "feasible": score.feasible,
                "analysed": score.analysed,
                "hard_violation": score.hard_violation,
                "objective": score.objective_value,
                "takeoff_mass_kg": score.takeoff_mass_kg,
                "failure": score.failure_reason,
                "wall_s": score.wall_time_s,
                "trip_flights": score.work.map(|work| work.trip_flights),
                "deck_evals": score.work.map(|work| work.deck_evals),
                "residuals": score.residuals.iter().map(|residual| json!({
                    "id": residual.id,
                    "actual": residual.actual,
                    "limit": residual.limit,
                    "raw": residual.raw_residual,
                    "violation": residual.normalized_violation,
                    "role": format!("{:?}", residual.role),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let document = json!({
        "tool": "alas-opt example fidelity_replay",
        "config": args[0], "designs": args[1],
        "fidelity": {
            "chordwise_resolution": fidelity.chordwise_resolution,
            "steps_per_segment": fidelity.steps_per_segment,
            "sizing_tolerance_kg": fidelity.sizing_tolerance_kg,
            "reduced_induced_checks": fidelity.reduced_induced_checks,
            "overlays": sets,
        },
        "workers": workers, "capped": capped,
        "bounds": bounds, "nominal": nominal,
        "wall_s": wall_s, "profiled_total_s": report.total_s(),
        "stages": report.stages.iter().map(|(key, seconds, calls, inclusive_s)| json!({
            "stage": key, "seconds": seconds, "fraction": seconds / total_s,
            "inclusive_seconds": inclusive_s, "calls": calls,
        })).collect::<Vec<_>>(),
        "counters": report.counters.iter().map(|(key, count)| (key.to_string(), json!(count)))
            .collect::<serde_json::Map<_, _>>(),
        "candidates": candidates,
    });
    if let Some(parent) = std::path::Path::new(&args[2]).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args[2], serde_json::to_string(&document)?)?;
    let feasible = scores.iter().filter(|score| score.feasible).count();
    println!(
        "{} candidates, {feasible} feasible, wall {wall_s:.1} s, profiled {:.1} s",
        scores.len(),
        report.total_s()
    );
    for (key, seconds, calls, _) in &report.stages {
        if *seconds > 0.0 {
            println!(
                "  {key:34} {seconds:9.2} s {:5.1} %  calls {calls}",
                100.0 * seconds / total_s
            );
        }
    }
    for (key, count) in &report.counters {
        println!("  {key:34} {count}");
    }
    Ok(())
}
