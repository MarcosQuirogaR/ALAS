// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The jet class start of a clean-sheet brief (`alas_config::clean_sheet`),
//! evaluated through the product assessment: a brief that names no engine
//! takes a catalogue engine the sizing can scale inside its validity window,
//! and the class installation hangs it clear of the wing and the ground.
//!
//! With the all-engine fleet thrust-to-weight the 72.5 t twin took a PW1500G
//! the sizing had to scale by 1.35 (outside the 20 percent window), its
//! 2.45 m nacelle 0.07 m into the wing and 0.39 m into the ground; the 380 t
//! quad took an engine it had to scale by 0.61. The start is not asked to be
//! feasible: the brief names no design range, so its route is the class
//! airport pair and its mass does not close on it.

// A test asserts on values it built from a brief it controls, so a failed
// unwrap there is the assertion failing.
// The test log carries the installation numbers behind the assertions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use serde_json::{json, Value};

fn brief(
    mtow_kg: f64,
    seats: u32,
    mach: f64,
    altitude_m: f64,
    diameter_m: f64,
    engine: Value,
) -> Value {
    json!({
        "requirements": {
            "mtow_kg": mtow_kg, "num_passengers": seats,
            "cruise_mach": mach, "cruise_altitude_m": altitude_m
        },
        "geometry": {"fuselage": {"diameter_m": diameter_m}, "engine": engine},
        "optimizer": {"design_space": {"mode": "clean_sheet"}}
    })
}

#[test]
fn a_jet_class_start_takes_an_engine_it_can_scale_and_hangs_it_clear() {
    let value = brief(
        72_500.0,
        168,
        0.785,
        11_277.6,
        3.96,
        json!({"spanwise_positions_m": [5.8, -5.8]}),
    );
    let config = AlasConfig::from_value(&value).unwrap();
    assert!(config.derives_clean_sheet_start());
    let design = config.configured_nominal_design();
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let row = |id: &str| {
        assessment
            .residuals
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    };
    for id in [
        "engine_scale_validity",
        "nacelle_clear_of_wing",
        "nacelle_ground_clearance_in_roll",
        "main_gear_clear_of_nacelle",
    ] {
        let r = row(id);
        eprintln!(
            "TWIN {} {id}: actual {} limit {} {}",
            config.geometry.engine.engine_name, r.actual, r.limit, r.unit
        );
        assert!(
            !r.violated(),
            "{id}: actual {} limit {} {}",
            r.actual,
            r.limit,
            r.unit
        );
    }
}

#[test]
fn a_quad_class_start_takes_an_engine_for_its_remaining_three() {
    let value = brief(
        380_000.0,
        380,
        0.84,
        11_300.0,
        6.0,
        json!({"spanwise_positions_m": [20.0, 11.0, -11.0, -20.0]}),
    );
    let config = AlasConfig::from_value(&value).unwrap();
    let design = config.configured_nominal_design();
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let scale = assessment
        .residuals
        .iter()
        .find(|row| row.id == "engine_scale_validity")
        .unwrap()
        .actual;
    // The all-engine ratio's engine needed 0.61; the engine-out one 0.70. The
    // rest of the gap is the class airframe's requirement, which the validity
    // row still reports.
    assert!(scale > 0.65, "{scale}");
    for row in assessment
        .residuals
        .iter()
        .filter(|row| row.id.starts_with("nacelle_"))
    {
        eprintln!(
            "QUAD {} {}: actual {} limit {} {}",
            config.geometry.engine.engine_name, row.id, row.actual, row.limit, row.unit
        );
        assert!(!row.violated(), "{}: {} m", row.id, row.actual);
    }
    eprintln!(
        "QUAD {} engine_scale_validity: actual {scale}",
        config.geometry.engine.engine_name
    );
}

/// A trijet of a DC-10's class (two wing engines and one on the centreline)
/// starts from the engine its 2.7 percent second segment asks for on its two
/// remaining engines: one the sizing scales inside its window. The nacelle
/// rows are reported, not asserted: a centre engine without a declared tail
/// pod is placed on the tail cone by the geometry builder, an installation
/// this start does not choose.
#[test]
fn a_trijet_class_start_takes_an_engine_for_its_remaining_two() {
    let value = brief(
        250_000.0,
        300,
        0.82,
        11_000.0,
        6.0,
        json!({"spanwise_positions_m": [11.0, 0.0, -11.0]}),
    );
    let config = AlasConfig::from_value(&value).unwrap();
    let design = config.configured_nominal_design();
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let rows: Vec<_> = assessment
        .residuals
        .iter()
        .filter(|row| row.id == "engine_scale_validity" || row.id.starts_with("nacelle_"))
        .collect();
    for row in &rows {
        eprintln!(
            "TRIJET {} {}: actual {} limit {} {}",
            config.geometry.engine.engine_name, row.id, row.actual, row.limit, row.unit
        );
    }
    let scale = rows
        .iter()
        .find(|row| row.id == "engine_scale_validity")
        .unwrap();
    assert!(
        !scale.violated(),
        "scale {} limit {}",
        scale.actual,
        scale.limit
    );
}
