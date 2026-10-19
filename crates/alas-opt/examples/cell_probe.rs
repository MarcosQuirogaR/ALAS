// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why one matrix cell's candidates fail: every violated hard residual with
//! its achieved value and limit, for the preset nominal and for chosen rows of
//! a search history.
//!
//! Usage:
//!   cell_probe INPUT_CONFIG_JSON MTOW_MODE [SEARCH_JSON ROWS]
//! `INPUT_CONFIG_JSON` is a cell's `input_config.json` (route-initialised);
//! `SEARCH_JSON` is the cell's `optimization_search.json`; `ROWS` is a
//! comma-separated list of history row indices (or `best` for the row with the
//! smallest hard violation).

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_config::{AlasConfig, DesignVector, MtowSizing};
use alas_opt::mdo::assess_product_candidate;
use serde_json::Value;
use std::{error::Error, fs};

fn parse_mode(text: &str) -> Result<MtowSizing, String> {
    match text {
        "fixed_requirement" => Ok(MtowSizing::FixedRequirement),
        "mtow_band" => Ok(MtowSizing::MtowBand),
        "payload_adjusted" => Ok(MtowSizing::PayloadAdjusted),
        other => Err(format!("unknown MTOW mode {other:?}")),
    }
}

fn nominal_of(config: &AlasConfig) -> DesignVector {
    config.configured_nominal_design()
}

fn report(label: &str, config: &AlasConfig, design: &DesignVector) {
    // The optimizer's own objective: the registered preset vector is the
    // nominal that reference-adaptation anchors (tail volumes, bounds) read.
    let _ = nominal_of(config);
    match assess_product_candidate(config, design) {
        Ok(a) => {
            println!(
                "{label}: tow {:.0} kg, block fuel {:.0} kg, hard_feasible {}, hard_violation_sum {:.5}",
                a.sized.takeoff_mass_kg, a.sized.block_fuel_kg, a.hard_feasible, a.hard_violation_sum
            );
            for r in &a.residuals {
                let watch = matches!(
                    r.id,
                    "vmc_over_vsr"
                        | "fin_area_ratio_for_vmc_ceiling"
                        | "offdesign_tow"
                        | "mtow_ceiling"
                        | "fuel_capacity_declared"
                        | "min_nose_gear_load"
                );
                if r.violated() || watch || std::env::var_os("CELL_PROBE_ALL").is_some() {
                    println!(
                        "    {:<34} actual {:>14.5} limit {:>14.5} {} role {:?} viol {:.5}{}",
                        r.id,
                        r.actual,
                        r.limit,
                        r.unit,
                        r.role,
                        r.normalized_violation,
                        if r.violated() { " VIOLATED" } else { "" }
                    );
                }
            }
        }
        Err(reason) => println!("{label}: not assessable: {reason}"),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: cell_probe INPUT_CONFIG_JSON MTOW_MODE [SEARCH_JSON ROWS]".into());
    }
    let mut config = AlasConfig::from_value(&serde_json::from_str::<Value>(&fs::read_to_string(
        &args[1],
    )?)?)?;
    if args[2] != "config" {
        config.optimizer.objective.mtow_sizing = parse_mode(&args[2])?;
    }
    let nominal = nominal_of(&config);
    if args.get(3).map(String::as_str) != Some("skip-nominal") {
        report("nominal", &config, &nominal);
    }
    if let (Some(path), Some("design_database")) = (args.get(3), args.get(4).map(String::as_str)) {
        // A delivered design: the `design_vector` object of design_database.json.
        let document: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        let values: Vec<f64> = alas_config::design_variables::SPECS
            .iter()
            .map(|spec| {
                document["design_vector"][spec.name]
                    .as_f64()
                    .unwrap_or(f64::NAN)
            })
            .collect();
        report("delivered", &config, &DesignVector::from_array(&values)?);
    } else if let (Some(path), Some(rows)) = (args.get(3), args.get(4)) {
        let document: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        let history = &document["optimization"]["history"];
        let vectors = history["design_vectors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let hard = history["hard_violation"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let indices: Vec<usize> = if rows == "best" {
            let mut order: Vec<usize> = (0..vectors.len()).collect();
            order.sort_by(|a, b| {
                let key = |i: &usize| hard[*i].as_f64().unwrap_or(f64::INFINITY);
                key(a).total_cmp(&key(b))
            });
            order.into_iter().take(3).collect()
        } else {
            rows.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        };
        for index in indices {
            let values: Vec<f64> = alas_config::design_variables::SPECS
                .iter()
                .map(|spec| vectors[index][spec.name].as_f64().unwrap_or(f64::NAN))
                .collect();
            if let Ok(design) = DesignVector::from_array(&values) {
                report(&format!("row{index}"), &config, &design);
            }
        }
    }
    Ok(())
}
