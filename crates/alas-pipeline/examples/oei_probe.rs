// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off field of a preset at its maximum take-off mass on an ISA sea-level
//! runway, as the gate and the ISA reference compute it (diagnostic).
//!
//! `cargo run -p alas-pipeline --example oei_probe [preset]`
//! prints the field length and the speeds of `report_isa_sea_level_field_reference`,
//! its second-segment evidence, and the failed-engine drag terms at V2.
//! `OEI_PERF='{"takeoff_gear_delta_cd":0.01}'` overrides performance fields for
//! sensitivity runs.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_config::{presets, AlasConfig};
use alas_opt::mdo::{EngineOutCondition, EngineOutModel, EngineOutRating, YawAirframe};
use alas_pipeline::field_performance::report_field_polar;
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ATR72-600".to_owned());
    let preset = presets::get(&name).unwrap();
    let mut value = json!({"preset": name});
    if let Ok(text) = std::env::var("OEI_PERF") {
        value["performance"] = serde_json::from_str(&text).unwrap();
    }
    let config = AlasConfig::from_value(&value).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let mtow = config.requirements.mtow_kg;
    let reference = report_isa_sea_level_field_reference(
        &config,
        &report,
        report.airplane.s_ref,
        mtow,
        config.landing_mass_limit_kg(mtow),
    )
    .unwrap();
    println!(
        "{name} at {mtow:.0} kg, ISA sea level: field length {:.0} m ({}), balanced {:.0} m; V2 {:.2} m/s = {:.3} VSR (VSR {:.2} m/s)",
        reference.takeoff_field_length_m,
        reference.takeoff_field_length_basis,
        reference.model_balanced_field_length_m,
        reference.v2_m_s,
        reference.v2_m_s / reference.v_stall_takeoff_m_s,
        reference.v_stall_takeoff_m_s
    );
    if let Some(published) = presets::published_takeoff_reference(&name) {
        println!(
            "published V2 minimum {:.2} m/s at {:.0} kg",
            published.v2_min_m_s, published.mass_kg
        );
    }
    println!(
        "second segment: required {:?} available {:?} status {:?}",
        reference.oei.required_gradient, reference.oei.available_gradient, reference.oei.status
    );
    let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    // Mach at V2 on the ISA sea-level atmosphere the reference uses (m-F3: the
    // drag terms of the failed-engine point are quoted at this Mach, not at 0).
    let sea_level_a0_m_s = alas_atmo::us1976_compute_values(0.0, 0.0).speed_of_sound_m_s;
    let polar = report_field_polar(
        &config,
        &report,
        reference.v2_m_s / sea_level_a0_m_s,
        &airport,
    )
    .unwrap();
    let model = EngineOutModel::new(
        &config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        report.airplane.s_ref,
        // The installed static thrust per engine the reference itself resolved.
        Some(reference.static_thrust_n / config.geometry.engine.spanwise_positions_m.len() as f64),
    )
    .unwrap()
    .with_weight_kg(mtow);
    let point = model
        .point(
            EngineOutCondition {
                altitude_m: 0.0,
                isa_deviation_c: 0.0,
                tas_m_s: reference.v2_m_s,
            },
            EngineOutRating::TakeoffGoAround,
            polar.cd0,
        )
        .unwrap();
    println!(
        "failed engine at V2: windmilling/feathered cd {:.4}, asymmetric trim cd {:?} ({:?})",
        point.windmilling_cd, point.asymmetric_trim_cd, point.trim_basis
    );
    turboprop_terms(&config, &model, &reference, &point);
}

/// Installed propeller thrust at V2 behind the second-segment evidence (turboprops).
fn turboprop_terms(
    config: &AlasConfig,
    model: &EngineOutModel,
    reference: &alas_pipeline::field_reference::IsaSeaLevelFieldReference,
    point: &alas_opt::mdo::EngineOutPoint,
) {
    use alas_config::ActiveEngineModel;
    use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};
    let Ok(ActiveEngineModel::Turboprop(spec)) = config.geometry.engine.active_model() else {
        return;
    };
    let unit = alas_opt::mdo::propulsion::turboprop_unit_model(spec);
    let atm = alas_atmo::us1976_compute_values(0.0, 0.0);
    let n = config.geometry.engine.spanwise_positions_m.len() as f64;
    for (label, rating) in [
        (
            "max take-off reserve (OEI)",
            Pw127mRating::MaximumTakeoffReserve,
        ),
        ("normal take-off", Pw127mRating::NormalTakeoff),
        ("max continuous", Pw127mRating::MaximumContinuous),
    ] {
        let out = unit
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: atm.density_kg_m3,
                    true_airspeed_m_s: reference.v2_m_s,
                },
                atm.temperature_k,
                TurbopropCommand {
                    rating,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: unit.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        println!(
            "  {label}: shaft {:.0} kW, propeller {:.0} kW, eta_p {:.3}, J {:.3}, thrust/engine {:.0} N (propeller {:.0} + jet {:.0}), P*eta/V {:.0} N",
            out.engine_shaft_power_w / 1e3,
            out.propeller_power_w / 1e3,
            out.propulsive_efficiency,
            out.advance_ratio,
            out.total_thrust_n,
            out.propeller_thrust_n,
            out.residual_jet_thrust_n,
            out.propeller_power_w * out.propulsive_efficiency / reference.v2_m_s
        );
    }
    let weight_n = reference.takeoff_mass_kg * 9.80665;
    println!(
        "  used: thrust/engine {:.0} N, ratio to static basis {:.3}, static basis {:.0} N/engine ({}); T_oei/W {:.4}; n {n}; model arm {:?}",
        point.thrust_per_engine_n,
        point.thrust_ratio,
        reference.static_thrust_n / n,
        reference.static_thrust_basis,
        point.thrust_per_engine_n * (n - 1.0) / weight_n,
        model.yaw_control().map(|c| (c.balance.engine_arm_m, c.balance.fin_arm_m, c.balance.fin_area_m2, c.balance.fin_effective_aspect_ratio, c.balance.rudder_area_m2))
    );
}
