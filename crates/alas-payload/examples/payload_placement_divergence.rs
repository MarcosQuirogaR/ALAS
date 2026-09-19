// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The two payload placements the product carries, side by side.
//!
//! `alas_mass::breakdown::complete_mass_analysis` replaces the lumped planning
//! payload with a [`alas_mass::breakdown::PayloadLayoutSummary`] when one is
//! supplied, and keeps the lumped cabin-centre placement when it is not.
//!
//! This probe runs one geometry through both and reports the payload mass, the
//! payload station, the aircraft centre of gravity and its percentage of mean
//! aerodynamic chord under each, plus the seat counts behind them.
//!
//! What it establishes: the two are **one loading definition — every seat
//! occupied at `passenger_mass_kg` — priced on two different passenger counts**.
//! The layout total is `seated_pax * passenger_mass_kg` exactly, `unseated_pax`
//! is zero throughout and the non-occupant share is `0.0 kg` on three of the
//! four, so no belly freight is involved and these are not two design load
//! cases. `DesignRequirements` declares the geometry-resolved count
//! authoritative ("Passenger capacity is always recomputed for each candidate
//! shell"; `resolves_payload_from_candidate_geometry` is unconditionally true),
//! so the lumped arm prices a superseded count.
//!
//! SI throughout: kg, m. `%MAC` is `(x_cg - mac_le_x) / mac * 100`.
//!
//! Run: `cargo run --release -p alas-payload --example payload_placement_divergence`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel,
    PayloadLayoutSummary,
};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;

/// The presets the dispatch asked for, plus the full-cabin control.
const PROBE: [&str; 4] = ["A220-300", "A320-200", "ATR72-600", "A380-800"];

fn main() {
    println!("Two payload placements on one aircraft. SI: kg, m.");
    println!(
        "{:<10} {:>10} {:>10} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>9}",
        "preset",
        "lump_kg",
        "layout_kg",
        "lump_x",
        "layout_x",
        "cg_lump",
        "cg_layout",
        "%MAC_l",
        "%MAC_L",
        "d_%MAC"
    );
    for name in PROBE {
        let Ok(preset) = presets::get(name) else {
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            continue;
        };
        let design = preset.design_vector;
        let Ok(plane) =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true)
        else {
            println!("{name}: geometry did not build");
            continue;
        };
        let model = config.analysis_mass_model(config.requirements.mtow_kg);
        let analyse = |layout: Option<&PayloadLayoutSummary>| {
            run_mass_analysis_with_model_checked_product_with_gear(
                &plane,
                &config.requirements,
                &config.geometry,
                &config.cabin,
                &config.control_surfaces,
                Some(&model),
                layout,
                MassCoordinateModel::StructuralWingbox(&config.structures),
                &config.landing_gear,
            )
        };

        // The lumped path: exactly what `alas-report`'s `quick_preview_report`
        // runs, which is the live CG the interface shows.
        let Ok((lump_masses, lump_coords, lump_cg)) = analyse(None) else {
            println!("{name}: lumped mass analysis failed");
            continue;
        };

        // The detailed path: the first pass gives the operating empty mass and
        // its station, which is what the layout balances the payload against.
        let (oew, x_oew) = oew_and_cg(&lump_masses, &lump_coords);
        let Ok(layout) = build_payload_layout(&plane, &config, oew, x_oew) else {
            println!("{name}: payload layout failed");
            continue;
        };
        let summary = PayloadLayoutSummary {
            total_mass: layout.total_mass,
            cg_x: layout.cg_x,
            cg_y: layout.cg_y,
        };
        let Ok((layout_masses, layout_coords, layout_cg)) = analyse(Some(&summary)) else {
            println!("{name}: detailed mass analysis failed");
            continue;
        };

        let wing = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .or_else(|| plane.wings.first());
        let (mac, mac_le_x) = match wing {
            Some(wing) => (
                wing.mean_aerodynamic_chord(),
                wing.aerodynamic_center(0.0)[0],
            ),
            None => (f64::NAN, f64::NAN),
        };
        let pct = |x: f64| 100.0 * (x - mac_le_x) / mac;

        println!(
            "{name:<10} {:>10.1} {:>10.1} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {:>8.2} {:>8.2} {:>9.2}",
            lump_masses.payload,
            layout_masses.payload,
            lump_coords.payload[0],
            layout_coords.payload[0],
            lump_cg[0],
            layout_cg[0],
            pct(lump_cg[0]),
            pct(layout_cg[0]),
            pct(layout_cg[0]) - pct(lump_cg[0]),
        );
        println!(
            "           fuel lumped={:.1} kg  fuel layout={:.1} kg  payload delta={:+.1} kg  \
             station delta={:+.3} m  layout mode={:?} items={}",
            lump_masses.fuel,
            layout_masses.fuel,
            layout_masses.payload - lump_masses.payload,
            layout_coords.payload[0] - lump_coords.payload[0],
            layout.mode,
            layout.items.len(),
        );
        // Where the payload difference comes from. `num_passengers` priced at
        // `passenger_mass_kg` is exactly the lumped figure, so pricing the
        // layout's own seat count the same way separates the occupant share
        // from anything else without introducing a second model. A non-occupant
        // share near zero is what shows the difference is seats, not freight.
        if let alas_payload::LayoutSummary::Passenger(summary) = &layout.summary {
            let occupants_kg = summary.seated_pax as f64 * config.requirements.passenger_mass_kg;
            println!(
                "           req_pax={} seated_pax={} unseated_pax={} -> occupants={:.1} kg, \
                 non-occupant payload={:.1} kg ({:.1} % of the layout total)",
                config.requirements.num_passengers,
                summary.seated_pax,
                summary.unseated_pax,
                occupants_kg,
                layout.total_mass - occupants_kg,
                100.0 * (layout.total_mass - occupants_kg) / layout.total_mass,
            );
        }
    }
}
