// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible coupled-search measurement with complete residual evidence.
//! Usage: optimizer_revision_probe CONFIG_JSON OUTPUT_JSON [WORKERS] [GENERATIONS]
//! A zero generation argument measures the nominal only.

use alas_config::{AlasConfig, DesignVector};
use alas_opt::{assess_product_candidate, CandidateAssessment, DesignOptimizer};
use serde_json::{json, Value};
use std::{error::Error, fs, time::Instant};

fn assessment(config: &AlasConfig, design: &DesignVector) -> Value {
    let start = Instant::now();
    let result = assess_product_candidate(config, design);
    let elapsed = start.elapsed().as_secs_f64();
    match result {
        Ok(value) => assessment_json(config, value, elapsed),
        Err(error) => json!({"assessable":false,"error":error,"wall_time_s":elapsed}),
    }
}

fn assessment_json(config: &AlasConfig, value: CandidateAssessment, elapsed: f64) -> Value {
    let geometry = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&value.resolved.design), true)
        .ok();
    let structural_config =
        if config.optimizer.objective.mtow_sizing == alas_config::MtowSizing::Unconstrained {
            config.at_closure_mass(value.sized.takeoff_mass_kg)
        } else {
            config.clone()
        };
    let structure = geometry.as_ref().and_then(|plane| {
        alas_opt::mdo::structural_feasibility::assess_candidate(
            &structural_config,
            &value.resolved.design,
            plane,
        )
        .ok()
    });
    json!({
        "assessable":true,"strictly_feasible":value.is_strictly_feasible(),
        "wall_time_s":elapsed,"design":value.resolved.design,
        "objective":value.objective_value,"cost":value.cost,
        "takeoff_mass_kg":value.sized.takeoff_mass_kg,
        "block_fuel_kg":value.sized.block_fuel_kg,
        "structural_model":structure.map(|s| json!({
            "passes":s.passes(), "full_wing_primary_mass_kg":s.primary_mass_kg,
            "full_wing_mesh_primary_mass_kg":s.mesh_primary_mass_kg,
            "max_tip_deflection_over_semispan":s.max_tip_deflection_ratio,
            "max_abs_slope":s.max_abs_slope, "curvature_error":s.max_linear_curvature_relative_error,
            "curvature_error_budget":s.limits.max_curvature_relative_error,
            "case":s.governing_load_case, "load_factor":s.governing_load_factor,
        })),
        "wing_sections":geometry.as_ref().and_then(|p| p.wings.first()).map(|w| w.xsecs.iter()
            .map(|s| json!({"leading_edge_m":s.xyz_le,"chord_m":s.chord})).collect::<Vec<_>>()),
        "rejected_by":value.violated_hard_ids(),
        "residuals":value.residuals.iter().map(|r| json!({
            "id":r.id,"family":format!("{:?}",r.family),"policy":format!("{:?}",r.policy),
            "actual":r.actual,"limit":r.limit,"unit":r.unit,
            "raw_residual":r.raw_residual,"normalized_violation":r.normalized_violation,
        })).collect::<Vec<_>>()
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: optimizer_revision_probe CONFIG_JSON OUTPUT_JSON [WORKERS] [GENERATIONS]"
                .into(),
        );
    }
    let document: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let mut config = AlasConfig::from_value(&document)?;
    let nominal = alas_config::presets::get(&config.preset)?.design_vector;
    if let Some(workers) = args.get(3) {
        config.optimizer.solver.workers = workers.parse()?;
    }
    let generations = args.get(4).map(|s| s.parse::<i64>()).transpose()?;
    if let Some(count) = generations {
        config.optimizer.solver.max_iterations = count;
    }
    config.optimizer.solver.seed = Some(7);
    let mut evidence = json!({
        "scope":"Native coupled optimizer only; external FE/aero and final pipeline not timed",
        "configuration":config,"nominal":assessment(&config,&nominal),
    });
    if generations != Some(0) {
        let start = Instant::now();
        let result =
            DesignOptimizer::new(config.clone()).run_diagnostics(None, Some(&nominal), None);
        evidence["search_wall_time_s"] = json!(start.elapsed().as_secs_f64());
        match result {
            Ok(outcome) => {
                let result = outcome.evidence();
                let candidate_key = if outcome.rejection().is_some() {
                    "restoration_candidate"
                } else {
                    "winner"
                };
                evidence[candidate_key] = assessment(&config, &result.best_design);
                evidence["search"] = serde_json::to_value(result)?;
                if let Some(rejection) = outcome.rejection() {
                    evidence["search_rejection"] = serde_json::to_value(rejection)?;
                }
            }
            Err(error) => evidence["search_error"] = json!(error.to_string()),
        }
    }
    if let Some(parent) = std::path::Path::new(&args[2]).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args[2], serde_json::to_vec_pretty(&evidence)?)?;
    Ok(())
}
