// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per registered preset: the mid-cabin section (width, height, centre line),
//! the main and lower deck floors and ceilings in the product frame, and the
//! belly inner skin. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example sec_probe
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

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
        let d = &g.passenger_decks[0];
        let l = &g.lower_deck;
        println!(
            "{name}: W {:.3} H {:.3} zc {:.3} wall {:.2} | main floor {:.3} ceil {:.3} (frac {:.2}) | lower floor {:.3} ceil {:.3} | belly_in {:.3} dbl {}",
            g.width_at(x),
            g.height_at(x),
            g.zc_at(x),
            g.wall,
            g.floor_z(d, x),
            g.ceil_z(d, x),
            d.floor_frac,
            g.floor_z(l, x),
            g.ceil_z(l, x),
            g.zc_at(x) - 0.5 * g.height_at(x) + g.wall,
            g.is_double_deck,
        );
    }
}
