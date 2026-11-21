// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The seat rows of one preset: station, seats filled, abreast and the row
//! envelope's width. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example seat_rows_probe -- ATR72-600
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemKind, ItemMeta};

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
    let Ok(layout) = build_payload_layout(&plane, &config, 0.0, 0.0) else {
        return;
    };
    for item in layout.items.iter().filter(|i| i.kind == ItemKind::SeatRow) {
        if let ItemMeta::Seat(meta) = &item.meta {
            println!(
                "x {:6.2} filled {} abreast {} width {:.3} z {:.3} h {:.3}",
                item.x, meta.filled, meta.abreast, item.width, item.z, item.height
            );
        }
    }
}
