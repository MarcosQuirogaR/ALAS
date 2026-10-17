// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical evidence of the registered nominals for comparing two builds or
//! two settings of the VLM speed switches (`ALAS_VLM_SCHUR_LU`,
//! `ALAS_VLM_FAST_KERNEL`).
//!
//! Usage: vlm_equivalence OUTPUT_JSON [PRESET ...]   (default: every preset)
//!
//! Per preset it records, from a full-fidelity assessment of the registered
//! nominal: the trimmed drag table (design CL, induced CD and CD on a CL grid),
//! L/D, closed takeoff mass, block fuel, the neutral point and CG; and from a
//! two-evaluation search seeded with the nominal, the history row of that
//! nominal (cruise angle of attack, trim stabilizer incidence, L/D, cost).
//! Compare two outputs with the lane's `compare_equivalence.py`.

#![allow(clippy::print_stdout)]

use alas_config::{solver_presets, AlasConfig};
use alas_opt::mdo::assess_product_candidate_with_controls;
use alas_opt::{DesignOptimizer, ScreeningFidelity};
use serde_json::{json, Value};
use std::{error::Error, fs};

fn row(preset: &str) -> Result<Value, Box<dyn Error>> {
    let config = AlasConfig::from_value(&json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))?;
    let design = alas_config::presets::get(preset)?.design_vector;
    let fidelity = ScreeningFidelity::full();
    let configured = fidelity.configure(&config);
    let assessed =
        assess_product_candidate_with_controls(&configured, &design, fidelity.controls())?;
    let drag = assessed.sized.fuel_artifacts.drag.table().map(|table| {
        let altitude = table.reference_altitude_m();
        let mach = table.design_mach();
        let samples: Vec<Value> = (0..=28)
            .map(|index| {
                let cl = f64::from(index) * 0.05;
                json!({"cl": cl, "induced_cd": table.induced_cd(cl), "cd": table.cd(cl, mach, altitude)})
            })
            .collect();
        json!({"design_cl": table.design_cl(), "samples": samples})
    });

    let mut search_config = AlasConfig::from_value(&json!({"preset": preset}))?;
    let mut settings = solver_presets::get("quick_draft")?.settings.clone();
    settings.screening.max_evaluations = 1;
    settings.refinement.max_evaluations = 1;
    settings.stop_on_evaluations_only = true;
    settings.workers = 1;
    settings.seed = Some(7);
    settings.validate_budgets()?;
    search_config.optimizer.solver = settings;
    let outcome = DesignOptimizer::new(search_config).run_diagnostics(None, Some(&design), None)?;
    let history = &outcome.evidence().history;
    Ok(json!({
        "preset": preset,
        "drag_table": drag,
        "lift_to_drag": assessed.sized.lift_to_drag,
        "takeoff_mass_kg": assessed.sized.takeoff_mass_kg,
        "block_fuel_kg": assessed.sized.block_fuel_kg,
        "cg_x_m": assessed.resolved.cg_x_m,
        "x_neutral_point_m": assessed.resolved.x_neutral_point_m,
        "cost": assessed.cost,
        "retrim_count": assessed.sized.retrim_count,
        "history_row0": {
            "span_m": history.span_m.first(), "alpha_deg": history.alpha_deg.first(),
            "trim_ih_deg": history.trim_ih_deg.first(), "l_over_d": history.l_over_d.first(),
            "cost": history.cost.first(), "area_m2": history.area_m2.first(),
        },
        "nominal_span_m": design.span_m,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("usage: vlm_equivalence OUTPUT_JSON [PRESET ...]".into());
    }
    let presets: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        alas_config::presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let mut rows = Vec::new();
    for preset in &presets {
        println!("{preset}");
        rows.push(row(preset)?);
    }
    fs::write(&args[1], serde_json::to_vec_pretty(&json!({"rows": rows}))?)?;
    Ok(())
}
