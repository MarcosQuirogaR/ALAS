// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cabin floor and seat-containment census of every passenger preset: the
//! main-deck floor height along the seat rows (level or sloping) and how many
//! rows protrude through the wall-inset fuselage ellipse at armrest and
//! shoulder height. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example payload_floor_probe
//! ```
#![allow(clippy::print_stdout)]

use std::error::Error;

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::{build_payload_layout, ItemKind, ItemMeta};

/// The model's cabin floor datum is the armrest plane of the physical cabin
/// (the section's widest chord), so the armrest check is at the datum and the
/// shoulder check is the physical shoulder-minus-armrest height above it.
const ARMREST_M: f64 = alas_payload::geometry::ARMREST_ABOVE_FLOOR_M;
const SHOULDER_M: f64 = alas_payload::geometry::SHOULDER_ABOVE_FLOOR_M;

fn main() -> Result<(), Box<dyn Error>> {
    for name in presets::available() {
        let preset = presets::get(name)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        if config.requirements.aircraft_type == "cargo" {
            continue;
        }
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| format!("{name}: {error:?}"))?;
        let g = product_cabin_geometry(&plane, &config)?;
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .map_err(|error| format!("{name}: {error:?}"))?;
        let deck = &g.passenger_decks[0];
        let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut rows, mut bad_arm, mut bad_sh) = (0, 0, 0);
        let (mut worst_arm, mut worst_sh) = (0.0_f64, 0.0_f64);
        for item in layout.items.iter().filter(|i| i.kind == ItemKind::SeatRow) {
            let ItemMeta::Seat(seat) = &item.meta else {
                continue;
            };
            if item.deck != deck.name {
                continue;
            }
            rows += 1;
            let (a, b) = (item.x - 0.5 * item.length, item.x + 0.5 * item.length);
            let floor = g.floor_z(deck, a).max(g.floor_z(deck, b));
            zmin = zmin.min(g.floor_z(deck, a));
            zmax = zmax.max(g.floor_z(deck, a));
            let occupied = seat.abreast as f64 * seat.seat_w + seat.aisles as f64 * seat.aisle_w;
            for (h, bad, worst) in [
                (ARMREST_M, &mut bad_arm, &mut worst_arm),
                (SHOULDER_M, &mut bad_sh, &mut worst_sh),
            ] {
                let mut over = 0.0_f64;
                for x in [a, 0.5 * (a + b), b] {
                    let chord = g.usable_width_at_z(x, floor + h);
                    let overhang = if h > ARMREST_M {
                        2.0 * alas_payload::geometry::shoulder_overhang_m(seat.seat_w)
                    } else {
                        0.0
                    };
                    over = over.max(occupied + overhang - chord);
                }
                if over > 1e-9 {
                    *bad += 1;
                    *worst = worst.max(over);
                }
            }
        }
        println!(
            "{name}: rows {rows} floor z {zmin:.3}..{zmax:.3} (drop {:.3}) armrest-violations {bad_arm} (worst {:.3} m) shoulder-violations {bad_sh} (worst {:.3} m)",
            zmax - zmin,
            worst_arm,
            worst_sh,
        );
    }
    Ok(())
}
