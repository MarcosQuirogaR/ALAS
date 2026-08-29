// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Controlled A340 input and optimizer sensitivity used by the preset-correlation audit.

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use alas_config::design_variables::{DesignVariableSpec, DesignVector, SPECS};
use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_opt::DesignOptimizer;
use alas_pipeline::full_analysis::{AnalysisReport, FullAnalysis};

const A340_PUBLIC_AREA_M2: f64 = 361.6;
const SEED: i64 = 42;

fn main() -> io::Result<()> {
    let output = parse_output(env::args().skip(1))?;
    let preset = presets::get("A340-300").map_err(io::Error::other)?;
    let config = preset_config(preset);
    let baseline = analyze(&config, &preset.design_vector)?;

    let mut public_cabin_config = config.clone();
    public_cabin_config.requirements.num_passengers = 335;
    let public_cabin = analyze(&public_cabin_config, &preset.design_vector)?;

    let mut incidence_adjusted_config = config.clone();
    incidence_adjusted_config.geometry.wing.root_twist_deg += 1.5;
    incidence_adjusted_config.geometry.wing.break_twist_deg += 1.5;
    let incidence_adjusted = analyze(&incidence_adjusted_config, &preset.design_vector)?;

    let calibrated_kink_fraction = calibrate_kink_fraction(&config, &preset.design_vector)?;
    let mut calibrated_planform_config = config.clone();
    calibrated_planform_config.geometry.wing.kink_span_fraction = Some(calibrated_kink_fraction);
    let calibrated_planform = analyze(&calibrated_planform_config, &preset.design_vector)?;

    let bounds = local_bounds(&preset.design_vector);
    let mut optimizer_runs = Vec::new();
    for incidence_offset_deg in [0.0, 1.5] {
        for method in [
            "differential_evolution",
            "feasibility_first_de",
            "nsga2",
            "turbo_1",
            "cma_es",
        ] {
            optimizer_runs.push(run_optimizer(
                &config,
                &preset.design_vector,
                &bounds,
                method,
                false,
                incidence_offset_deg,
            ));
        }
    }
    optimizer_runs.push(run_optimizer(
        &config,
        &preset.design_vector,
        &bounds,
        "differential_evolution",
        true,
        0.0,
    ));

    let artifact = serde_json::json!({
        "scope": "A340-300 selected because it has the largest composite discrepancy among directly comparable public anchors: a 290-seat modeled load case versus 335-seat Airbus planning cabin and cruise body alpha above the user-specified 2-4 deg window.",
        "source_for_area": "Airbus A340-200/-300 Aircraft Characteristics, wing reference area 361.6 m^2",
        "fixed_seed": SEED,
        "optimizer_budget": {
            "max_iterations": 8,
            "population_multiplier": 1,
            "bounds": "+/-15% rounded exactly as the GUI; global bounds retained for zero-centered variables"
        },
        "input_sensitivities": {
            "baseline_290_passengers": baseline,
            "public_planning_cabin_335_passengers": public_cabin,
            "root_and_break_incidence_plus_1p5_deg": incidence_adjusted,
            "area_calibrated_kink": {
                "kink_span_fraction": calibrated_kink_fraction,
                "result": calibrated_planform
            }
        },
        "optimizer_sensitivities": optimizer_runs,
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        output,
        serde_json::to_string_pretty(&artifact).map_err(io::Error::other)?,
    )
}

fn preset_config(preset: &alas_config::AircraftPreset) -> AlasConfig {
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        landing_gear: preset.landing_gear.clone(),
        ..AlasConfig::default()
    };
    if let Some(mass_model) = preset.mass_model.as_ref() {
        config.mass_model = mass_model.clone();
    }
    if let Some(performance) = preset.performance.as_ref() {
        config.performance = performance.clone();
    }
    config
}

fn analyze(config: &AlasConfig, design: &DesignVector) -> io::Result<serde_json::Value> {
    let report = FullAnalysis::new(config.clone())
        .run(design, false)
        .map_err(io::Error::other)?;
    Ok(analysis_record(config, &report))
}

fn analysis_record(config: &AlasConfig, report: &AnalysisReport) -> serde_json::Value {
    let projected_area = report
        .geometry_summary
        .get("projected_wing_area_m2")
        .copied();
    let trim = report.trimmed_design_point;
    serde_json::json!({
        "requested_passengers": config.requirements.num_passengers,
        "modeled_payload_kg": report.component_masses.get("Payload"),
        "projected_wing_area_m2": projected_area,
        "area_error_percent": projected_area.map(|area| 100.0 * (area / A340_PUBLIC_AREA_M2 - 1.0)),
        "geometric_body_alpha_deg": trim.map(|point| point.geometric_body_alpha_deg),
        "trim_incidence_deg": trim.map(|point| point.trim_ih_deg),
        "cl": trim.map_or(report.design_point.cl, |point| point.cl),
        "cd": trim.map_or(report.design_point.cd, |point| point.cd),
        "l_over_d": trim.map_or(report.design_point.l_over_d, |point| point.l_over_d),
        "static_margin_percent_mac": 100.0 * report.static_margin,
        "physical_cg_m": report.physical_cg,
    })
}

fn calibrate_kink_fraction(config: &AlasConfig, design: &DesignVector) -> io::Result<f64> {
    let mut best = (f64::INFINITY, 0.37);
    for step in 0..=200 {
        let fraction = 0.20 + step as f64 * 0.002;
        let mut candidate_config = config.clone();
        candidate_config.geometry.wing.kink_span_fraction = Some(fraction);
        let airplane = AircraftBuilder::new(Some(candidate_config.geometry))
            .build(Some(design), true)
            .map_err(io::Error::other)?;
        let area = airplane
            .wings
            .first()
            .map(|wing| wing.projected_area())
            .ok_or_else(|| io::Error::other("A340 geometry has no main wing"))?;
        let error = (area - A340_PUBLIC_AREA_M2).abs();
        if error < best.0 {
            best = (error, fraction);
        }
    }
    Ok(best.1)
}

fn run_optimizer(
    baseline_config: &AlasConfig,
    initial: &DesignVector,
    bounds: &[(f64, f64)],
    method: &str,
    shape_priors: bool,
    incidence_offset_deg: f64,
) -> serde_json::Value {
    let mut config = baseline_config.clone();
    config.geometry.wing.root_twist_deg += incidence_offset_deg;
    config.geometry.wing.break_twist_deg += incidence_offset_deg;
    config.optimizer.solver.method = method.to_owned();
    config.optimizer.solver.seed = Some(SEED);
    config.optimizer.solver.max_iterations = 8;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed_near_initial_design = true;
    config.optimizer.weights.transport_shape_priors_enabled = shape_priors;
    let mut optimizer = DesignOptimizer::new(config);
    let result = match optimizer.run(Some(bounds), Some(initial), None) {
        Ok(result) => result,
        Err(error) => {
            return serde_json::json!({
                "method": method,
                "shape_priors_enabled": shape_priors,
                "root_and_break_incidence_offset_deg": incidence_offset_deg,
                "error": error.to_string(),
            })
        }
    };
    let best_index = result
        .history
        .design_vectors
        .iter()
        .enumerate()
        .find(|(index, design)| {
            **design == result.best_design
                && result.history.cost.get(*index).copied() == Some(result.best_cost)
        })
        .map(|(index, _)| index);
    serde_json::json!({
        "method": method,
        "shape_priors_enabled": shape_priors,
        "root_and_break_incidence_offset_deg": incidence_offset_deg,
        "evaluations": result.history.n_evaluations(),
        "valid_evaluations": result.history.n_valid(),
        "best_cost": result.best_cost,
        "best_design": result.best_design,
        "best_l_over_d": best_index.and_then(|index| result.history.l_over_d.get(index)).copied(),
        "best_geometric_body_alpha_deg": best_index.and_then(|index| result.history.alpha_deg.get(index)).copied(),
        "best_projected_area_m2": best_index.and_then(|index| result.history.area_m2.get(index)).copied(),
        "reject_reason_counts": result.history.reject_reason_counts(),
        "pareto_front_size": result.pareto_front.len(),
    })
}

fn local_bounds(design: &DesignVector) -> Vec<(f64, f64)> {
    design
        .to_array()
        .into_iter()
        .zip(SPECS)
        .map(|(value, spec)| centered_bound(spec, value))
        .collect()
}

fn centered_bound(spec: &DesignVariableSpec, value: f64) -> (f64, f64) {
    if !value.is_finite() || value.abs() < f64::EPSILON {
        return (spec.lower, spec.upper);
    }
    let half_width = 0.15 * value.abs();
    let decimals = if value.abs() >= 1.0 {
        spec.decimals.min(1)
    } else {
        spec.decimals
    };
    let scale = 10_f64.powi(decimals as i32);
    (
        ((value - half_width) * scale).floor() / scale,
        ((value + half_width) * scale).ceil() / scale,
    )
}

fn parse_output(arguments: impl Iterator<Item = String>) -> io::Result<PathBuf> {
    let values = arguments.collect::<Vec<_>>();
    match values.as_slice() {
        [flag, path] if flag == "--output" => Ok(PathBuf::from(path)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: worst_preset_sensitivity --output <json-path>",
        )),
    }
}
