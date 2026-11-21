// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Operating-empty CG by ledger group, per registered preset.
//!
//! Each preset is analyzed in the baseline sandbox at its own design vector,
//! as `tests/oew_cg_station_review.rs` does. Printed per preset: LEMAC/L, the
//! OEW CG, every OEW ledger group (mass, x, x/L, %MAC), the Torenbeek swept
//! wing point (0.7 of the spar box at 0.35 b/2; Scholz, *Aircraft Design*
//! lecture notes ch. 10, Fig. 10.8, after Torenbeek 1988) beside the ledger
//! wing, and the fuselage-group fraction p of L that alone would put the OEW
//! CG at `target` %MAC (default 27.5, the middle of Torenbeek's 25-30 class
//! band), then the cabin: cabin start and length, the
//! furnishings station, and each seat row the layout placed (mass, x, cabin
//! fraction), the main-gear station in %MAC with the empty nose-gear share,
//! and the flown-takeoff CG of the ledger against the lumped model the trim
//! uses. Frame: x m aft of the model nose, %MAC from the model LEMAC. Set
//! `ALAS_PROBE_ITEMS=1` to print every ledger item (mass, x, %MAC, moment)
//! under its group. Set `ALAS_PROBE_CONFIG` to a configuration file to probe
//! that configuration on its preset (a freighter on its passenger donor).
//!
//! ```text
//! cargo run --release -p alas-pipeline --example oew_group_ledger_probe -- [target] [preset ...]
//! ```
#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_opt::ModelCgLoadingState;
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let target = args
        .peek()
        .and_then(|value| value.parse::<f64>().ok())
        .inspect(|_| {
            args.next();
        })
        .unwrap_or(27.5);
    let only: Vec<String> = args.collect();
    for name in presets::available() {
        if !only.is_empty() && !only.iter().any(|wanted| wanted == name) {
            continue;
        }
        probe(name, target);
    }
}

fn probe(name: &str, target: f64) {
    // `ALAS_PROBE_CONFIG`: a configuration file run on this preset (a
    // freighter flown on its passenger donor), instead of the bare preset.
    let document = std::env::var("ALAS_PROBE_CONFIG")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|value| value["preset"] == name)
        .unwrap_or_else(|| serde_json::json!({ "preset": name }));
    let Ok(mut config) = AlasConfig::from_value(&document) else {
        return;
    };
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let Ok(preset) = presets::get(name) else {
        return;
    };
    let design = preset.design_vector;
    let Ok(report) = FullAnalysis::new(config.clone()).run(&design, true) else {
        println!("== {name}: analysis failed");
        return;
    };
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let (Some(statement), Some(envelope), Some(frame)) = (
        physical.mass_balance.as_ref(),
        physical.model_cg.as_ref(),
        report.airplane.mac_frame(),
    ) else {
        println!("== {name}: no mass statement, envelope or MAC frame");
        return;
    };
    let Some(empty) = envelope
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::OperatingEmpty)
    else {
        return;
    };
    let mac = report.airplane.c_ref;
    let length = design.fuselage_length_m;
    let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / mac;
    println!(
        "== {name}: L {length:.2} m, LEMAC {:.3} m ({:.3} L), MAC {mac:.3} m, OEW {:.0} kg at {:.3} m = {:.1} %MAC",
        frame.x_lemac_m,
        frame.x_lemac_m / length,
        empty.mass_kg,
        empty.cg_x_m,
        pct(empty.cg_x_m)
    );

    // OEW groups: every ledger row except the payload.
    let mut groups: Vec<(&str, f64, f64)> = Vec::new();
    for item in statement
        .ledger_items
        .iter()
        .filter(|item| item.group != "payload")
    {
        let moment = item.mass_kg * item.position_m[0];
        match groups.iter_mut().find(|group| group.0 == item.group) {
            Some(group) => {
                group.1 += item.mass_kg;
                group.2 += moment;
            }
            None => groups.push((item.group, item.mass_kg, moment)),
        }
    }
    let (oew_kg, oew_moment) = groups.iter().fold((0.0, 0.0), |(mass, moment), group| {
        (mass + group.1, moment + group.2)
    });
    for (group, mass, moment) in &groups {
        let x = moment / mass.max(1.0e-9);
        println!(
            "  {group:<16} {mass:>9.0} kg  x {x:>7.3} m  {:>5.3} L  {:>7.1} %MAC",
            x / length,
            pct(x)
        );
        if std::env::var_os("ALAS_PROBE_ITEMS").is_some() {
            for item in statement
                .ledger_items
                .iter()
                .filter(|item| item.group == *group)
            {
                let x = item.position_m[0];
                println!(
                    "      {:<44} {:>8.0} kg  x {x:>7.3} m  {:>5.3} L  {:>7.1} %MAC  {:>8.1} kg m",
                    item.id,
                    item.mass_kg,
                    x / length,
                    pct(x),
                    item.mass_kg * x
                );
            }
        }
    }
    cabin_line(&config, statement, length);
    gear_line(&config, statement, (empty.cg_x_m, empty.mass_kg), &pct);
    if let Some(takeoff) = statement
        .states
        .iter()
        .find(|state| state.label == "flown takeoff")
    {
        println!(
            "  takeoff CG: ledger {:.1} % MAC, lumped (trim) {:.1} % MAC",
            takeoff.cg_pct_mac, statement.lumped_takeoff_cg_pct_mac
        );
    }
    if let Some(wing_x) = torenbeek_wing_point_x(&report.airplane, &config) {
        println!(
            "  Torenbeek wing point {wing_x:.3} m = {:.1} %MAC",
            pct(wing_x)
        );
    }
    if let Some((_, fuselage_kg, fuselage_moment)) =
        groups.iter().find(|group| group.0 == "fuselage")
    {
        let target_x = frame.x_lemac_m + target / 100.0 * mac;
        let p = (oew_kg * target_x - (oew_moment - fuselage_moment)) / fuselage_kg / length;
        println!(
            "  fuselage p for {target:.1} %MAC: {p:.3} L (ledger {:.3} L)",
            fuselage_moment / fuselage_kg / length
        );
    }
}

/// The OEW CG against the gear. Printed: the main-gear ledger
/// station in % MAC; the static nose-gear share of the empty aircraft,
/// `(x_main - x_cg) / (x_main - x_nose)`; and the most aft OEW CG, % MAC, at
/// which the nose gear still carries the minimum static share the CG envelope
/// holds (`mass_model.pct_load_nlg_min`).
/// The share uses only the ledger and the gear stations, not the MAC frame.
/// Where that aft cap sits ahead of the class band's forward edge, no mass
/// distribution reaches the band without unloading the nose gear: the band
/// miss is then in where the MAC sits against the gear, not in the ledger.
/// Gear points are the ledger mass stations, not the tyre contact points the
/// envelope uses, so the share is an estimate of the envelope's.
fn gear_line(
    config: &AlasConfig,
    statement: &alas_pipeline::MassBalanceAssessment,
    (cg_x_m, _mass_kg): (f64, f64),
    pct: &dyn Fn(f64) -> f64,
) {
    let station = |id: &str| {
        statement
            .ledger_items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.position_m[0])
    };
    if let (Some(nose), Some(main)) = (station("nose_gear"), station("main_gear")) {
        let minimum = config.mass_model.pct_load_nlg_min;
        println!(
            "  gear: main {main:.3} m = {:.1} %MAC, nose {nose:.3} m; OEW CG {:.3} m ahead of the main gear, empty nose-gear share {:.1} %; aft OEW CG at the {:.1} % minimum share {:.1} %MAC",
            pct(main),
            main - cg_x_m,
            100.0 * (main - cg_x_m) / (main - nose),
            100.0 * minimum,
            pct(main - minimum * (main - nose))
        );
    }
}

/// 0.7 of the spar box behind the front spar at 0.35 of the semispan.
fn torenbeek_wing_point_x(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
) -> Option<f64> {
    let wing = plane.wings.iter().find(|wing| wing.name == "Main Wing")?;
    let (fractions, full_span) = config.structures.resolved_spars();
    let full: Vec<f64> = fractions
        .iter()
        .zip(full_span.iter())
        .filter(|(_, full)| **full)
        .map(|(fraction, _)| *fraction)
        .collect();
    let front = full.iter().copied().reduce(f64::min)?;
    let rear = full.iter().copied().reduce(f64::max)?;
    let y = 0.35 * wing.xsecs.last()?.xyz_le[1];
    wing.xsecs.windows(2).find_map(|pair| {
        let (inboard, outboard) = (&pair[0], &pair[1]);
        let span = outboard.xyz_le[1] - inboard.xyz_le[1];
        if span <= 0.0 || y < inboard.xyz_le[1] || y > outboard.xyz_le[1] {
            return None;
        }
        let t = (y - inboard.xyz_le[1]) / span;
        let le = inboard.xyz_le[0] + t * (outboard.xyz_le[0] - inboard.xyz_le[0]);
        let chord = inboard.chord + t * (outboard.chord - inboard.chord);
        Some(le + (front + 0.7 * (rear - front)) * chord)
    })
}

/// Cabin start and length, the furnishings station and the seat rows placed.
fn cabin_line(config: &AlasConfig, statement: &alas_pipeline::MassBalanceAssessment, length: f64) {
    let fuselage = &config.geometry.fuselage;
    let start = fuselage.cabin_start_x_m;
    let cabin = (length - start - fuselage.tailcone_length_m).max(1.0);
    let station = statement
        .ledger_items
        .iter()
        .find(|item| item.id == "furnishings")
        .map_or(f64::NAN, |item| item.position_m[0]);
    print!("  cabin: start {start:.2} m, length {cabin:.2} m, furnishings station {station:.2} m;");
    for item in statement
        .ledger_items
        .iter()
        .filter(|item| item.id.contains("seats"))
    {
        let x = item.position_m[0];
        print!(
            " {} {:.0} kg at {x:.2} m ({:.2})",
            item.id,
            item.mass_kg,
            (x - start) / cabin
        );
    }
    println!();
}
