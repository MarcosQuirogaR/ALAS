// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per preset: the lofted lining's chord at the armrests and the shoulders at
//! the middle of the cabin, and the ratio the drawn (or class) widths give.
//! Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example lining_scale_probe
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::geometry::{ARMREST_ABOVE_FLOOR_M, SHOULDER_ABOVE_FLOOR_M};

fn main() {
    for name in presets::available() {
        let Ok(preset) = presets::get(name) else {
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            continue;
        };
        let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
        else {
            continue;
        };
        let Ok(g) = product_cabin_geometry(&plane, &config) else {
            continue;
        };
        let x = 0.5 * (g.cabin_start_x + g.cabin_end_x);
        let floor = g.floor_z(&g.passenger_decks[0], x);
        let lofted = [
            g.lining_chord_at_z(x, floor + ARMREST_ABOVE_FLOOR_M),
            g.lining_chord_at_z(x, floor + SHOULDER_ABOVE_FLOOR_M),
        ];
        println!(
            "{name}: lofted lining armrest {:.3} shoulder {:.3} | scale {:.4} {:.4} | drawn-equivalent {:.3} {:.3}",
            lofted[0],
            lofted[1],
            g.lining_scale[0],
            g.lining_scale[1],
            lofted[0] * g.lining_scale[0],
            lofted[1] * g.lining_scale[1],
        );
    }
}
