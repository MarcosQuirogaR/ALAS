// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Emit the ATR 72-600 turboprop operating matrix as auditable evidence.
//!
//! The matrix sweeps the certified PW127M ratings over the altitudes and true
//! airspeeds the ATR 72-600 actually flies, and records, per engine, the
//! free-turbine shaft power, the accessory and gearbox split, the propeller
//! power, the propeller thrust, the propulsive efficiency, the advance ratio
//! and the fuel flow. Nothing here converts shaft power into a jet thrust: the
//! residual core thrust is reported on its own line and is zero unless the
//! installation declares one.
//!
//! This is model evidence, not aircraft validation. The propeller coefficient
//! surface is an explicitly generic six-blade surrogate and the fuel model
//! carries a single aircraft-level anchor; both are labelled as such by
//! [`alas_prop::turboprop`] itself.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-prop --example atr72_operating_matrix -- out.json
//! ```

#![allow(clippy::print_stdout)]

use alas_atmo::Atmosphere;
use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
};
use serde_json::{json, Value};
use std::{error::Error, fs, path::PathBuf};

const KNOT_M_S: f64 = 0.514_444_444_444_444_4;
const FOOT_M: f64 = 0.3048;

fn rating_name(rating: Pw127mRating) -> &'static str {
    match rating {
        Pw127mRating::NormalTakeoff => "normal_takeoff",
        Pw127mRating::MaximumTakeoffReserve => "maximum_takeoff_reserve",
        Pw127mRating::MaximumContinuous => "maximum_continuous",
        Pw127mRating::MaximumClimb => "maximum_climb",
        Pw127mRating::MaximumCruise => "maximum_cruise",
        Pw127mRating::FlightIdleSurrogate => "flight_idle_surrogate",
    }
}

/// One evaluated operating point, or the typed refusal that replaced it.
fn point(
    model: Pw127m568fModel,
    altitude_ft: f64,
    true_airspeed_kt: f64,
    rating: Pw127mRating,
) -> Value {
    let altitude_m = altitude_ft * FOOT_M;
    let atmosphere = Atmosphere::new(altitude_m);
    let true_airspeed_m_s = true_airspeed_kt * KNOT_M_S;
    let condition = TurbopropCondition {
        density_kg_m3: atmosphere.density(),
        true_airspeed_m_s,
    };
    let command = TurbopropCommand {
        rating,
        power_fraction: 1.0,
        mode: TurbopropMode::Governed,
        propeller_speed_rpm: model.governed_propeller_speed_rpm,
    };
    let common = json!({
        "altitude_ft": altitude_ft,
        "true_airspeed_kt": true_airspeed_kt,
        "rating": rating_name(rating),
        "density_kg_m3": atmosphere.density(),
        "temperature_k": atmosphere.temperature(),
    });
    let mut row = common.as_object().cloned().unwrap_or_default();
    match model.evaluate(condition, command) {
        Ok(output) => {
            // Two engines: the aircraft-level fuel flow and thrust the mission
            // integrator consumes, beside the per-engine quantities.
            row.insert("status".into(), json!("evaluated"));
            row.insert(
                "engine_shaft_power_kw".into(),
                json!(output.engine_shaft_power_w / 1_000.0),
            );
            row.insert(
                "engine_shaft_power_shp".into(),
                json!(output.engine_shaft_power_w / 745.699_872),
            );
            row.insert(
                "accessory_power_kw".into(),
                json!(output.accessory_power_w / 1_000.0),
            );
            row.insert(
                "gearbox_loss_kw".into(),
                json!(output.gearbox_loss_w / 1_000.0),
            );
            row.insert(
                "propeller_power_kw".into(),
                json!(output.propeller_power_w / 1_000.0),
            );
            row.insert(
                "propeller_torque_n_m".into(),
                json!(output.propeller_torque_n_m),
            );
            row.insert(
                "propeller_thrust_n".into(),
                json!(output.propeller_thrust_n),
            );
            row.insert(
                "residual_jet_thrust_n".into(),
                json!(output.residual_jet_thrust_n),
            );
            row.insert(
                "total_thrust_per_engine_n".into(),
                json!(output.total_thrust_n),
            );
            row.insert(
                "two_engine_thrust_n".into(),
                json!(2.0 * output.total_thrust_n),
            );
            row.insert(
                "propulsive_efficiency".into(),
                json!(output.propulsive_efficiency),
            );
            row.insert("advance_ratio".into(), json!(output.advance_ratio));
            row.insert("blade_angle_deg".into(), json!(output.blade_angle_deg));
            row.insert(
                "fuel_flow_per_engine_kg_h".into(),
                json!(output.fuel_flow_kg_s * 3_600.0),
            );
            row.insert(
                "two_engine_fuel_flow_kg_h".into(),
                json!(2.0 * output.fuel_flow_kg_s * 3_600.0),
            );
            // Reported per point so the matrix shows on its face that the
            // specific consumption is the same constant everywhere.
            row.insert("psfc_kg_kwh".into(), json!(output.psfc_kg_kwh));
            row.insert(
                "power_balance_residual_w".into(),
                json!(output.power_balance_residual_w),
            );
        }
        Err(error) => {
            row.insert("status".into(), json!("refused"));
            row.insert("reason".into(), json!(error.to_string()));
        }
    }
    Value::Object(row)
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/atr72-operating-matrix.json"));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let model = Pw127m568fModel::default();

    // The sweep is the ATR 72-600's own envelope: sea-level take-off, the
    // 170 KIAS climb schedule up to FL170, and the published cruise band.
    let cases: Vec<(f64, f64, Pw127mRating)> = vec![
        (0.0, 0.0, Pw127mRating::NormalTakeoff),
        (0.0, 0.0, Pw127mRating::MaximumTakeoffReserve),
        (0.0, 70.0, Pw127mRating::NormalTakeoff),
        (0.0, 115.0, Pw127mRating::NormalTakeoff),
        (5_000.0, 170.0, Pw127mRating::MaximumClimb),
        (10_000.0, 180.0, Pw127mRating::MaximumClimb),
        (15_000.0, 190.0, Pw127mRating::MaximumClimb),
        (17_000.0, 200.0, Pw127mRating::MaximumClimb),
        (17_000.0, 275.0, Pw127mRating::MaximumCruise),
        (20_000.0, 275.0, Pw127mRating::MaximumCruise),
        (24_000.0, 270.0, Pw127mRating::MaximumCruise),
        (25_000.0, 265.0, Pw127mRating::MaximumCruise),
        (17_000.0, 275.0, Pw127mRating::MaximumContinuous),
        (10_000.0, 200.0, Pw127mRating::FlightIdleSurrogate),
    ];

    let rows: Vec<Value> = cases
        .into_iter()
        .map(|(altitude_ft, true_airspeed_kt, rating)| {
            point(model, altitude_ft, true_airspeed_kt, rating)
        })
        .collect();

    // The field-performance contract at the two densities a take-off gate
    // actually asks about: ISA sea level, and a hot-and-high case the ATR is
    // routinely dispatched from. `V_LOF` is the consumer's own number and is
    // supplied here as the published 115 kt take-off safety speed band rather
    // than derived, because this model has no lift coefficient.
    let field: Vec<Value> = [(1.225_f64, "isa_sea_level"), (1.0, "hot_and_high_proxy")]
        .into_iter()
        .map(|(density_kg_m3, label)| {
            match model.field_performance(
                density_kg_m3,
                Pw127mRating::NormalTakeoff,
                115.0 * KNOT_M_S,
            ) {
                Ok(contract) => json!({
                    "case": label,
                    "density_kg_m3": density_kg_m3,
                    "available_shaft_power_per_engine_kw":
                        contract.available_shaft_power_per_engine_w / 1_000.0,
                    "static_thrust_per_engine_n": contract.static_thrust_per_engine_n,
                    "mean_ground_roll_true_airspeed_m_s":
                        contract.mean_ground_roll_true_airspeed_m_s,
                    "mean_ground_roll_thrust_per_engine_n":
                        contract.mean_ground_roll_thrust_per_engine_n,
                    "lift_off_thrust_per_engine_n": contract.lift_off_thrust_per_engine_n,
                    "mean_ground_roll_fuel_flow_per_engine_kg_s":
                        contract.mean_ground_roll_fuel_flow_per_engine_kg_s,
                    "lift_off_propulsive_efficiency": contract.lift_off_propulsive_efficiency,
                    "residual_jet_thrust_per_engine_n":
                        contract.residual_jet_thrust_per_engine_n,
                    "static_thrust_relative_band": [
                        contract.static_thrust_uncertainty.relative_low,
                        contract.static_thrust_uncertainty.relative_high,
                    ],
                    "static_thrust_band_basis": contract.static_thrust_uncertainty.basis,
                    "forward_flight_thrust_relative_band": [
                        contract.forward_flight_thrust_uncertainty.relative_low,
                        contract.forward_flight_thrust_uncertainty.relative_high,
                    ],
                    "forward_flight_thrust_band_basis":
                        contract.forward_flight_thrust_uncertainty.basis,
                    "model_uncertainty": format!("{:?}", contract.model_uncertainty),
                }),
                Err(error) => json!({ "case": label, "error": error.to_string() }),
            }
        })
        .collect();

    let envelope = model.operating_envelope();
    let fuel = model.fuel_calibration();
    let report = json!({
        "schema_version": 3,
        "generated_by": "cargo run -p alas-prop --example atr72_operating_matrix",
        "aircraft": "ATR 72-600 (ATR 72-212A)",
        "installation": {
            "engines": 2,
            "engine": "PW127M",
            "propeller": "568F-1",
            "propeller_diameter_m": model.propeller_diameter_m,
            "governed_propeller_speed_rpm": model.governed_propeller_speed_rpm,
            "gearbox_efficiency": model.gearbox_efficiency,
            "accessory_power_kw": model.accessory_power_w / 1_000.0,
        },
        "declared_ratings_shp": {
            "normal_takeoff": model.normal_takeoff_power_w / 745.699_872,
            "maximum_takeoff_reserve": model.maximum_takeoff_reserve_power_w / 745.699_872,
            "maximum_continuous": model.maximum_continuous_power_w / 745.699_872,
            "maximum_climb": model.maximum_climb_power_w / 745.699_872,
            "maximum_cruise": model.maximum_cruise_power_w / 745.699_872,
        },
        "validation_scope": "model verification only; the propeller coefficient \
    surface is a generic six-blade surrogate and the fuel model carries one \
    aircraft-level anchor. Shaft power is never converted into a jet thrust: the \
    residual core thrust is a separate declared quantity.",
        "points": rows,
        // The typed contract the field-performance consumer reads instead of
        // `EngineConfig::thrust_kn()`, which is held at exactly zero for a
        // shaft-power engine by design and therefore gives a turboprop a
        // static thrust-to-weight ratio of zero at the take-off gate.
        "field_performance_contract": field,
        "operating_envelope": {
            "maximum_operating_altitude_m": envelope.maximum_operating_altitude_m,
            "maximum_operating_mach": envelope.maximum_operating_mach,
            "maximum_cruise_true_airspeed_m_s": envelope.maximum_cruise_true_airspeed_m_s,
            "power_lapse_floor_density_kg_m3": envelope.power_lapse_floor_density_kg_m3,
            "supported_modes": envelope
                .supported_modes
                .iter()
                .map(|mode| format!("{mode:?}"))
                .collect::<Vec<_>>(),
            "surrogate_modes": envelope
                .surrogate_modes
                .iter()
                .map(|(mode, reason)| json!({ "mode": format!("{mode:?}"), "reason": reason }))
                .collect::<Vec<_>>(),
            "unsupported_modes": envelope
                .unsupported_modes
                .iter()
                .map(|(mode, reason)| json!({ "mode": format!("{mode:?}"), "reason": reason }))
                .collect::<Vec<_>>(),
            "fuel_validity": envelope.fuel_validity,
            "source": envelope.source,
        },
        // The typed fuel statement, so a consumer reading the fuel flows above
        // can see what they rest on without reading this file.
        "fuel_calibration": {
            "anchor_total_fuel_flow_kg_h": fuel.anchor_total_fuel_flow_kg_s * 3_600.0,
            "anchor_engine_count": fuel.anchor_engine_count,
            "anchor_condition": fuel.anchor_condition,
            "anchor_altitude_is_published": fuel.anchor_altitude_is_published,
            "assumed_anchor_density_kg_m3": fuel.assumed_anchor_density_kg_m3,
            "implied_psfc_kg_kwh": fuel.implied_psfc_kg_kwh,
            "psfc_varies_with_condition": fuel.psfc_varies_with_condition,
            "measured_psfc_band_kg_kwh": [
                fuel.measured_psfc_band_kg_kwh.0,
                fuel.measured_psfc_band_kg_kwh.1,
            ],
            "relative_excess_over_measured": [
                fuel.relative_excess_over_measured.0,
                fuel.relative_excess_over_measured.1,
            ],
            "anchor_altitude_sensitivity": fuel
                .anchor_altitude_sensitivity
                .iter()
                .map(|(density_kg_m3, psfc_kg_kwh)| json!({
                    "density_kg_m3": density_kg_m3,
                    "implied_psfc_kg_kwh": psfc_kg_kwh,
                }))
                .collect::<Vec<_>>(),
            "validity": fuel.validity,
            "source": fuel.source,
        },
    });
    fs::write(&output_path, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{}", output_path.display());
    Ok(())
}
