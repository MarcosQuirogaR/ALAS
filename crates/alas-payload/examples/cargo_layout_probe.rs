// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: what cargo arrangement each preset actually builds.
#![allow(clippy::print_stdout, missing_docs)]

use std::collections::BTreeMap;

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemMeta};

fn main() {
    for name in presets::available() {
        let p = presets::get(name).unwrap();
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": p.name })).unwrap();
        config.cabin = p.planning_cabin_config();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&p.design_vector), true)
            .unwrap();
        let layout = match build_payload_layout(&plane, &config, 0.0, 0.0) {
            Ok(layout) => layout,
            Err(error) => {
                println!("{name:<11} payload layout failed: {error:?}");
                continue;
            }
        };
        // Group container positions by hold prefix and by longitudinal station.
        let mut by_hold: BTreeMap<String, Vec<(f64, f64, String)>> = BTreeMap::new();
        let mut bulk = 0usize;
        for item in &layout.items {
            match &item.meta {
                ItemMeta::Container(meta) => {
                    let hold = item
                        .label
                        .split(|c: char| c == '-' || c.is_whitespace())
                        .next()
                        .unwrap_or("?")
                        .to_owned();
                    by_hold
                        .entry(hold)
                        .or_default()
                        .push((item.x, item.y, meta.uld.to_owned()));
                }
                ItemMeta::BulkBag => bulk += 1,
                _ => {}
            }
        }
        let total: usize = by_hold.values().map(Vec::len).sum();
        println!("\n=== {name} === {total} container position(s), {bulk} bulk block(s)");
        for (hold, slots) in &by_hold {
            // distinct longitudinal stations = rows; distinct lateral = abreast
            let mut xs: Vec<f64> = slots.iter().map(|s| s.0).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            let mut ys: Vec<f64> = slots.iter().map(|s| s.1).collect();
            ys.sort_by(f64::total_cmp);
            ys.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            let code = slots.first().map(|s| s.2.clone()).unwrap_or_default();
            println!(
                "  {hold:<6} {:>3} positions  {:>2} row(s) x {:>2} abreast  ULD {code}  x {:.2}..{:.2}  y {:?}",
                slots.len(),
                xs.len(),
                ys.len(),
                xs.first().copied().unwrap_or(0.0),
                xs.last().copied().unwrap_or(0.0),
                ys.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>()
            );
        }
    }
}
