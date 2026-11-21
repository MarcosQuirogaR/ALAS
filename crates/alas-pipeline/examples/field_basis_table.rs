// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The field rows of every preset on the ISA sea-level basis, one JSON object per line.
//!
//! `cargo run --profile harness -p alas-pipeline --example field_basis_table [preset ...]`
//!
//! Basis: dry level hard runway, no wind, CS 25.113(a) take-off field length at the take-off
//! mass and factored landing field length at the landing mass. `FIELD_MASSES="takeoff,landing"`
//! (kg) replaces maximum take-off and landing mass, for a reference quoted at another weight.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let masses: Option<(f64, f64)> = std::env::var("FIELD_MASSES").ok().and_then(|text| {
        let (a, b) = text.split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    });
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        match row(name, masses) {
            Ok(value) => println!("{value}"),
            Err(error) => println!("{}", json!({"name": name, "error": error})),
        }
    }
}

fn row(name: &str, masses: Option<(f64, f64)>) -> Result<serde_json::Value, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|e| e.to_string())?;
    let mtow = config.requirements.mtow_kg;
    let (takeoff, landing) = masses.unwrap_or((mtow, config.landing_mass_limit_kg(mtow)));
    let r =
        report_isa_sea_level_field_reference(&config, &report, airplane.s_ref, takeoff, landing)?;
    let p = &config.performance;
    let field = r.takeoff_field;
    Ok(json!({
        "name": name,
        "s_ref_m2": airplane.s_ref,
        "takeoff_kg": takeoff,
        "landing_kg": landing,
        "cl_clean": p.cl_max_clean,
        "cl_to": p.cl_max_to,
        "cl_land": p.cl_max_land,
        "static_tw": r.static_tw,
        "vsr_ms": r.v_stall_takeoff_m_s,
        "v2_ms": r.v2_m_s,
        "v2_over_vsr": r.v2_m_s / r.v_stall_takeoff_m_s,
        "improved_climb": r.oei.improved_climb,
        "gradient_required": r.oei.required_gradient,
        "gradient_available": r.oei.available_gradient,
        "tofl_m": r.takeoff_field_length_m,
        "bfl_m": r.model_balanced_field_length_m,
        "aeo_m": field.map(|f| f.all_engine_m),
        "asd_m": field.map(|f| f.accelerate_stop_m),
        "go_m": field.map(|f| f.continued_m),
        "v1_ms": field.map(|f| f.v1_m_s),
        "vr_ms": field.map(|f| f.vr_m_s),
        "vref_ms": r.vref_m_s,
        "ldr_m": r.landing_distance_m,
        "lfl_m": r.landing_field_length_m,
        "prop_thrust_kn": propeller_thrust_kn(&config),
    }))
}

/// Per-engine normal take-off thrust, kN, at 0, 20, 40, 60 and 80 m/s TAS at ISA
/// sea level, with propeller power, kW, for a turboprop; `None` for a jet.
fn propeller_thrust_kn(config: &AlasConfig) -> Option<Vec<[f64; 3]>> {
    use alas_config::ActiveEngineModel;
    use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};
    let ActiveEngineModel::Turboprop(spec) = config.geometry.engine.active_model().ok()? else {
        return None;
    };
    let model = alas_opt::mdo::propulsion::turboprop_unit_model(spec);
    let mut rows = Vec::new();
    for speed in [0.0, 20.0, 40.0, 60.0, 80.0] {
        let out = model
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: speed,
                },
                288.15,
                TurbopropCommand {
                    rating: Pw127mRating::NormalTakeoff,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .ok()?;
        rows.push([
            speed,
            out.total_thrust_n / 1000.0,
            out.propeller_power_w / 1000.0,
        ]);
    }
    Some(rows)
}
