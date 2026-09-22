// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Trace the ATR 72-600 end to end through the production analysis path.
//!
//! The point of this probe is to show that a turboprop now reaches every
//! stage of the product workflow on the production mass architecture, with
//! shaft power and propeller efficiency carried as themselves and no jet
//! thrust anywhere: preset resolution, geometry build, the pure-FLOPS mass
//! buildup with the shaft-power propulsion group, mass properties and CG,
//! and the mission-facing propulsion deck.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-pipeline --example atr72_end_to_end -- out.json
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{ActiveEngineModel, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::{json, Map, Value};
use std::{error::Error, fs, path::PathBuf, time::Instant};

const WATTS_PER_SHP: f64 = 745.699_872;

fn sorted_map(values: &std::collections::HashMap<String, f64>) -> Value {
    let mut map = Map::new();
    let mut names: Vec<&String> = values.keys().collect();
    names.sort();
    for name in names {
        map.insert(name.clone(), json!(values[name]));
    }
    Value::Object(map)
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/atr72-end-to-end.json"));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Stage 1: the registered preset, loaded through the reviewed loader.
    let started = Instant::now();
    let config = AlasConfig::from_value(&json!({"preset": "ATR72-600"}))?;
    let preset = alas_config::presets::get("ATR72-600").map_err(std::io::Error::other)?;
    let engine = match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turboprop(spec)) => spec.clone(),
        Ok(ActiveEngineModel::Turbofan(_)) => {
            return Err("the ATR preset must resolve as a turboprop".into())
        }
        Err(error) => return Err(format!("engine model: {error}").into()),
    };
    // The compatibility thrust field stays at zero by design: nothing on this
    // path may read a jet thrust for a propeller-driven aircraft.
    let compatibility_thrust_kn = config.geometry.engine.thrust_kn();

    // Stage 2: the full production analysis, which is where the turboprop
    // used to stop with `unsupported_propulsion_technology`.
    let analysis = FullAnalysis::new(config.clone());
    let report = analysis
        .run(&preset.design_vector, true)
        .map_err(|error| format!("ATR full analysis: {error}"))?;
    let elapsed_s = started.elapsed().as_secs_f64();

    let buildup = report
        .flops_mass_buildup
        .as_ref()
        .ok_or("the production architecture must publish a FLOPS mass buildup")?;
    let turboprop_group = buildup
        .airframe
        .turboprop_propulsion
        .ok_or("the ATR must carry the shaft-power propulsion group")?;
    if buildup.airframe.propulsion.is_some() {
        return Err("a turboprop must not carry a thrust-based FLOPS propulsion group".into());
    }

    let oew_kg: f64 = alas_mass::breakdown::OEW_KEYS
        .iter()
        .filter_map(|name| report.component_masses.get(*name))
        .sum();

    // The explicit comparison control: the same geometry and design vector
    // under the legacy reference-compatible architecture, which is the only
    // mass model the ATR could reach before the shaft-power group existed.
    let mut legacy_config = config.clone();
    legacy_config.mass_model.mass_architecture =
        alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
    legacy_config.mass_model.apply_architecture();
    let legacy = match FullAnalysis::new_reference_compatibility(legacy_config)
        .run(&preset.design_vector, true)
    {
        Ok(control) => {
            let control_oew: f64 = alas_mass::breakdown::OEW_KEYS
                .iter()
                .filter_map(|name| control.component_masses.get(*name))
                .sum();
            json!({
                "status": "evaluated",
                "oew_kg": control_oew,
                "component_masses_kg": sorted_map(&control.component_masses),
                "physical_cg_m": control.physical_cg,
                "static_margin": control.static_margin,
                "x_neutral_point_m": control.x_neutral_point,
                "cg_envelope_ok": control.cg_envelope_ok,
            })
        }
        Err(error) => json!({"status": "unavailable", "reason": error}),
    };

    let report_json = json!({
        "schema_version": 1,
        "generated_by": "cargo run -p alas-pipeline --example atr72_end_to_end",
        "aircraft": "ATR 72-600 (ATR 72-212A)",
        "elapsed_s": elapsed_s,
        "stage_1_configuration": {
            "mass_architecture": format!("{:?}", config.mass_model.mass_architecture),
            "takeoff_shaft_power_per_engine_kw": engine.takeoff_shaft_power_kw,
            "takeoff_shaft_power_per_engine_shp":
                engine.takeoff_shaft_power_kw * 1_000.0 / WATTS_PER_SHP,
            "maximum_cruise_shaft_power_kw": engine.maximum_cruise_shaft_power_kw,
            "maximum_cruise_fuel_flow_kg_h": engine.maximum_cruise_fuel_flow_kg_h,
            "propeller_model": engine.propeller_model,
            "propeller_diameter_m": engine.propeller_diameter_m,
            "governed_propeller_speed_rpm": engine.governed_propeller_speed_rpm,
            "engine_count": config.geometry.engine.spanwise_positions_m.len(),
            "compatibility_sea_level_static_thrust_kn": compatibility_thrust_kn,
            "mtow_kg": config.requirements.mtow_kg,
        },
        "stage_2_mass": {
            "component_masses_kg": sorted_map(&report.component_masses),
            "oew_kg": oew_kg,
            "resolved_rated_thrust_per_engine_n": buildup.inputs.rated_thrust_per_engine_n,
            "turboprop_propulsion_group_kg": {
                "engine_mass_source": turboprop_group.engine_mass_source,
                "engine_each": turboprop_group.engine_each_kg,
                "engines": turboprop_group.engines_kg,
                "gearboxes": turboprop_group.gearboxes_kg,
                "propeller_each": turboprop_group.propeller_each_kg,
                "propellers": turboprop_group.propellers_kg,
                "nacelles": turboprop_group.nacelles_kg,
                "pylons": turboprop_group.pylons_kg,
                "engine_installation": turboprop_group.engine_installation_kg,
                "fuel_system": turboprop_group.fuel_system_kg,
                "unusable_fuel": turboprop_group.unusable_fuel_kg,
                "total_without_nacelles": turboprop_group.total_without_nacelles_kg,
            },
            "operating_items_kg": {
                "flight_crew_and_baggage":
                    buildup.systems_and_operating_items.operating_items.flight_crew_and_baggage_kg,
                "cabin_crew_and_baggage":
                    buildup.systems_and_operating_items.operating_items.cabin_crew_and_baggage_kg,
                "unusable_fuel":
                    buildup.systems_and_operating_items.operating_items.unusable_fuel_kg,
                "engine_oil": buildup.systems_and_operating_items.operating_items.engine_oil_kg,
                "passenger_service":
                    buildup.systems_and_operating_items.operating_items.passenger_service_kg,
                "cargo_containers":
                    buildup.systems_and_operating_items.operating_items.cargo_containers_kg,
                "total": buildup.systems_and_operating_items.operating_items.total_kg,
            },
        },
        "stage_3_mass_properties": {
            "physical_cg_m": report.physical_cg,
            "static_margin": report.static_margin,
            "x_neutral_point_m": report.x_neutral_point,
            "cg_envelope_ok": report.cg_envelope_ok,
            "mass_coordinate_count": report.mass_coordinates.len(),
        },
        // The legacy comparison control on the same geometry and design
        // vector. It is the only way to say whether a mass-property result is
        // a consequence of the new shaft-power propulsion group or of the
        // aircraft's geometry, which both architectures share.
        "stage_3b_legacy_comparison_control": legacy,
        "stage_4_aerodynamics": {
            "design_point_cl": report.design_point.cl,
            "design_point_cd": report.design_point.cd,
            "lift_to_drag": report.design_point.cl / report.design_point.cd.max(1e-12),
        },
        "validation_scope": "model verification and end-to-end availability. \
    The propeller coefficient surface is a generic six-blade surrogate, the fuel \
    model carries one aircraft-level anchor, and the propulsion-group masses \
    include two values calibrated to a NASA GASP ATR 42-600 group statement. \
    None of this is physical validation against a weighed aircraft.",
    });
    fs::write(
        &output_path,
        serde_json::to_string_pretty(&report_json)? + "\n",
    )?;
    println!("{}", output_path.display());
    println!("ATR 72-600 OEW {oew_kg:.1} kg in {elapsed_s:.2} s");
    Ok(())
}
