// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cabin census of every registered preset: what the passenger layout seats,
//! how long the cabin it seats them in is, and what that does to the empty
//! and loaded aircraft.
//!
//! Each preset is analyzed twice at its registered design vector: in the
//! baseline sandbox, where the planning seat count caps the cabin, and as a
//! clean sheet, where only the exits and the certified maximum do. Per run it
//! reports the seats and their classes, the payload, the operating empty mass
//! and its furnishings group, and the loaded centre of gravity in percent of
//! the model MAC; per preset, the main-deck cabin length, the monument count
//! and the exit-limited seat ceiling. Masses are kg, lengths m.
//!
//! ```text
//! cargo run -p alas-pipeline --release --example cabin_census
//! ```
#![allow(clippy::print_stdout)]

use std::error::Error;

use alas_config::{presets, AlasConfig, DesignMode};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{FURNISHINGS, OEW_KEYS};
use alas_payload::cabin::cabin_deck_segments;
use alas_payload::layout::{ItemKind, LayoutSummary, MAIN};
use alas_pipeline::FullAnalysis;

/// One analysis of one preset, as the census reports it.
struct Run {
    seats: i64,
    classes: String,
    monuments: usize,
    exit_ceiling: i64,
    payload_kg: f64,
    oew_kg: f64,
    furnishings_kg: f64,
    cg_x_m: f64,
}

fn run(
    name: &str,
    mut config: AlasConfig,
    mode: DesignMode,
) -> Result<Option<Run>, Box<dyn Error>> {
    let preset = presets::get(name)?;
    config.optimizer.design_space.mode = mode;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let report = FullAnalysis::new(config).run(&preset.design_vector, true)?;
    let Some(layout) = report.payload_layout.as_ref() else {
        return Ok(None);
    };
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        return Ok(None);
    };
    let monuments = layout
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                ItemKind::Galley | ItemKind::Lav | ItemKind::AccessibleLav
            )
        })
        .count();
    let mass = |key: &str| report.component_masses.get(key).copied().unwrap_or(0.0);
    Ok(Some(Run {
        seats: summary.seated_pax,
        classes: summary
            .classes
            .iter()
            .map(|(class, seats)| format!("{}{}", &class[..1], seats))
            .collect::<Vec<_>>()
            .join("/"),
        monuments,
        exit_ceiling: summary.geometric_capacity,
        payload_kg: layout.total_mass,
        oew_kg: OEW_KEYS.iter().map(|key| mass(key)).sum(),
        furnishings_kg: mass(FURNISHINGS),
        cg_x_m: report.physical_cg[0],
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "| preset | basis | seats | classes | cabin length m | monuments | exit ceiling | payload kg | OEW kg | furnishings kg | CG %MAC |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for name in presets::available() {
        let preset = presets::get(name)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        if config.requirements.aircraft_type != "passenger" {
            continue;
        }
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| format!("{name}: {error:?}"))?;
        let g = alas_payload::build::product_cabin_geometry(&plane, &config)?;
        let cabin_length: f64 = cabin_deck_segments(&g)
            .iter()
            .filter(|segment| segment.deck.name == MAIN)
            .map(|segment| segment.x1 - segment.x0)
            .sum();
        for (basis, mode) in [
            ("sandbox", DesignMode::BaselineSandbox),
            ("clean sheet", DesignMode::CleanSheet),
        ] {
            let Some(row) = run(name, config.clone(), mode)? else {
                continue;
            };
            println!(
                "| {name} | {basis} | {} | {} | {cabin_length:.2} | {} | {} | {:.0} | {:.0} | {:.0} | {:.2} |",
                row.seats,
                row.classes,
                row.monuments,
                row.exit_ceiling,
                row.payload_kg,
                row.oew_kg,
                row.furnishings_kg,
                g.x_to_pct_mac(row.cg_x_m),
            );
        }
    }
    Ok(())
}
