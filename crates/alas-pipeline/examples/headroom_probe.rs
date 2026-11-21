// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Floor, floor plus headroom and ceiling of the main and the lower deck at
//! the cabin ends and the middle of every passenger preset: the three must
//! agree (`floor + deck_height = ceiling`). Heights m.
//!
//! ```text
//! cargo run -p alas-pipeline --example headroom_probe
//! ```
#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for name in presets::available() {
        let preset = presets::get(name)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| format!("{name}: {error:?}"))?;
        let g = product_cabin_geometry(&plane, &config)?;
        let mut worst = 0.0_f64;
        for deck in g
            .passenger_decks
            .iter()
            .chain(std::iter::once(&g.lower_deck))
        {
            for fraction in [0.0, 0.1, 0.5, 0.9, 1.0] {
                let x = g.cabin_start_x + fraction * (g.cabin_end_x - g.cabin_start_x);
                let gap = g.floor_z(deck, x) + g.deck_height(deck, x) - g.ceil_z(deck, x);
                if g.deck_height(deck, x) > 0.3 + 1e-9 {
                    worst = worst.max(gap.abs());
                }
            }
        }
        println!("{name}: largest |floor + headroom - ceiling| over the cabin {worst:.4} m");
    }
    Ok(())
}
