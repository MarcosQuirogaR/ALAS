// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What one complete FLOPS mass buildup costs, per registered aircraft.
//!
//! The optimizer lane measured the ATR 72-600's coupled candidate evaluation
//! at about 13 s against 0.12 s for the notional widebody and handed the
//! difference to this lane as a propulsion-path finding. This probe bounds the
//! mass model's possible share of it directly: it times the buildup the
//! coupled analysis calls, on the same aircraft, with nothing else in the
//! loop. A turboprop buildup that costs the same as a turbofan one cannot be
//! the source of a hundredfold difference.
//!
//! Usage:
//!
//! ```text
//! cargo run --release -p alas-mass --example mass_cost_probe -- [repeats]
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::calculate_flops_mass_buildup;
use serde_json::json;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repeats: u32 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);

    println!(
        "{:<12} {:>12} {:>14} {:>10}",
        "preset", "mean_us", "buildups/s", "status"
    );
    for name in presets::available() {
        let config = AlasConfig::from_value(&json!({ "preset": name }))?;
        let preset = presets::get(name)?;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)?;
        let call = || {
            calculate_flops_mass_buildup(
                &plane,
                &config.requirements,
                &config.geometry,
                &config.control_surfaces,
                Some(&config.mass_model),
                &config.landing_gear,
                &config.cabin,
            )
        };
        // One warm call so the timing below is not the first-touch cost.
        let status = match call() {
            Ok(_) => "evaluated",
            Err(_) => "unverified",
        };
        let started = Instant::now();
        for _ in 0..repeats {
            let _ = call();
        }
        let mean_s = started.elapsed().as_secs_f64() / f64::from(repeats);
        println!(
            "{:<12} {:>12.1} {:>14.0} {:>10}",
            name,
            mean_s * 1.0e6,
            1.0 / mean_s,
            status
        );
    }
    Ok(())
}
