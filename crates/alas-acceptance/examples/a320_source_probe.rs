// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The A320 source maximum-payload load case of the acceptance matrix
//! (180 seats, 2,682 kg of belly cargo, LD3-45 hold): the containers placed
//! and what each carries. Lengths m, masses kg.
//!
//! ```text
//! cargo run -p alas-acceptance --example a320_source_probe
//! ```
#![allow(clippy::print_stdout, reason = "a probe prints its table")]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, LayoutSummary};

fn main() {
    let Ok(preset) = presets::get("A320-200") else {
        return;
    };
    let Ok(mut config) = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
    else {
        return;
    };
    config.cabin.passenger.belly_cargo_kg = 2_682.0;
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::CleanSheet;
    config.cabin.cargo.lower_deck_uld = "LD3-45".to_owned();
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
            "seats {} hold_ulds {} bag {:.3} t belly {:.3} t cg {:.3}",
            s.seated_pax, s.hold_ulds, s.bag_mass_t, s.belly_cargo_t, layout.cg_x
        );
    }
    for item in layout.items.iter().filter(|i| i.deck == "lower") {
        println!(
            "  {:?} x {:6.2} y {:5.2} len {:.2} mass {:7.1}",
            item.kind, item.x, item.y, item.length, item.mass
        );
    }
}
