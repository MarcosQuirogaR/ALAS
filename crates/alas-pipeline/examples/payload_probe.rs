// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Payload-only census of every registered preset: seats against the exit
//! limit and the type-certificate maximum, and the installed lower-hold
//! container positions per hold with the hold stations and volumes.
//!
//! No performance analysis is run, so this takes seconds where the full model
//! reference dump takes minutes. Lengths m, volumes m3.
//!
//! ```text
//! cargo run -p alas-pipeline --example payload_probe
//! ```
#![allow(clippy::print_stdout)]

use std::error::Error;

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::cargo::derive_hold_compartments;
use alas_payload::{build_payload_layout, CargoLoadManager, LayoutSummary};

fn main() -> Result<(), Box<dyn Error>> {
    for name in presets::available() {
        let preset = presets::get(name)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| format!("{name}: {error:?}"))?;
        let g = alas_payload::build::product_cabin_geometry(&plane, &config)?;
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .map_err(|error| format!("{name}: {error:?}"))?;
        if let LayoutSummary::Passenger(s) = &layout.summary {
            let tcds = preset.reference.certified_max_seats;
            let certified = tcds.map_or(s.geometric_capacity, |cap| s.geometric_capacity.min(cap));
            println!(
                "{name}: classes {:?} seats {} planning {:?} exit_limit {} tcds {:?} certified_max {} binding {} layout {:?}",
                s.classes,
                s.seated_pax,
                preset.reference.planning_seats,
                s.geometric_capacity,
                tcds,
                certified,
                s.capacity_binding,
                s.source_exit_layout,
            );
        }
        let manager = CargoLoadManager::new(
            &g,
            alas_config::CargoDeckConfig {
                use_main_deck: false,
                ..config.cabin.cargo.clone()
            },
        );
        let (wb0, wb1) = g.lower_hold_gap_x_range();
        let count = |prefix: &str, bulk: bool| {
            manager
                .slots
                .iter()
                .filter(|s| s.sid.starts_with(prefix) && (s.uld.code == "BLK") == bulk)
                .count()
        };
        let extent = |prefix: &str| {
            let xs: Vec<f64> = manager
                .slots
                .iter()
                .filter(|s| s.sid.starts_with(prefix))
                .flat_map(|s| [s.x - 0.5 * s.uld.length, s.x + 0.5 * s.uld.length])
                .collect();
            let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (lo, hi)
        };
        let comps = derive_hold_compartments(
            &g,
            &manager.slots,
            None,
            &config.cabin.cargo.hold_compartments,
        );
        let volume: f64 = comps.iter().map(|c| c.volume_m3).sum();
        let ellipse: f64 = comps
            .iter()
            .filter(|c| c.deck == alas_config::HoldDeck::Lower)
            .map(|c| {
                let n = 200;
                let dx = (c.x_end_m - c.x_start_m) / n as f64;
                (0..n)
                    .map(|i| {
                        let x = c.x_start_m + (i as f64 + 0.5) * dx;
                        let Some((a, b)) = g.inner_semi_axes(x) else {
                            return 0.0;
                        };
                        let zc = g.zc_at(x);
                        let u1 = ((g.floor_z(&g.lower_deck, x) - zc) / b).clamp(-1.0, 1.0);
                        let u2 = ((g.ceil_z(&g.lower_deck, x) - zc) / b).clamp(-1.0, 1.0);
                        let f = |u: f64| u * (1.0 - u * u).sqrt() + u.asin();
                        a * b * (f(u2) - f(u1)) * dx
                    })
                    .sum::<f64>()
            })
            .sum();
        println!(
            "  cargo: uld {} fwd {} (bulk {}) aft {} (bulk {}) cabin x {:.2}..{:.2} wingbox {:.2}..{:.2} fwd {:.2?} aft {:.2?} lower deck height mid {:.2} width {:.2} total hold volume {:.1} m3 (elliptic section {:.1})",
            manager.lower_uld.code,
            count("FWD-", false),
            count("FWD-", true),
            count("AFT-", false),
            count("AFT-", true),
            g.cabin_start_x,
            g.cabin_end_x,
            wb0,
            wb1,
            extent("FWD-"),
            extent("AFT-"),
            g.deck_height(&g.lower_deck, 0.5 * (g.cabin_start_x + g.cabin_end_x)),
            g.usable_width(&g.lower_deck, 0.5 * (g.cabin_start_x + g.cabin_end_x)),
            volume,
            ellipse,
        );
    }
    Ok(())
}
