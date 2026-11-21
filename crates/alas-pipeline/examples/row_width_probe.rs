// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The width a seat row may take along the cabin of one preset: the floor, the
//! armrest-height and the shoulder-height chords and the usable width that
//! results. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example row_width_probe -- ATR72-600
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::geometry::{ARMREST_ABOVE_FLOOR_M, SHOULDER_ABOVE_FLOOR_M};

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ATR72-600".to_owned());
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
    let main = &g.passenger_decks[0];
    let mut x = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(g.cabin_start_x);
    while x <= g.cabin_end_x {
        let floor = g.floor_z(main, x);
        println!(
            "x {x:6.2} floor {floor:.3} | chord floor {:.3} armrest {:.3} shoulder {:.3} | usable {:.3}",
            g.usable_width_at_z(x, floor),
            g.usable_width_at_z(x, floor + ARMREST_ABOVE_FLOOR_M),
            g.usable_width_at_z(x, floor + SHOULDER_ABOVE_FLOOR_M),
            g.usable_width(main, x),
        );
        x += std::env::args()
            .nth(3)
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);
    }
}
