// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: cruise physics of each preset's flown mission.
#![allow(clippy::print_stdout, missing_docs)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

fn main() {
    println!(
        "{:<11} {:>6} {:>6} {:>7} {:>8} {:>7} {:>6} {:>6} {:>8} {:>9} {:>8} {:>7}",
        "preset",
        "FL",
        "Mach",
        "TAS",
        "mass_t",
        "CL",
        "CD",
        "L/D",
        "thrust_kN",
        "ff_kg_h",
        "TSFC",
        "thr"
    );
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })).unwrap();
        let options = PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: true,
            output_dir: None,
            save_plots: false,
            seed: Some(42),
            quiet: true,
            ..Default::default()
        };
        let result = match DesignPipeline::new(config).run(&options, &RunEnvironment::default()) {
            Ok(result) => result,
            Err(error) => {
                println!("{name:<11} pipeline failed: {error}");
                continue;
            }
        };
        let Some(mission) = result.mission_result.as_ref() else {
            println!("{name:<11} no mission result");
            continue;
        };
        // Take the longest cruise segment's midpoint.
        let mut best: Option<(usize, usize)> = None;
        for (si, seg) in mission.segments.iter().enumerate() {
            if format!("{:?}", seg.spec.kind)
                .to_lowercase()
                .contains("cruise")
            {
                let n = seg.conditions.altitude_m.len();
                if n > 0 {
                    best = Some((si, n / 2));
                }
            }
        }
        let Some((si, pi)) = best else {
            println!(
                "{name:<11} no cruise segment ({} segments)",
                mission.segments.len()
            );
            continue;
        };
        let c = &mission.segments[si].conditions;
        let thrust_n = c.thrust[pi].thrust_n;
        let ff = c.vehicle_mass_rate_kg_s[pi];
        let tsfc = if thrust_n > 0.0 {
            ff / thrust_n
        } else {
            f64::NAN
        };
        println!(
            "{:<11} {:>6.0} {:>6.3} {:>7.1} {:>8.1} {:>7.4} {:>6.4} {:>6.2} {:>8.1} {:>9.0} {:>8.2e} {:>7.3}",
            name,
            c.altitude_m[pi] / 0.3048 / 100.0,
            c.mach[pi],
            c.velocity_m_s[pi],
            c.total_mass_kg[pi] / 1000.0,
            c.lift_coefficient[pi],
            c.drag_coefficient[pi],
            c.lift_coefficient[pi] / c.drag_coefficient[pi],
            thrust_n / 1000.0,
            ff * 3600.0,
            tsfc,
            c.throttle[pi],
        );
    }
}
