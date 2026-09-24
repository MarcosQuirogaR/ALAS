// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the interface preview shows, before and after it resolves the cabin.
//!
//! `quick_preview_report` feeds the centre-of-gravity envelope, the
//! landing-gear planform and the control-surface figure. This probe runs the
//! old one-pass form and the current two-pass form on the same geometry and
//! reports the passenger count each priced, the payload and fuel it closed, the
//! payload station, the aircraft centre of gravity in metres and in percentage
//! of mean aerodynamic chord, and the wall time the added cabin layout costs on
//! the calling thread.
//!
//! SI throughout: kg, m. `%MAC = (x_cg - mac_le_x) / mac * 100`, both columns in
//! the same frame, so the frame cancels out of the difference.
//!
//! Run: `cargo run --release -p alas-report --example preview_loading_contract`

// A developer probe whose whole output is the printed comparison.
#![allow(clippy::print_stdout)]

use std::time::Instant;

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel, OEW_KEYS,
};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;
use alas_report::families::mass_balance::quick_preview_report;

const PROBE: [&str; 4] = ["A220-300", "A320-200", "ATR72-600", "A380-800"];

fn main() {
    println!("Interface preview loading contract: one-pass (before) against two-pass (after).");
    println!("SI: kg, m. OEW is the sum of the operating-empty keys.");
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

        // BEFORE: the one-pass form this function used to have, reproduced here
        // so the comparison is against the real previous behaviour rather than
        // against a recollection of it.
        let model = config.analysis_mass_model(config.requirements.mtow_kg);
        let Ok((before_masses, before_coords, before_cg)) =
            run_mass_analysis_with_model_checked_product_with_gear(
                &plane,
                &config.requirements,
                &config.geometry,
                &config.cabin,
                &config.control_surfaces,
                Some(&model),
                None,
                MassCoordinateModel::StructuralWingbox(&config.structures),
                &config.landing_gear,
            )
        else {
            println!("{name}: one-pass preview failed");
            continue;
        };

        // AFTER: the shipped function.
        let started = Instant::now();
        let Ok(after) = quick_preview_report(plane.clone(), &config, design) else {
            println!("{name}: two-pass preview failed");
            continue;
        };
        let two_pass_ms = started.elapsed().as_secs_f64() * 1e3;

        // The cost of the part that is new: one cabin layout on this thread.
        let (oew, x_oew) = oew_and_cg(&before_masses, &before_coords);
        let layout_started = Instant::now();
        let layout = build_payload_layout(&plane, &config, oew, x_oew);
        let layout_ms = layout_started.elapsed().as_secs_f64() * 1e3;

        let after_payload = after
            .component_masses
            .get("Payload")
            .copied()
            .unwrap_or(f64::NAN);
        let after_fuel = after
            .component_masses
            .get("Fuel")
            .copied()
            .unwrap_or(f64::NAN);
        let after_station = after
            .mass_coordinates
            .get("Payload")
            .map_or(f64::NAN, |xyz| xyz[0]);

        // Operating empty mass on each side. The layout substitutes payload and
        // fuel only, so this must be unchanged: read it back off the report's
        // own published component masses rather than off the first pass, or the
        // check is a tautology.
        let oew_after: f64 = OEW_KEYS
            .iter()
            .map(|&key| after.component_masses.get(key).copied().unwrap_or(f64::NAN))
            .sum();

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

        let seated = match layout.as_ref().map(|l| &l.summary) {
            Ok(alas_payload::LayoutSummary::Passenger(summary)) => summary.seated_pax,
            _ => -1,
        };

        println!();
        println!("== {name} ==");
        println!(
            "  pax: requirement={} seated(resolved)={seated}   \
             frame: mac={mac:.4} m, mac_le_x={mac_le_x:.4} m",
            config.requirements.num_passengers,
        );
        println!(
            "  before  payload={:>9.1} kg  fuel={:>9.1} kg  station={:>8.3} m  \
             cg={:>8.3} m  {:>7.2} %MAC",
            before_masses.payload,
            before_masses.fuel,
            before_coords.payload[0],
            before_cg[0],
            pct(before_cg[0]),
        );
        println!(
            "  after   payload={after_payload:>9.1} kg  fuel={after_fuel:>9.1} kg  \
             station={after_station:>8.3} m  cg={:>8.3} m  {:>7.2} %MAC",
            after.physical_cg[0],
            pct(after.physical_cg[0]),
        );
        println!(
            "  delta   payload={:>+9.1} kg  fuel={:>+9.1} kg  station={:>+8.3} m  \
             cg={:>+8.3} m  {:>+7.2} %MAC",
            after_payload - before_masses.payload,
            after_fuel - before_masses.fuel,
            after_station - before_coords.payload[0],
            after.physical_cg[0] - before_cg[0],
            pct(after.physical_cg[0]) - pct(before_cg[0]),
        );
        println!(
            "  closure MTOW-(OEW+payload+fuel)={:+.6} kg   OEW unchanged={}   \
             layout attached={}",
            config.requirements.mtow_kg - (oew_after + after_payload + after_fuel),
            (oew - oew_after).abs() < 1e-9,
            after.payload_layout.is_some(),
        );
        println!("  cost    whole two-pass preview={two_pass_ms:.1} ms  of which cabin layout={layout_ms:.1} ms");
    }
}
