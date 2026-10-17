// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion selection for the shared airport and ISA field-distance method.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_opt::mdo::{
    field_polar, jet_takeoff_field, EngineOutCondition, EngineOutModel, EngineOutRating,
    JetFieldInputs, YawAirframe,
};
use alas_perf::performance::{
    compute_field_performance_for_propulsion, compute_v_speeds_at_masses, landing_runway_share,
    FieldInputs, FieldPerformance, FieldPropulsion,
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
        alas_perf::performance::JET_DRY_LANDING_SHARE
    }
}

/// Share of the landing distance available the actual landing distance may
/// use for the configured runway condition: [`dry_landing_distance_share`],
/// divided by 1.15 on a wet runway (`wet_landing_runway`, EU Air OPS
/// CAT.POL.A.235(a)(2)). The translated replay has no wet mode.
pub fn landing_distance_share(config: &AlasConfig) -> f64 {
    let dry = dry_landing_distance_share(config);
    if config.performance.legacy_field_correlations {
        dry
    } else {
        landing_runway_share(dry, config.performance.wet_landing_runway)
    }
}

/// [`calculate_with_airframe`] without an airframe: a turboprop then needs
/// `oei_asymmetric_trim_cd` configured, because the asymmetric trim drag of the
/// failed-engine state cannot be derived without a fin (jets are unaffected).
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
    calculate_with_airframe(
        config,
        None,
        airport,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        jet_static_tw,
        cd0,
        k,
    )
}

/// Compute airport distances from the installed propulsion type. The propeller
/// deck supplies thrust at each true airspeed and the actual airport density;
/// no sea-level roll-mean thrust or extra density lapse is applied to it.
///
/// The propeller failed-engine drag is the feathered-propeller drag plus the
/// asymmetric trim drag at V2 from the shared engine-out evidence builder
/// (configured values override the derivations). A missing term is an error,
/// never a silent zero.
// Aircraft masses, field, installed jet rating and clean polar are separate inputs.
#[allow(clippy::too_many_arguments)]
pub fn calculate_with_airframe(
    config: &AlasConfig,
    airframe: Option<YawAirframe<'_>>,
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
            let mut field = compute_field_performance_for_propulsion(inputs, FieldPropulsion::Jet)?;
            if perf.legacy_field_correlations {
                return Ok(field);
            }
            // CS 25.113 field length by integration: the correlation above is
            // kept only for the speeds, the landing distance and replay.
            let static_thrust_n =
                jet_static_tw * takeoff_mass_kg * config.requirements.gravity_m_s2;
            let model = EngineOutModel::new(config, airframe, wing_area_m2, None)?
                .with_weight_kg(takeoff_mass_kg);
            let solved = jet_takeoff_field(&JetFieldInputs {
                perf,
                engine_out: &model,
                airport,
                wing_area_m2,
                mass_kg: takeoff_mass_kg,
                static_thrust_n,
                polar: (cd0, k),
                speeds: field.v_speeds,
            })?;
            field.todr_m = solved.tofl_m;
            field.bfl_m = solved.balanced_m;
            field.asd_m = solved.accelerate_stop_m;
            // The speeds the field length was flown with: the balanced V1 and
            // the selected rotation speed (VR >= the schedule minimum).
            field.v_speeds.v1_ms = solved.v1_m_s;
            field.v_speeds.v_r_ms = solved.vr_m_s;
            field.takeoff_field = Some(solved);
            Ok(field)
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
            let static_per_engine_n = thrust(0.0)? / engine_count as f64;
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
                    ground_roll_gear_drag_coefficient: perf.takeoff_gear_delta_cd,
                    induced_drag_factor: k,
                    // The translated replay ignores the term and is not asked
                    // for evidence it does not use.
                    failed_engine_drag_coefficient: if perf.legacy_field_correlations {
                        0.0
                    } else {
                        failed_engine_drag(
                            config,
                            airframe,
                            airport,
                            wing_area_m2,
                            takeoff_mass_kg,
                            static_per_engine_n,
                            cd0,
                        )?
                    },
                },
            )
        }
    }
}

/// Failed-engine drag coefficient (feathered propeller plus asymmetric trim)
/// at V2 for the propeller field estimate, from the shared evidence builder.
fn failed_engine_drag(
    config: &AlasConfig,
    airframe: Option<YawAirframe<'_>>,
    airport: &Airport,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    static_per_engine_n: f64,
    clean_cd0: f64,
) -> Result<f64, String> {
    let perf = &config.performance;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        takeoff_mass_kg,
        wing_area_m2,
        airport,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let model = EngineOutModel::new(config, airframe, wing_area_m2, Some(static_per_engine_n))?
        .with_weight_kg(takeoff_mass_kg);
    let point = model.point(
        EngineOutCondition {
            altitude_m: airport.elevation_m,
            isa_deviation_c: airport.isa_deviation_c,
            tas_m_s: speeds.v2_ms,
        },
        EngineOutRating::TakeoffGoAround,
        clean_cd0,
    )?;
    let trim = point.asymmetric_trim_cd.ok_or_else(|| {
        "propeller engine-out drag needs the asymmetric trim drag: configure \
         oei_asymmetric_trim_cd or evaluate with an airframe that has a fin"
            .to_owned()
    })?;
    Ok(point.windmilling_cd + trim)
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
    field_polar(&artifacts.drag, &config.performance, mach, airport)
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
            calculate_with_airframe(
                &config,
                Some(YawAirframe {
                    plane: &report.airplane,
                    cg_x_m: report.physical_cg[0],
                }),
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
    fn no_silent_zero_remains_for_the_failed_engine_drag() {
        // DRG-03 / PROP-12: the propeller failed-engine drag used to default
        // to zero when the windmilling or trim value was unset.
        let source = include_str!("field_performance.rs");
        for field in ["oei_windmilling_cd", "oei_asymmetric_trim_cd"] {
            let silent_zero = [field, ".unwrap_or(0.0)"].concat();
            assert!(!source.contains(&silent_zero), "{silent_zero}");
        }
    }

    #[test]
    fn a_turboprop_without_trim_evidence_is_an_error_not_a_zero() {
        // No airframe and no configured asymmetric trim drag: the propeller
        // engine-out drag cannot be formed, and no silent zero is used.
        let config = config("ATR72-600");
        assert!(config.performance.oei_asymmetric_trim_cd.is_none());
        let airport = Airport::custom("field", 0.0, 10000.0, 10000.0, 0.0, 0.0, 0.0);
        let error = calculate(&config, &airport, 61.0, 23000.0, 21000.0, 0.3, 0.02, 0.04)
            .err()
            .unwrap_or_else(|| panic!("expected an evidence error"));
        assert!(error.contains("asymmetric trim"), "{error}");
        // A configured value makes it evaluable and enters the OEI gradient.
        let mut configured = config.clone();
        configured.performance.oei_asymmetric_trim_cd = Some(0.004);
        let low = calculate(
            &configured,
            &airport,
            61.0,
            23000.0,
            21000.0,
            0.3,
            0.02,
            0.04,
        )
        .unwrap_or_else(|reason| panic!("field: {reason}"));
        configured.performance.oei_asymmetric_trim_cd = Some(0.012);
        let high = calculate(
            &configured,
            &airport,
            61.0,
            23000.0,
            21000.0,
            0.3,
            0.02,
            0.04,
        )
        .unwrap_or_else(|reason| panic!("field: {reason}"));
        // More failed-engine drag lowers the engine-out gradient: the balanced
        // field length grows.
        assert!(high.bfl_m > low.bfl_m);
    }

    #[test]
    fn extended_gear_drag_lengthens_only_the_propeller_ground_roll() {
        let mut config = config("ATR72-600");
        config.performance.oei_asymmetric_trim_cd = Some(0.004);
        let airport = Airport::custom("field", 0.0, 10000.0, 10000.0, 0.0, 0.0, 0.0);
        let run = |gear: f64| {
            let mut config = config.clone();
            config.performance.takeoff_gear_delta_cd = gear;
            calculate(&config, &airport, 61.0, 23000.0, 21000.0, 0.3, 0.02, 0.04)
                .unwrap_or_else(|reason| panic!("field: {reason}"))
        };
        let clean = run(0.0);
        let geared = run(0.05);
        assert!(geared.todr_m > clean.todr_m);
        // The landing distance does not read the take-off gear increment.
        assert_eq!(geared.ldr_m.to_bits(), clean.ldr_m.to_bits());
    }

    #[test]
    fn a_wet_runway_divides_the_planning_share_by_1_15() {
        let mut config = config("A320-200");
        let dry = landing_distance_share(&config);
        assert_eq!(dry, dry_landing_distance_share(&config));
        config.performance.wet_landing_runway = true;
        assert!((landing_distance_share(&config) - dry / 1.15).abs() < 1e-15);
        // The dry definition used for published-value comparison is unchanged.
        assert_eq!(dry_landing_distance_share(&config), dry);
        let turboprop = {
            let mut atr = self::config("ATR72-600");
            atr.performance.wet_landing_runway = true;
            atr
        };
        assert!(
            (landing_distance_share(&turboprop)
                - turboprop.performance.propeller_dry_landing_distance_share / 1.15)
                .abs()
                < 1e-15
        );
    }

    #[test]
    fn jet_and_propeller_report_the_same_reference_speed_definition() {
        for name in ["A320-200", "ATR72-600"] {
            let mut config = config(name);
            // The reference-speed definition does not depend on the engine-out
            // drag; a configured trim value stands in for the airframe.
            config.performance.oei_asymmetric_trim_cd = Some(0.003);
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
