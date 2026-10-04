// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible candidate timing and numerical evidence.
//! Usage: vlm_speed_probe measure OUTPUT_JSON [PRESET ...]
//!        vlm_speed_probe search OUTPUT_JSON PRESET [WORKERS]
//! Candidate measurements use one Rayon worker; dense kernels are sequential.

use alas_config::{AlasConfig, DesignVector};
use alas_opt::mdo::assess_product_candidate_with_controls;
use alas_opt::{CandidateAssessment, DesignOptimizer, ScreeningFidelity};
use serde_json::{json, Value};
use std::{error::Error, fs, path::Path, time::Instant};

fn configuration(preset: &str) -> Result<AlasConfig, Box<dyn Error>> {
    Ok(AlasConfig::from_value(&json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))?)
}

fn numerical_evidence(value: &CandidateAssessment) -> Value {
    let table = value.sized.fuel_artifacts.drag.table();
    let drag = table.map(|table| {
        let altitude = table.reference_altitude_m();
        let mach = table.design_mach();
        let samples: Vec<Value> = (0..=28)
            .map(|index| {
                let cl = f64::from(index) * 0.05;
                json!({
                    "cl": cl, "induced_cd": table.induced_cd(cl),
                    "cd": table.cd(cl, mach, altitude),
                    "cd_sea_level": table.cd(cl, mach, 0.0),
                    "cd_mach_plus_004": table.cd(cl, mach + 0.04, altitude)
                })
            })
            .collect();
        json!({
            "design_cl": table.design_cl(), "design_mach": mach,
            "reference_altitude_m": altitude, "grid_size": table.grid_size(),
            "max_abs_error_cd": table.max_abs_error_cd(), "samples": samples
        })
    });
    json!({
        "objective": value.objective_value, "cost": value.cost,
        "closed_tow_kg": value.sized.takeoff_mass_kg,
        "block_fuel_kg": value.sized.block_fuel_kg,
        "structural_primary_mass_kg": value.sized.structural_primary_mass_kg,
        "retrim_count": value.sized.retrim_count,
        "sizing_iterations": value.sized.sizing_iterations,
        "strictly_feasible": value.is_strictly_feasible(),
        "drag_table": drag, "rejected_by": value.violated_hard_ids(),
        "design": value.resolved.design,
        "residuals": value.residuals.iter().map(|residual| json!({
            "id": residual.id, "actual": residual.actual,
            "normalized_violation": residual.normalized_violation
        })).collect::<Vec<_>>()
    })
}

fn measure_candidate(
    config: &AlasConfig,
    design: &DesignVector,
    fidelity: ScreeningFidelity,
) -> Result<Value, Box<dyn Error>> {
    let configured = fidelity.configure(config);
    let mut wall_time_s = Vec::with_capacity(3);
    let mut evidence = Value::Null;
    for _ in 0..3 {
        let started = Instant::now();
        let assessed =
            assess_product_candidate_with_controls(&configured, design, fidelity.controls())?;
        wall_time_s.push(started.elapsed().as_secs_f64());
        evidence = numerical_evidence(&assessed);
    }
    let best_s = wall_time_s.iter().copied().fold(f64::INFINITY, f64::min);
    Ok(json!({
        "best_of_3_s": best_s, "wall_time_s": wall_time_s,
        "chordwise_resolution": configured.analysis.chordwise_resolution,
        "spanwise_resolution": configured.analysis.spanwise_resolution,
        "steps_per_segment": fidelity.steps_per_segment, "outputs": evidence
    }))
}

fn measure(presets: &[String]) -> Result<Value, Box<dyn Error>> {
    let mut rows = Vec::with_capacity(presets.len());
    for preset in presets {
        let config = configuration(preset)?;
        let design = alas_config::presets::get(preset)?.design_vector;
        rows.push(json!({
            "preset": preset, "configuration": config,
            "full": measure_candidate(&config, &design, ScreeningFidelity::full())?,
            "screening": measure_candidate(&config, &design, ScreeningFidelity::shipped())?
        }));
    }
    Ok(json!({"rayon_threads": 1, "faer_parallelism": "sequential", "rows": rows}))
}

fn search(preset: &str, workers: i64) -> Result<Value, Box<dyn Error>> {
    let mut config = configuration(preset)?;
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.seed = Some(7);
    let design = alas_config::presets::get(preset)?.design_vector;
    let started = Instant::now();
    let outcome =
        DesignOptimizer::new(config.clone()).run_diagnostics(None, Some(&design), None)?;
    let result = outcome.evidence();
    Ok(json!({
        "preset": preset, "configuration": config,
        "search_wall_time_s": started.elapsed().as_secs_f64(),
        "resolved_workers": config.optimizer.solver.resolved_workers(),
        "search_diagnostics": result.search_diagnostics,
        "search_rejection": outcome.rejection(),
        "best_design": result.best_design,
        "best_cost": result.best_cost
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: vlm_speed_probe measure|search OUTPUT_JSON [PRESET ...]".into());
    }
    let result = match args[1].as_str() {
        "measure" => {
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build_global()?;
            let presets = if args.len() > 3 {
                args[3..].to_vec()
            } else {
                ["A320-200", "ATR72-600", "AVE", "B787-9"]
                    .map(str::to_owned)
                    .to_vec()
            };
            measure(&presets)?
        }
        "search" => {
            let preset = args.get(3).ok_or("search requires a preset")?;
            let workers = args
                .get(4)
                .map(|value| value.parse())
                .transpose()?
                .unwrap_or(0);
            search(preset, workers)?
        }
        _ => return Err("unknown measurement mode".into()),
    };
    if let Some(parent) = Path::new(&args[2])
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args[2], serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}

#[cfg(test)]
#[path = "vlm_speed_probe/equality.rs"]
mod equality;
