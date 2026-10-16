// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stage durations of one native baseline pipeline run (no optimization), with
//! the `alas_math::stage_profile` split of the mission stage.
//!
//! ```text
//! cargo run -p alas-pipeline --profile harness --example baseline_stage_profile -- A320-200 ATR72-600
//! ```
//!
//! Per preset it runs the public pipeline with `optimize = false` and every
//! external tool off (the matrix's native baseline cell), prints the
//! top-level and downstream stage durations from the run events, and the
//! stage-profile table (exclusive and inclusive seconds, counters). Absolute
//! seconds belong to the host that ran them; run it alone for clean numbers.
#![allow(clippy::print_stdout)]

use std::error::Error;
use std::sync::Mutex;

use alas_config::{presets, AlasConfig};
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

fn main() -> Result<(), Box<dyn Error>> {
    let names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        return Err("usage: baseline_stage_profile PRESET...".into());
    }
    alas_math::stage_profile::enable();
    // `WATCHDOG_S=n`: after n seconds print the in-flight profile and exit,
    // to see where a non-terminating run spends its time.
    if let Some(limit) = std::env::var("WATCHDOG_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(limit));
            let report = alas_math::stage_profile::report();
            println!("== WATCHDOG after {limit} s");
            for (key, exclusive, calls, inclusive) in &report.stages {
                if *calls > 0 {
                    println!("  profile {key:<36} excl {exclusive:>8.3} s  incl {inclusive:>8.3} s  calls {calls}");
                }
            }
            for (key, count) in &report.counters {
                if *count > 0 {
                    println!("  counter {key:<24} {count}");
                }
            }
            std::process::exit(3);
        });
    }
    for name in names {
        // A `*.json` argument is a full configuration file (for example a
        // cell's `input_config.json`); anything else is a preset name.
        let mut config = if name.ends_with(".json") {
            let text = std::fs::read_to_string(&name)?;
            AlasConfig::from_value(&serde_json::from_str::<serde_json::Value>(&text)?)?
        } else {
            let preset = presets::get(&name)?;
            AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))?
        };
        if let Ok(path) = std::env::var("DUMP_CONFIG") {
            std::fs::write(&path, serde_json::to_string_pretty(&config)?)?;
        }
        config.mses.enabled = false;
        config.downstream.openvsp = false;
        config.downstream.vspaero = false;
        config.downstream.avl = false;
        config.downstream.flowunsteady = false;
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        let options = PipelineOptions {
            optimize: false,
            compare_baseline: true,
            parallel: true,
            aerodynamic_solver: Default::default(),
            optimization_solver: Default::default(),
            output_dir: None,
            save_plots: false,
            seed: Some(42),
            quiet: true,
        };
        let durations = Mutex::new(Vec::<(String, u64)>::new());
        let on_event = |event: alas_pipeline::runs::RunEvent| {
            if let Some(ms) = event.duration_ms {
                if let Ok(mut list) = durations.lock() {
                    list.push((event.stage, ms));
                }
            }
        };
        alas_math::stage_profile::reset();
        let started = std::time::Instant::now();
        let result = DesignPipeline::new(config).run_with_environment_and_events(
            &options,
            &RunEnvironment::default(),
            &on_event,
        );
        let wall_s = started.elapsed().as_secs_f64();
        println!("== {name}: wall {wall_s:.2} s, ok = {}", result.is_ok());
        for (stage, ms) in durations.lock().map(|l| l.clone()).unwrap_or_default() {
            if ms >= 50 {
                println!("  stage {stage:<28} {:>8.3} s", ms as f64 / 1.0e3);
            }
        }
        let report = alas_math::stage_profile::report();
        for (key, exclusive, calls, inclusive) in &report.stages {
            if *calls > 0 {
                println!(
                    "  profile {key:<36} excl {exclusive:>8.3} s  incl {inclusive:>8.3} s  calls {calls}"
                );
            }
        }
        for (key, count) in &report.counters {
            if *count > 0 {
                println!("  counter {key:<24} {count}");
            }
        }
    }
    Ok(())
}
