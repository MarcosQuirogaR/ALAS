// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lofted lining chord of a registered body at given stations and heights
//! above the passenger floor, with the drawn-width ratios and the production
//! row chord a 0.46 m seat gets there. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example row_chord_probe -- A320-200 30.5 25 5
//! ```
#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "A320-200".to_owned());
    let stations: Vec<f64> = args.filter_map(|s| s.parse().ok()).collect();
    let Ok(preset) = presets::get(&name) else {
        return;
    };
    let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
        return;
    };
    let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
    else {
        return;
    };
    let Ok(g) = product_cabin_geometry(&plane, &config) else {
        return;
    };
    let deck = &g.passenger_decks[0];
    println!(
        "{name}: lining scale {:?}, wall {:.3} m",
        g.lining_scale, g.wall
    );
    for x in stations {
        let floor = g.floor_z(deck, x);
        let chords: Vec<String> = [0.635, 1.034, 1.10, 1.15]
            .iter()
            .map(|h| format!("{h}: {:.3}", g.lining_chord_at_z(x, floor + h)))
            .collect();
        println!(
            "x = {x:.2} m: lofted lining chord by height {} | outer width {:.3} | row chord (0.46 m seat) {:.3}",
            chords.join(", "),
            g.width_at(x),
            g.row_chord_over(deck, x - 0.4, x + 0.4, 0.46)
        );
    }
}
