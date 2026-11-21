// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed cargo capacity and horizontal-tail geometry of one or more
//! presets or configuration files, without the mission: hold and usable
//! volumes, ULD positions, compartment limits, every lower-deck position, and
//! the tailplane area, quarter-MAC arm and volume coefficient. Lengths m,
//! areas m2, volumes m3, masses kg.
//!
//! ```text
//! cargo run -p alas-acceptance --example capacity_probe -- A220-300 DC-10 cfg/DC-10-30F.json
//! ```
//! An argument ending in `.json` is read as a configuration file; anything
//! else is a preset name. `--slots` also lists every position.
#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, LayoutSummary};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let slots = args.iter().any(|a| a == "--slots");
    for arg in args.iter().filter(|a| !a.starts_with("--")) {
        probe(arg, slots);
    }
}

fn probe(arg: &str, list_slots: bool) {
    let value = if arg.ends_with(".json") {
        match std::fs::read_to_string(arg)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            Some(v) => v,
            None => {
                println!("{arg}: unreadable configuration");
                return;
            }
        }
    } else {
        serde_json::json!({ "preset": arg })
    };
    let Ok(config) = AlasConfig::from_value(&value) else {
        println!("{arg}: configuration rejected");
        return;
    };
    let Ok(preset) = presets::get(&config.preset) else {
        println!("{arg}: unknown preset {}", config.preset);
        return;
    };
    let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
    else {
        println!("{arg}: geometry build failed");
        return;
    };
    let (vh, _) = alas_stab::trim::tail_volume_coefficients(&plane);
    let sh = plane
        .wings
        .get(1)
        .map(|w| w.reference_planform().reference_area());
    println!(
        "{arg}: S {:.2} cbar {:.3} Sh {} Vh {}",
        plane.s_ref,
        plane.c_ref,
        sh.map_or("-".to_owned(), |v| format!("{v:.3}")),
        vh.map_or("-".to_owned(), |v| format!("{v:.3}"))
    );
    let Ok(layout) = build_payload_layout(&plane, &config, 0.0, 0.0) else {
        println!("{arg}: payload layout failed");
        return;
    };
    let cap = match &layout.summary {
        LayoutSummary::Passenger(s) => s.hold_capacity,
        LayoutSummary::Cargo(s) => s.installed,
    };
    println!(
        "  hold {:.2} usable {:.2} (containers {:.2}, bulk {:.2}) ULD lower {} main {} capable {} bulk positions {} limit {:?} net {:.0}",
        cap.hold_volume_m3,
        cap.usable_volume_m3(),
        cap.container_usable_volume_m3,
        cap.bulk_usable_volume_m3,
        cap.lower_deck_uld_positions,
        cap.main_deck_uld_positions,
        cap.lower_deck_container_capable_positions,
        cap.bulk_positions,
        cap.structural_limit_kg,
        cap.net_capacity_kg
    );
    if list_slots {
        let Ok(g) = alas_payload::build::product_cabin_geometry(&plane, &config) else {
            return;
        };
        // Lower-hold section every 2 m: floor and ceiling heights and the
        // usable width at heights above the floor.
        let low = &g.lower_deck;
        let mut x = g.cabin_start_x;
        while x < g.cabin_end_x {
            let (floor, ceil) = (g.floor_z(low, x), g.ceil_z(low, x));
            let widths: Vec<String> = [0.0, 0.3, 0.6, 0.9, 1.2, 1.5, 1.63]
                .iter()
                .map(|h| format!("{:.2}", g.usable_width_at_z(x, floor + h)))
                .collect();
            println!(
                "    section x {x:.1} floor {floor:.3} ceil {ceil:.3} (rule {:.3}) clear {:.3} floor-width {:.3} widths@0/.3/.6/.9/1.2/1.5/1.63 {} | zc {:.3} h {:.3} w {:.3} main-floor {:.3}",
                g.zc_at(x) + low.ceil_frac * g.internal_half_height(x),
                ceil - floor,
                g.usable_width(low, x),
                widths.join(" "),
                g.zc_at(x),
                g.height_at(x),
                g.width_at(x),
                g.passenger_decks.first().map_or(f64::NAN, |d| g.floor_z(d, x))
            );
            x += if x > g.cabin_end_x - 14.0 { 0.5 } else { 2.0 };
        }
        let x_ref = 0.5 * (g.cabin_start_x + g.cabin_end_x);
        for v in g.hold_vertex_clearances(x_ref) {
            println!(
                "    hold vertex {:>10} y {:.3} z {:.3} clearance to the lining {:+.3} m",
                v.name, v.y_m, v.z_m, v.clearance_m
            );
        }
        let manager = alas_payload::CargoLoadManager::new(&g, config.cabin.cargo.clone());
        for s in &manager.slots {
            println!(
                "    {} {} x {:.2} y {:.2} {} h {:.3} w {:.3} vol {:.2}",
                s.deck,
                s.sid,
                s.x,
                s.y,
                s.uld.code,
                s.realized_height_m,
                s.realized_width_m,
                s.usable_volume_m3()
            );
        }
    }
}
