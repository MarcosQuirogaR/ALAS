// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the drawn cabin section and the lofted skin of one preset disagree:
//! the outer section at the middle of the cabin, the horizontal gap between the
//! drawn hold polygon and the skin at its floor corner, chamfer top and ceiling
//! corner, and, along the cabin, the stations where the skin leaves the hold
//! narrower than its drawing. Lengths m.
//!
//! ```text
//! cargo run -p alas-acceptance --example section_vs_loft_probe -- A340-300
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_acceptance::plausibility::geometry::{half_width_at, Station};
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A340-300".to_owned());
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
    let Ok(g) = product_cabin_geometry(&plane, &config) else {
        return;
    };
    let x_ref = 0.5 * (g.cabin_start_x + g.cabin_end_x);
    let main = &g.passenger_decks[0];
    let low = &g.lower_deck;
    println!(
        "{name}: outer {:.3} x {:.3} m at x {x_ref:.2}; main floor {:.3} m below the crown",
        g.width_at(x_ref),
        g.height_at(x_ref),
        g.zc_at(x_ref) + 0.5 * g.height_at(x_ref) - g.floor_z(main, x_ref)
    );
    let Some(hold) = g.hold else {
        println!("  no hold polygon");
        return;
    };
    let floor = g.floor_z(low, x_ref);
    let skin = |x: f64, z: f64| half_width_at(&stations, x, z).unwrap_or(0.0);
    for (label, above, half) in [
        ("floor corner", 0.0, hold.floor_half_m),
        ("chamfer top", hold.chamfer_height_m, hold.side_half_m),
        ("ceiling corner", hold.height_m, hold.side_half_m),
    ] {
        println!(
            "  {label:15} drawn half-width {half:.3} skin {:.3} gap {:.3}",
            skin(x_ref, floor + above),
            skin(x_ref, floor + above) - half
        );
    }
    // Along the cabin: the hold chord the skin leaves against the drawn floor.
    let mut x = g.cabin_start_x;
    let mut narrowed: Option<(f64, f64)> = None;
    let mut lines = Vec::new();
    while x <= g.cabin_end_x {
        let chord = g.usable_width_at_z(x, g.floor_z(low, x) + 1e-3);
        let drawn = 2.0 * hold.floor_half_m;
        if chord < drawn - 1e-6 {
            narrowed.get_or_insert((x, chord));
            lines.push(format!(
                "    x {x:6.2} hold floor chord {chord:.3} (drawn {drawn:.3}, short {:.3})",
                drawn - chord
            ));
        }
        x += 1.0;
    }
    match narrowed {
        Some((x0, c)) => {
            println!("  hold floor narrower than drawn from x {x0:.2} (chord {c:.3}):");
            for line in lines {
                println!("{line}");
            }
        }
        None => println!("  hold floor chord equals the drawn floor over the whole cabin"),
    }
}
