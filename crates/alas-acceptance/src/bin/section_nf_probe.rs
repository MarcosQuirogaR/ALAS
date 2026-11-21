// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Section-level cross-check: NeuralFoil (the section model ALAS uses) at the
//! main-wing root section, at the angles, Mach and Reynolds number of an MSES
//! `polar_diagnostics.json`, printed beside the MSES values.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::env;
use std::fs;
use std::io;

use alas_aero::neuralfoil::{Conditions, ModelSize, PreparedAirfoil};
use alas_config::presets;
use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use serde_json::{json, Value};

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let [preset_name, diagnostics, output] = args.as_slice() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: section_nf_probe <preset> <polar_diagnostics.json> <output.json>",
        ));
    };
    let preset = presets::get(preset_name).map_err(io::Error::other)?;
    let mut config = AlasConfig::from_value(&json!({ "preset": preset.name }))
        .map_err(|error| io::Error::other(error.to_string()))?;
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = false;
    config.downstream.vspaero = false;
    config.downstream.openvsp = false;
    config.downstream.avl = false;
    config.downstream.flowunsteady = false;
    let locator = ToolLocator::for_current_process();
    let environment = locator.resolve_environment(
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
    );
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = DesignPipeline::new(config)
        .run(&options, &environment)
        .map_err(io::Error::other)?;
    let report = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
        .ok_or_else(|| io::Error::other("no analysis report"))?;
    let root = report
        .airplane
        .wings
        .first()
        .and_then(|wing| wing.xsecs.first())
        .ok_or_else(|| io::Error::other("no root section"))?;
    let doc: Value = serde_json::from_str(&fs::read_to_string(diagnostics)?)?;
    let mach = doc["mach"].as_f64().unwrap_or(0.0);
    let reynolds = doc["reynolds"].as_f64().unwrap_or(0.0);
    let prepared = PreparedAirfoil::prepare(&root.airfoil).map_err(io::Error::other)?;
    let column = |key: &str| -> Vec<f64> {
        doc[key]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default()
    };
    let (alpha, cl, cd, cm) = (
        column("converged_alpha_deg"),
        column("cl"),
        column("cd"),
        column("cm"),
    );
    let rows = alpha
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let nf = prepared
                .aero(&Conditions::new(a, reynolds), mach, ModelSize::Large)
                .ok();
            json!({
                "alpha_deg": a,
                "cl_mses": cl.get(i), "cd_mses": cd.get(i), "cm_mses": cm.get(i),
                "cl_nf": nf.as_ref().map(|r| r.cl),
                "cd_nf": nf.as_ref().map(|r| r.cd),
                "cm_nf": nf.as_ref().map(|r| r.cm),
            })
        })
        .collect::<Vec<_>>();
    let out = json!({
        "preset": preset_name, "mach": mach, "reynolds": reynolds,
        "root_chord_m": root.chord, "rows": rows,
    });
    fs::write(
        output,
        serde_json::to_vec_pretty(&out).map_err(io::Error::other)?,
    )?;
    println!("wrote {output}");
    Ok(())
}
