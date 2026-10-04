// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset diagnostics, retaining primary/secondary source and condition metadata.

use alas_config::airports::Airport;
use alas_config::{presets, ActiveEngineModel, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_perf::performance::compute_v_speeds_at_masses;
use alas_pipeline::field_performance::report_field_polar;
use alas_pipeline::field_reference::{
    isa_sea_level_field_reference, report_isa_sea_level_field_reference,
};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};
use serde_json::{json, Value};

pub(super) fn evaluate() -> Result<Vec<Value>, String> {
    let contract: Value = serde_json::from_str(
        &std::fs::read_to_string("golden/aircraft/real_aircraft_parity.json")
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    presets::available()
        .iter()
        .map(|name| {
            Ok(match evaluate_preset(name, &contract) {
                Ok(value) => value,
                Err(error) => json!({"name": name, "error": error}),
            })
        })
        .collect()
}

fn evaluate_preset(name: &str, contract: &Value) -> Result<Value, String> {
    let preset = presets::get(name).map_err(|error| error.to_string())?;
    let config =
        AlasConfig::from_value(&json!({"preset": name})).map_err(|error| error.to_string())?;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|error| error.to_string())?;
    let wing_area = airplane.s_ref;
    let takeoff_mass = config.requirements.mtow_kg;
    let landing_mass = config.landing_mass_limit_kg(takeoff_mass);
    let takeoff_source = presets::published_takeoff_reference(name).map(|reference| {
        json!({
            "mass_kg": reference.mass_kg,
            "reference_area_m2": reference.reference_area_m2,
            "v2_min_m_s": reference.v2_min_m_s,
            "selected_v2_over_vs1g": config.performance.v2_vstall_factor,
            "speed_uncertainty_m_s": reference.speed_uncertainty_m_s,
            "cl_max_to_source_area": reference.cl_max_to(config.performance.v2_vstall_factor),
            "source": reference.source,
            "applicability": reference.applicability,
            "comparison": "Effective model input reconstruction, not independent speed validation",
        })
    });
    let (field_result, trace) = if matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) {
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .map_err(|error| error.to_string())?;
        let trace = propeller_trace(&config, &report, wing_area, takeoff_mass, landing_mass)?;
        (
            report_isa_sea_level_field_reference(
                &config,
                &report,
                wing_area,
                takeoff_mass,
                landing_mass,
            ),
            trace,
        )
    } else {
        // Jet field lengths and Vref are independent of the OEI drag diagnostic.
        (
            isa_sea_level_field_reference(
                &config,
                wing_area,
                0.02,
                0.04,
                takeoff_mass,
                landing_mass,
            ),
            Value::Null,
        )
    };
    let field = match field_result {
        Ok(field) => field,
        Err(error) => {
            return Ok(json!({
                "name": name, "error": error, "wing_area_m2": wing_area,
                "takeoff_mass_kg": takeoff_mass, "landing_mass_kg": landing_mass,
                "cl_max_to": config.performance.cl_max_to,
                "cl_max_to_source": config.performance.cl_max_to_source,
                "cl_max_land": config.performance.cl_max_land,
                "published_takeoff_input": takeoff_source,
                "field_trace": trace,
            }))
        }
    };
    let references: Vec<Value> = contract["aircraft"][name]["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|record| {
            record["category"] == "performance"
                && record["id"].as_str().is_some_and(|id| {
                    id.contains("tofl") || id.contains("lfl") || id.contains("vref")
                })
        })
        .cloned()
        .collect();
    let landing_source = presets::published_landing_reference(name).map(|reference| {
        json!({
            "mass_kg": reference.mass_kg,
            "reference_area_m2": reference.reference_area_m2,
            "vref_m_s": reference.vref_m_s,
            "speed_uncertainty_m_s": reference.speed_uncertainty_m_s,
            "source": reference.source,
            "applicability": reference.applicability,
            "cl_max_land_source_area": reference.cl_max_land(),
            "expected_vref_at_preset_mlw_m_s": reference.vref_m_s * (landing_mass / reference.mass_kg).sqrt(),
            "expected_vref_at_preset_mlw_and_model_area_m_s": reference.vref_m_s
                * (landing_mass / reference.mass_kg * reference.reference_area_m2 / wing_area).sqrt(),
            "comparison": "CLmax input reconstruction, not independent speed validation",
        })
    });
    Ok(json!({
        "name": name,
        "variant": preset.identity.model,
        "wing_area_m2": wing_area,
        "takeoff_mass_kg": takeoff_mass,
        "landing_mass_kg": landing_mass,
        "cl_max_to": config.performance.cl_max_to,
        "cl_max_to_source": config.performance.cl_max_to_source,
        "cl_max_land": config.performance.cl_max_land,
        "tofl_m": field.takeoff_field_length_m,
        "bfl_m": field.model_balanced_field_length_m,
        "lfl_m": field.landing_field_length_m,
        "landing_distance_m": field.landing_distance_m,
        "vref_m_s": field.vref_m_s,
        "vs1g_m_s": field.vs1g_landing_m_s,
        "static_tw": field.static_tw,
        "field_trace": trace,
        "takeoff_basis": field.takeoff_field_length_basis,
        "landing_basis": field.landing_field_length_basis,
        "condition": "ISA sea level, zero wind, dry level runway; configured takeoff/landing lift",
        "published_landing_input": landing_source,
        "published_takeoff_input": takeoff_source,
        "references": references,
        "landing_factor_source_caveat": if name == "ATR72-600" {
            "The historical contract's 1.67 landing-factor annotation is not adopted: the current EU propeller dry share is 0.70 (factor 1.429), while the source factsheet names EASA field length without printing a multiplier."
        } else { "" },
    }))
}

fn propeller_trace(
    config: &AlasConfig,
    report: &alas_pipeline::full_analysis::AnalysisReport,
    wing_area: f64,
    takeoff_mass: f64,
    landing_mass: f64,
) -> Result<Value, String> {
    let airport = Airport::custom("ISA sea level", 0.0, 20_000.0, 20_000.0, 0.0, 0.0, 0.0);
    let perf = &config.performance;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass,
        landing_mass,
        wing_area,
        &airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let atmosphere = alas_atmo::us1976_compute_values(0.0, 0.0);
    let (cd0, k) = report_field_polar(
        config,
        report,
        speeds.v2_ms / atmosphere.speed_of_sound_m_s,
        &airport,
    )?;
    let ActiveEngineModel::Turboprop(spec) = config
        .geometry
        .engine
        .active_model()
        .map_err(|error| error.to_string())?
    else {
        return Ok(Value::Null);
    };
    let model = turboprop_unit_model(spec);
    let evaluate = |rating| {
        model
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: atmosphere.density_kg_m3,
                    true_airspeed_m_s: speeds.v2_ms,
                },
                atmosphere.temperature_k,
                TurbopropCommand {
                    rating,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .map_err(|error| error.to_string())
    };
    let engines = config.geometry.engine.spanwise_positions_m.len();
    let normal = evaluate(Pw127mRating::NormalTakeoff)?;
    let reserve = evaluate(Pw127mRating::MaximumTakeoffReserve)?;
    let aeo_thrust = engines as f64 * normal.total_thrust_n;
    let oei_thrust = engines.saturating_sub(1) as f64 * reserve.total_thrust_n;
    let cl_v2 = perf.cl_max_to * (speeds.v_stall_to_ms / speeds.v2_ms).powi(2);
    let cd_takeoff = cd0 + perf.oei_climb_delta_cd + k * cl_v2.powi(2);
    let weight = takeoff_mass * 9.81;
    Ok(json!({
        "v2_m_s": speeds.v2_ms, "cl_v2": cl_v2,
        "clean_cd0": cd0, "takeoff_cd0": cd0 + perf.oei_climb_delta_cd,
        "induced_drag_factor": k, "cd_at_v2": cd_takeoff,
        "aeo_normal_takeoff_thrust_at_v2_n": aeo_thrust,
        "oei_maximum_takeoff_reserve_thrust_at_v2_n": oei_thrust,
        "normal_per_engine_shaft_power_w": normal.engine_shaft_power_w,
        "normal_per_engine_propeller_power_w": normal.propeller_power_w,
        "normal_propulsive_efficiency": normal.propulsive_efficiency,
        "normal_installed_propeller_eta_power_over_v_n": engines as f64
            * normal.propulsive_efficiency * normal.propeller_power_w / speeds.v2_ms,
        "normal_installed_residual_jet_thrust_n": engines as f64 * normal.residual_jet_thrust_n,
        "takeoff_lift_basis": perf.cl_max_to_source,
        "aeo_gradient_before_extra_failed_engine_drag": aeo_thrust / weight - cd_takeoff / cl_v2,
        "oei_gradient_before_extra_failed_engine_drag": oei_thrust / weight - cd_takeoff / cl_v2,
        "basis": "Actual shared candidate drag and installed propeller deck; same field V2 and ratings as the production caller",
    }))
}
