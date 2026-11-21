// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ATR 72-600 main-deck layout: where the seat rows end and every
//! monument and baggage block stands. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example atr_layout_probe
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemKind, LayoutSummary};

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
    if let LayoutSummary::Passenger(s) = &layout.summary {
        println!(
            "seats {} lavatories {} galleys {} dropped {:?} cg {:.3}",
            s.seated_pax, s.lavatories, s.galleys, s.dropped_monuments, layout.cg_x
        );
    }
    for item in layout.items.iter().filter(|i| i.deck == "main") {
        if matches!(item.kind, ItemKind::SeatRow) {
            continue;
        }
        println!(
            "{:?} {} x {:.2}..{:.2} y {:.2} w {:.2} h {:.2}",
            item.kind,
            item.label,
            item.x - 0.5 * item.length,
            item.x + 0.5 * item.length,
            item.y,
            item.width,
            item.height
        );
    }
    let last = layout
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow)
        .map(|i| i.x + 0.5 * i.length)
        .fold(f64::NEG_INFINITY, f64::max);
    println!("last seat row aft edge {last:.2}");
}
