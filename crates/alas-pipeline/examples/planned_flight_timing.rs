// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wall time of the flight the Mission page computes in the background
//! (`quick_analysis::planned_flight`), with the `alas_math::stage_profile`
//! split and the share of the baseline analysis it starts from.
//!
//! ```text
//! cargo run -p alas-pipeline --profile harness --example planned_flight_timing -- AVE
//! ```
//!
//! Each preset is flown three times; every run starts cold for the analysis
//! (nothing is kept between runs but the process-wide numeric caches). Run it
//! alone for clean numbers; absolute seconds belong to the host.
#![allow(clippy::print_stdout)]

use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alas_config::{presets, AlasConfig};
use alas_pipeline::FullAnalysis;

fn main() -> Result<(), Box<dyn Error>> {
    let names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        return Err("usage: planned_flight_timing PRESET...".into());
    }
    alas_math::stage_profile::enable();
    for name in names {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        let design = presets::get(&name)?.design_vector;
        for run in 0..3 {
            alas_math::stage_profile::reset();
            let start = Instant::now();
            let flown = alas_pipeline::quick_analysis::planned_flight(
                &config,
                &design,
                None,
                &AtomicBool::new(false),
            )?;
            let total = start.elapsed();
            let table = alas_math::stage_profile::report();
            // A fingerprint of everything the flight returned, to compare two builds bit for bit.
            let print = {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                format!("{:?}", flown.phases).hash(&mut hasher);
                format!("{:?}", flown.load_case).hash(&mut hasher);
                flown.landing_mass_kg.to_bits().hash(&mut hasher);
                hasher.finish()
            };
            // The baseline analysis the flight starts from, timed alone.
            let analysis = FullAnalysis::new(config.clone());
            let start = Instant::now();
            let airplane = analysis.build_airplane(&design, true)?;
            let built = start.elapsed();
            let start = Instant::now();
            let _report = analysis.run_on_airplane(&design, airplane)?;
            let analysed = start.elapsed();
            println!(
                "{name} run {run}: total {:.0} ms ({} phases, takeoff {:.6} kg, fingerprint {print:016x}); baseline alone: build {:.0} ms, analysis {:.0} ms",
                total.as_secs_f64() * 1e3,
                flown.phases.len(),
                flown.load_case.takeoff_mass_kg,
                built.as_secs_f64() * 1e3,
                analysed.as_secs_f64() * 1e3,
            );
            if run == 0 || std::env::var("ALL_RUNS").is_ok() {
                for (key, exclusive, calls, inclusive) in table.stages {
                    if calls > 0 {
                        println!(
                            "  {key:<36} excl {:>8.0} ms  incl {:>8.0} ms  calls {calls}",
                            exclusive * 1e3,
                            inclusive * 1e3
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
