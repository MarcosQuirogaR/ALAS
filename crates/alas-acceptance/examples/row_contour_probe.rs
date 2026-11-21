// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! For one preset, the seat row whose box leaves the outer fuselage contour
//! the furthest (the `CABIN-SEATS-IN-CONTOUR` rule's own corner test), with
//! the corner, the contour half-width there and the row's occupancy. With a
//! second argument `cargo`, the same for the containers and bulk bags (the
//! `CARGO-IN-CONTOUR` rule). Lengths m.
//!
//! ```text
//! cargo run -p alas-acceptance --example row_contour_probe -- A320-200
//! cargo run -p alas-acceptance --example row_contour_probe -- E195-E2 cargo
//! ```
#![allow(clippy::print_stdout)]

use alas_acceptance::plausibility::geometry::{half_width_at, Station};
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemKind, ItemMeta};

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A320-200".to_owned());
    let Ok(preset) = presets::get(&name) else {
        println!("unknown preset {name}");
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
    let stations: Vec<Station> = plane.fuselages.first().map_or_else(Vec::new, |f| {
        f.xsecs
            .iter()
            .map(|s| Station {
                x: s.xyz_c[0],
                width: s.width,
                height: s.height,
                zc: s.xyz_c[2],
                shape: s.shape,
            })
            .collect()
    });
    let Ok(layout) = build_payload_layout(&plane, &config, 0.0, 0.0) else {
        return;
    };
    if std::env::args().nth(2).as_deref() == Some("cargo") {
        for item in layout
            .items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Uld | ItemKind::Bag))
        {
            let mut margin = f64::INFINITY;
            for dx in [-0.5, 0.0, 0.5] {
                for dz in [-0.5, 0.5] {
                    let x = item.x + dx * item.length;
                    let z = item.z + dz * item.height;
                    let hw = half_width_at(&stations, x, z).unwrap_or(0.0);
                    margin = margin.min(hw - (item.y.abs() + 0.5 * item.width));
                }
            }
            println!(
                "{:?} x {:.2} y {:.2} z {:.2} len {:.2} w {:.2} h {:.2} margin {margin:.3}",
                item.kind, item.x, item.y, item.z, item.length, item.width, item.height
            );
        }
        return;
    }
    let mut worst: Option<(f64, String)> = None;
    let (x_lo, x_hi) = layout
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), i| {
            (lo.min(i.x), hi.max(i.x))
        });
    let mut mid_min = f64::INFINITY;
    for item in layout.items.iter().filter(|i| i.kind == ItemKind::SeatRow) {
        let ItemMeta::Seat(seat) = &item.meta else {
            continue;
        };
        for dx in [-0.5, 0.0, 0.5] {
            for dz in [-0.5, 0.5] {
                let x = item.x + dx * item.length;
                let z = item.z + dz * item.height;
                let hw = half_width_at(&stations, x, z).unwrap_or(0.0);
                let margin = hw - (item.y.abs() + 0.5 * item.width);
                let line = format!(
                    "x {x:.2} z {z:.2} contour half-width {hw:.3} row half-width {:.3} ({} abreast, {} aisles, box h {:.2}) margin {margin:.3}",
                    0.5 * item.width,
                    seat.abreast,
                    seat.aisles,
                    item.height
                );
                if (item.x - 0.5 * (x_lo + x_hi)).abs() < 0.3 * (x_hi - x_lo) {
                    mid_min = mid_min.min(margin);
                }
                if worst.as_ref().is_none_or(|(m, _)| margin < *m) {
                    worst = Some((margin, line));
                }
            }
        }
    }
    if let Some((_, line)) = worst {
        println!(
            "{name}: worst corner {line}; mid-cabin (central 60 %) minimum margin {mid_min:.3}"
        );
    }
}
