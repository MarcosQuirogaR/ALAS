// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion selection for the shared airport and ISA field-distance method.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_perf::performance::{
    compute_field_performance_for_propulsion, FieldInputs, FieldPerformance, FieldPropulsion,
};
use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};

use crate::full_analysis::AnalysisReport;

/// Dry airport-planning share under CAT.POL.A.230(a): 60% for jets and the
/// configured propeller share (70% by default). FAR 121.195(b) generally uses
/// 60% for turbine aircraft; selecting 0.60 for the propeller share represents
/// that convention. Historical correlation replay retains its 60% reference.
pub fn dry_landing_distance_share(config: &AlasConfig) -> f64 {
    if !config.performance.legacy_field_correlations
        && matches!(
            config.geometry.engine.active_model(),
            Ok(ActiveEngineModel::Turboprop(_))
        )
    {
        config.performance.propeller_dry_landing_distance_share
    } else {
        0.60
    }
}

/// Compute airport distances from the installed propulsion type. The propeller
/// deck supplies thrust at each true airspeed and the actual airport density;
/// no sea-level roll-mean thrust or extra density lapse is applied to it.
// Aircraft masses, field, installed jet rating and clean polar are separate inputs.
#[allow(clippy::too_many_arguments)]
pub fn calculate(
    config: &AlasConfig,
    airport: &Airport,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    jet_static_tw: f64,
    cd0: f64,
    k: f64,
) -> Result<FieldPerformance, String> {
    let perf = &config.performance;
    let share = dry_landing_distance_share(config);
    if !share.is_finite() || share <= 0.0 || share > 1.0 {
        return Err("dry landing distance share must be in (0, 1]".to_owned());
    }
    let inputs = FieldInputs {
        takeoff_mass_kg,
        landing_mass_kg,
        wing_area_m2,
        airport,
        cl_max_to: perf.cl_max_to,
        cl_max_land: perf.cl_max_land,
        sea_level_static_tw: jet_static_tw,
        landing_distance_factor: perf.k_land,
        config: perf,
    };
    match config
        .geometry
        .engine
        .active_model()
        .map_err(|error| error.to_string())?
    {
        ActiveEngineModel::Turbofan(_) => {
            compute_field_performance_for_propulsion(inputs, FieldPropulsion::Jet)
        }
        ActiveEngineModel::Turboprop(spec) => {
            let engine_count = config.geometry.engine.spanwise_positions_m.len();
            let model = turboprop_unit_model(spec);
            let atmosphere =
                alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
            let evaluate_thrust = |speed_m_s, rating, operating_engines| {
                model
                    .evaluate_at_temperature(
                        TurbopropCondition {
                            density_kg_m3: atmosphere.density_kg_m3,
                            true_airspeed_m_s: speed_m_s,
                        },
                        atmosphere.temperature_k,
                        TurbopropCommand {
                            rating,
                            power_fraction: 1.0,
                            mode: TurbopropMode::Governed,
                            propeller_speed_rpm: model.governed_propeller_speed_rpm,
                        },
                    )
                    .map(|output| output.total_thrust_n * operating_engines as f64)
                    .map_err(|error| format!("propeller field thrust at {speed_m_s} m/s: {error}"))
            };
            let thrust = |speed| evaluate_thrust(speed, Pw127mRating::NormalTakeoff, engine_count);
            let engine_out_thrust = |speed| {
                evaluate_thrust(
                    speed,
                    Pw127mRating::MaximumTakeoffReserve,
                    engine_count.saturating_sub(1),
                )
            };
            let inputs = if perf.legacy_field_correlations {
                inputs
            } else {
                FieldInputs {
                    sea_level_static_tw: thrust(0.0)?
                        / (takeoff_mass_kg * config.requirements.gravity_m_s2),
                    ..inputs
                }
            };
            compute_field_performance_for_propulsion(
                inputs,
                FieldPropulsion::Propeller {
                    engine_count,
                    thrust_n: &thrust,
                    engine_out_thrust_n: Some(&engine_out_thrust),
                    zero_lift_drag_coefficient: cd0 + perf.oei_climb_delta_cd,
                    induced_drag_factor: k,
                    // No unconfigured windmilling or asymmetric-drag credit is inferred.
                    // The zero assumption is exposed in the reference method's basis.
                    failed_engine_drag_coefficient: perf.oei_windmilling_cd.unwrap_or(0.0)
                        + perf.oei_asymmetric_trim_cd.unwrap_or(0.0),
                },
            )
        }
    }
}

/// Low-speed quadratic field polar from the candidate's shared drag model.
/// A positive linear lift term is conservatively bounded over 0..CLmax_TO;
/// a negative term receives no credit. The model itself is left intact.
pub fn report_field_polar(
    config: &AlasConfig,
    report: &AnalysisReport,
    mach: f64,
    airport: &Airport,
) -> Result<(f64, f64), String> {
    let artifacts = report.fuel.artifacts(config, &report.design)?;
    let cd = |cl| {
        artifacts
            .drag
            .cd_at_atmosphere(cl, mach, airport.elevation_m, airport.isa_deviation_c)
    };
    let (c0, c_half, c_one) = (cd(0.0), cd(0.5), cd(1.0));
    let k = 2.0 * (c_one - 2.0 * c_half + c0);
    let c1 = c_one - c0 - k;
    let cd0 = c0 + c1.max(0.0) * config.performance.cl_max_to;
    if !(cd0.is_finite() && cd0 > 0.0 && k.is_finite() && k > 0.0) {
        return Err(
            "candidate field polar needs finite positive CD0 and induced factor".to_owned(),
        );
    }
    Ok((cd0, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({"preset": name}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"))
    }

    #[test]
    fn installed_propeller_distances_increase_with_weight_and_elevation() {
        let config = config("ATR72-600");
        let design = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|reason| panic!("preset: {reason}"))
            .design_vector;
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|reason| panic!("analysis: {reason}"));
        let mtow = config.requirements.mtow_kg;
        let mlw = config.landing_mass_limit_kg(mtow);
        let airport = Airport::custom("field", 0.0, 10000.0, 10000.0, 0.0, 0.0, 0.0);
        let evaluate = |weight_scale, elevation| {
            let airport = Airport {
                elevation_m: elevation,
                ..airport.clone()
            };
            let (cd0, k) = report_field_polar(&config, &report, 0.2, &airport)
                .unwrap_or_else(|reason| panic!("polar: {reason}"));
            calculate(
                &config,
                &airport,
                report.airplane.s_ref,
                mtow * weight_scale,
                mlw * weight_scale,
                0.3,
                cd0,
                k,
            )
            .unwrap_or_else(|reason| panic!("field: {reason}"))
        };
        let reference = evaluate(1.0, 0.0);
        for condition in [evaluate(1.1, 0.0), evaluate(1.0, 2000.0)] {
            assert!(condition.todr_m > reference.todr_m);
            assert!(condition.ldr_m > reference.ldr_m);
            assert!(condition.v_speeds.v_app_ms > reference.v_speeds.v_app_ms);
        }
        assert_eq!(
            reference.v_speeds.v_app_ms,
            1.23 * reference.v_speeds.v_stall_land_ms
        );
        assert_eq!(
            dry_landing_distance_share(&config),
            config.performance.propeller_dry_landing_distance_share
        );
    }

    #[test]
    fn jet_and_propeller_report_the_same_reference_speed_definition() {
        for name in ["A320-200", "ATR72-600"] {
            let mut config = config(name);
            let airport = Airport::custom("field", 0.0, 10000.0, 10000.0, 0.0, 0.0, 0.0);
            let (area, takeoff, landing) = if name == "ATR72-600" {
                (61.0, 23000.0, 21000.0)
            } else {
                (122.6, 50000.0, 45000.0)
            };
            let (cd0, k) = if name == "ATR72-600" {
                let design = alas_config::presets::get(name)
                    .unwrap_or_else(|reason| panic!("preset: {reason}"))
                    .design_vector;
                let report = crate::full_analysis::FullAnalysis::new(config.clone())
                    .run(&design, true)
                    .unwrap_or_else(|reason| panic!("analysis: {reason}"));
                report_field_polar(&config, &report, 0.2, &airport)
                    .unwrap_or_else(|reason| panic!("polar: {reason}"))
            } else {
                (0.02, 0.04)
            };
            let field = calculate(&config, &airport, area, takeoff, landing, 0.3, cd0, k)
                .unwrap_or_else(|reason| panic!("field: {reason}"));
            assert_eq!(
                field.v_speeds.v_app_ms,
                1.23 * field.v_speeds.v_stall_land_ms
            );
            config.performance.propeller_dry_landing_distance_share = f64::NAN;
            if name == "ATR72-600" {
                assert!(
                    calculate(&config, &airport, 61.0, 23000.0, 21000.0, 0.3, 0.02, 0.04).is_err()
                );
            }
        }
    }
}
