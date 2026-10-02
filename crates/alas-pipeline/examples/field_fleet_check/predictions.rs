// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Call the production Rust methods with CADO geometry, mass and propulsion inputs.

use std::f64::consts::PI;

use alas_config::airports::Airport;
use alas_config::{performance_presets, PerformanceConfig};
use alas_perf::performance::{
    compute_field_performance_for_propulsion, compute_propeller_landing_distance,
    compute_v_speeds_at_masses, density_ratio, FieldInputs, FieldPerformance, FieldPropulsion,
};
use alas_prop::turboprop::actuator_disk_thrust_bound_n;
use serde_json::{json, Value};

use super::cado::Aircraft;

pub(super) fn evaluate(aircraft: &Aircraft) -> Value {
    let mut output = aircraft.metadata();
    match predict(aircraft) {
        Ok(prediction) => {
            if let Some(target) = output.as_object_mut() {
                target.extend(prediction);
            }
        }
        Err(error) => output["input_error"] = json!(error),
    }
    output
}

fn predict(aircraft: &Aircraft) -> Result<serde_json::Map<String, Value>, String> {
    let level = match aircraft.category() {
        "narrowbody" => "modern_narrowbody",
        "widebody" => "advanced_highlift_widebody",
        _ => "conservative_simple_flaps",
    };
    let config = &performance_presets::get(level)
        .map_err(|error| error.to_string())?
        .settings;
    let airport = Airport::custom("ISA sea level", 0.0, 20_000.0, 20_000.0, 0.0, 0.0, 0.0);
    let takeoff_mass_kg = aircraft.number("mtow")?;
    let landing_mass_kg = aircraft.number("mlw")?;
    let wing_area_m2 = aircraft.number("wing_area")?;
    let engine_count = aircraft
        .text("n_engine")
        .parse::<usize>()
        .map_err(|error| error.to_string())?;
    let sea_level_static_tw =
        engine_count as f64 * aircraft.number("max_thrust")? / (takeoff_mass_kg * 9.81);
    let inputs = FieldInputs {
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        airport: &airport,
        cl_max_to: config.cl_max_to,
        cl_max_land: config.cl_max_land,
        sea_level_static_tw,
        landing_distance_factor: config.k_land,
        config,
    };
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        &airport,
        config.cl_max_to,
        config.cl_max_land,
        config,
    );
    let mut output = serde_json::Map::new();
    output.insert("cl_max_to".to_owned(), json!(config.cl_max_to));
    output.insert("cl_max_land".to_owned(), json!(config.cl_max_land));
    output.insert("vref_m_s".to_owned(), json!(1.23 * speeds.v_stall_land_ms));
    output.insert("vapp_m_s".to_owned(), json!(1.23 * speeds.v_stall_land_ms));
    if aircraft.text("engine_type") == "turbofan" {
        attach_field(
            &mut output,
            compute_field_performance_for_propulsion(inputs, FieldPropulsion::Jet),
            config,
            false,
            "",
        );
    } else {
        let power_w = aircraft.number("max_power")? * 1_000.0;
        let diameter_m = aircraft.number("rotor_diameter")?;
        let span_m = aircraft.number("wing_span")?;
        if power_w <= 0.0 || diameter_m <= 0.0 || span_m <= 0.0 {
            return Err("propeller power, diameter and span must be positive".to_owned());
        }
        let disk_area_m2 = PI * diameter_m.powi(2) / 4.0;
        let aspect_ratio = span_m.powi(2) / wing_area_m2;
        let density_kg_m3 = 1.225 * density_ratio(0.0, 0.0);
        output.insert("shaft_power_per_engine_w".to_owned(), json!(power_w));
        output.insert("rotor_diameter_m".to_owned(), json!(diameter_m));
        let takeoff_cd0 = 0.02 + config.oei_climb_delta_cd;
        output.insert("zero_lift_drag_coefficient".to_owned(), json!(takeoff_cd0));
        output.insert(
            "induced_drag_factor".to_owned(),
            json!(1.0 / (PI * aspect_ratio)),
        );
        output.insert("propeller_assumptions".to_owned(), json!(
            "CADO shaft rating; ideal momentum disk with speed lapse (McCormick 1967 sec.3.1); zero failed-engine additional drag; clean CD0=0.02 conceptual assumption plus configured takeoff high-lift oei_climb_delta_cd; ideal elliptical span loading e=1; no per-aircraft drag evidence or separate gear increment. The 0.85 effective-disk-power case is a sensitivity assumption, not a sourced propeller calibration."
        ));
        match compute_propeller_landing_distance(inputs) {
            Ok(distance_m) => {
                output.insert("landing_distance_m".to_owned(), json!(distance_m));
                output.insert(
                    "lfl_m".to_owned(),
                    json!(distance_m / config.propeller_dry_landing_distance_share),
                );
                output.insert(
                    "landing_distance_share".to_owned(),
                    json!(config.propeller_dry_landing_distance_share),
                );
            }
            Err(error) => {
                output.insert("landing_error".to_owned(), json!(error));
            }
        }
        for (power_fraction, suffix) in [(1.0, ""), (0.85, "_power85")] {
            let thrust_n = |speed_m_s| {
                Ok(engine_count as f64
                    * actuator_disk_thrust_bound_n(
                        power_fraction * power_w,
                        density_kg_m3,
                        disk_area_m2,
                        speed_m_s,
                    ))
            };
            let field = compute_field_performance_for_propulsion(
                inputs,
                FieldPropulsion::Propeller {
                    engine_count,
                    thrust_n: &thrust_n,
                    engine_out_thrust_n: None,
                    zero_lift_drag_coefficient: takeoff_cd0,
                    induced_drag_factor: 1.0 / (PI * aspect_ratio),
                    failed_engine_drag_coefficient: 0.0,
                },
            );
            attach_field(&mut output, field, config, true, suffix);
        }
    }
    Ok(output)
}

fn attach_field(
    output: &mut serde_json::Map<String, Value>,
    field: Result<FieldPerformance, String>,
    config: &PerformanceConfig,
    propeller: bool,
    suffix: &str,
) {
    match field {
        Ok(field) => {
            let share = if propeller {
                config.propeller_dry_landing_distance_share
            } else {
                0.6
            };
            for (name, value) in [
                ("tofl_m", field.todr_m),
                ("bfl_m", field.bfl_m),
                ("lfl_m", field.ldr_m / share),
                ("landing_distance_m", field.ldr_m),
            ] {
                output.insert(format!("{name}{suffix}"), json!(value));
            }
            output.insert("landing_distance_share".to_owned(), json!(share));
        }
        Err(error) => {
            output.insert(format!("field_error{suffix}"), json!(error));
        }
    }
}
