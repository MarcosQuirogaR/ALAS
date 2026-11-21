// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe (ignored): the A320 take-off field row in ReferenceAdaptation, in
//! CleanSheet with the engine held at the catalogue rating, and in CleanSheet
//! sized. Prints the row, the mass it is evaluated at, the departure
//! aerodrome and the engine. Run with
//! `cargo test -p alas-opt --test engine_field_probe -- --ignored --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, AlasConfig};
use alas_opt::{assess_product_candidate, canonicalize_design};
use serde_json::json;

fn report(label: &str, config: &AlasConfig) {
    let preset = presets::registry()
        .iter()
        .find(|p| p.name == "A320-200")
        .unwrap();
    let design = canonicalize_design(config, preset.design_vector).unwrap();
    let a = assess_product_candidate(config, &design).unwrap();
    let field = a
        .residuals
        .iter()
        .find(|r| r.id == "takeoff_field")
        .unwrap();
    let airport = config.departure_airport_record();
    let engine = a
        .resolved
        .sized_engine
        .as_ref()
        .map_or(&config.geometry.engine, |sized| &sized.engine);
    eprintln!(
        "PROBE {label}: tofl_m={:.1} toda_m={:.1} design_gross_kg={:.0} takeoff_kg={:.0} rating_kn={:.2} airport={:?}",
        field.limit,
        field.actual,
        a.sized.design_gross_mass_kg,
        a.sized.takeoff_mass_kg,
        engine.scaling_rating().unwrap(),
        airport.map(|x| (x.elevation_m, x.isa_deviation_c, x.toda_m)),
    );
}

#[test]
#[ignore = "probe: prints, asserts nothing"]
fn a320_takeoff_field_by_mode() {
    let base = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
    report("ReferenceAdaptation", &base);
    let mut held = base.clone();
    held.optimizer.design_space.mode = DesignMode::CleanSheet;
    held.optimizer.design_space.engine_sized_by_requirements = false;
    report("CleanSheet held", &held);
    let mut sized = base;
    sized.optimizer.design_space.mode = DesignMode::CleanSheet;
    report("CleanSheet sized", &sized);
    for stated_m in [3_000.0, 2_600.0] {
        let mut stated = sized.clone();
        stated.toda_override_m = Some(stated_m);
        report(
            &format!("CleanSheet sized, stated {stated_m:.0} m"),
            &stated,
        );
    }
}
