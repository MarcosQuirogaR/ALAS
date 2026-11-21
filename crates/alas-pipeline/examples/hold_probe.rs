// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lower-hold floor, ceiling and chord along the cabin of one preset. Lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --example hold_probe -- A320-200 [ULD code]
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A320-200".to_owned());
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
    println!("cabin x {:.3} .. {:.3}", g.cabin_start_x, g.cabin_end_x);
    for i in 0..=12 {
        let x = 26.0 + 0.5 * f64::from(i);
        println!(
            "lining x {x:.2} outer width {:.3} chord at armrest {:.3}",
            g.width_at(x),
            g.lining_chord_at_z(x, g.floor_z(&g.passenger_decks[0], x) + 0.65)
        );
    }
    let low = &g.lower_deck;
    let main = &g.passenger_decks[0];
    let mut x = g.cabin_start_x;
    while x <= g.cabin_end_x {
        println!(
            "x {x:6.2} zc {:.3} H {:.3} W {:.3} | main floor {:.3} | hold floor {:.3} ceil {:.3} height {:.3} chord {:.3}",
            g.zc_at(x),
            g.height_at(x),
            g.width_at(x),
            g.floor_z(main, x),
            g.floor_z(low, x),
            g.ceil_z(low, x),
            g.deck_height(low, x),
            g.usable_width(low, x),
        );
        x += 2.0;
    }
    for i in 0..=12 {
        let x = 7.0 + 0.1 * f64::from(i);
        println!("fine x {x:.2} hold height {:.4}", g.deck_height(low, x));
    }
    // A LD3-45 footprint (1.53 m wide, 1.14 m high) on the hold floor at 9.5 m.
    for x in [22.93_f64, 24.57, 25.0, 25.5] {
        let zb = g.floor_z(low, x);
        let h = 1.14;
        let half = 0.765;
        let contour = [[-half, zb], [half, zb], [half, zb + h], [-half, zb + h]];
        println!(
            "x {x}: rectangle {:?}",
            g.check_polygon_containment(x - 0.78, x + 0.78, &contour)
        );
    }
    let manager = alas_payload::CargoLoadManager::new(
        &g,
        alas_config::CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: std::env::args()
                .nth(2)
                .unwrap_or_else(|| "LD3-45".to_owned()),
            ..config.cabin.cargo.clone()
        },
    );
    println!(
        "slots {} config.lower {:?} use_main {}",
        manager.slots.len(),
        config.cabin.cargo.lower_deck_uld,
        config.cabin.cargo.use_main_deck
    );
    for s in &manager.slots {
        println!("  {} x {:.2} y {:.2} {}", s.sid, s.x, s.y, s.uld.code);
    }
}
