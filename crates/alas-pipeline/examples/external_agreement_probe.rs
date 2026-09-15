// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: numeric agreement between the internal VLM and native AVL.
//!
//! `classify_avl_comparison` only decides whether the two are *admissible* to
//! compare: same references, frames, alpha schedule and Prandtl-Glauert
//! domain. Nothing in the tree then computes how far apart they actually are.
#![allow(clippy::print_stdout, missing_docs)]
// Standalone fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use alas_config::{presets, AlasConfig};
use alas_exec::ToolLocator;
use alas_pipeline::{DesignPipeline, PipelineOptions};

fn main() {
    let only = std::env::args().nth(1);
    for name in presets::available() {
        if only.as_deref().is_some_and(|want| want != name) {
            continue;
        }
        let preset = presets::get(name).unwrap();
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })).unwrap();
        config.mission.enabled = false;
        config.structures.enabled = false;
        config.mses.enabled = false;
        let locator = ToolLocator::for_current_process();
        let preferences = locator.load_preferences();
        let environment = locator.resolve_environment(
            Path::new(&config.mses.mses_dir),
            Path::new(&config.structures.nastran_exe_path),
            Path::new(&config.structures.patran_exe_path),
            Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
            Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
        );
        let options = PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: true,
            aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
            output_dir: Some(std::env::temp_dir().join(format!("alas_agree_{name}"))),
            save_plots: false,
            seed: Some(42),
            quiet: true,
            ..Default::default()
        };
        let result = match DesignPipeline::new(config).run(&options, &environment) {
            Ok(result) => result,
            Err(error) => {
                println!("{name:<11} pipeline failed: {error}");
                continue;
            }
        };
        let Some(avl) = result.avl_result.as_ref() else {
            println!("{name:<11} no AVL result");
            continue;
        };
        let (Some(polar), Some(reference)) =
            (avl.polar.as_ref(), avl.comparison_reference.as_ref())
        else {
            println!(
                "{name:<11} AVL status {} ({:?})",
                avl.status.as_str(),
                avl.error
            );
            continue;
        };
        println!(
            "\n=== {name} === AVL {} | phase {} | M{:.3}",
            avl.status.as_str(),
            reference.phase,
            reference.mach
        );
        let mut a = Vec::new();
        let mut vlm = Vec::new();
        let mut avl_cl = Vec::new();
        let mut vlm_cm = Vec::new();
        let mut avl_cm = Vec::new();
        for (i, point) in polar.points.iter().enumerate() {
            let (Some(&cl), Some(&cm)) =
                (reference.vlm_polar.cl.get(i), reference.vlm_polar.cm.get(i))
            else {
                break;
            };
            a.push(point.alpha_deg);
            vlm.push(cl);
            avl_cl.push(point.lift_coefficient);
            vlm_cm.push(cm);
            avl_cm.push(point.pitching_moment_coefficient);
        }
        let slope = |x: &[f64], y: &[f64]| {
            let n = x.len() as f64;
            let mx = x.iter().sum::<f64>() / n;
            let my = y.iter().sum::<f64>() / n;
            let num: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
            let den: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
            (num / den, my - num / den * mx)
        };
        let (sv, iv) = slope(&a, &vlm);
        let (sa, ia) = slope(&a, &avl_cl);
        let (scmv, _) = slope(&a, &vlm_cm);
        let (scma, _) = slope(&a, &avl_cm);
        let at = |s: f64, i: f64, x: f64| s * x + i;
        println!(
            "  dCL/dalpha  VLM {:.5}/deg  AVL {:.5}/deg  ({:+.1}%)",
            sv,
            sa,
            (sv - sa) / sa * 100.0
        );
        println!(
            "  alpha at CL=0  VLM {:+.3} deg  AVL {:+.3} deg  ({:+.3} deg)",
            -iv / sv,
            -ia / sa,
            -iv / sv + ia / sa
        );
        for probe in [0.0_f64, 4.0, 8.0] {
            let (v, w) = (at(sv, iv, probe), at(sa, ia, probe));
            println!(
                "  alpha {probe:>4.1} deg: CL VLM {v:.4} vs AVL {w:.4}  dCL {:+.4} ({:+.1}%)",
                v - w,
                (v - w) / w * 100.0
            );
        }
        println!(
            "  dCm/dalpha  VLM {:.5}/deg  AVL {:.5}/deg  ({:+.1}%)",
            scmv,
            scma,
            (scmv - scma) / scma * 100.0
        );
    }
}
