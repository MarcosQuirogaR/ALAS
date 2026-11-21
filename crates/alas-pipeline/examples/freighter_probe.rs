// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main-deck freight positions of a passenger body configured as a freighter:
//! the count, the station range and the container used. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example freighter_probe -- B747-400
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::CargoLoadManager;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "B747-400".to_owned());
    let preset = presets::get(&name).unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.requirements.aircraft_type = "cargo".to_owned();
    config.cabin.cargo.use_main_deck = true;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    let g = product_cabin_geometry(&plane, &config).unwrap();
    let manager = CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let mut rows: Vec<(f64, usize, &str)> = Vec::new();
    for slot in manager.slots.iter().filter(|slot| slot.deck == "main") {
        match rows.iter_mut().find(|row| (row.0 - slot.x).abs() < 0.05) {
            Some(row) => row.1 += 1,
            None => rows.push((slot.x, 1, slot.uld.code)),
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!(
        "{name}: cabin x {:.2}..{:.2}, main-deck rows {}",
        g.cabin_start_x,
        g.cabin_end_x,
        rows.len()
    );
    for (x, across, uld) in rows {
        println!("  x {x:6.2} m  {across} across  {uld}");
    }
}
