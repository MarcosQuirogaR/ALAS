// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical invariants of the propulsion-specific conceptual field methods.

use alas_config::airports::Airport;
use alas_config::PerformanceConfig;
use alas_perf::performance::{
    compute_field_performance_at_masses, compute_field_performance_for_propulsion,
    compute_propeller_landing_distance, density_ratio, FieldInputs, FieldPerformance,
    FieldPropulsion, VREF_OVER_VS1G,
};

fn airport(elevation_m: f64) -> Airport {
    Airport::custom(
        "Physics fixture",
        elevation_m,
        5_000.0,
        5_000.0,
        0.0,
        0.0,
        0.0,
    )
}

fn inputs<'a>(config: &'a PerformanceConfig, field: &'a Airport) -> FieldInputs<'a> {
    FieldInputs {
        takeoff_mass_kg: 23_000.0,
        landing_mass_kg: 21_000.0,
        wing_area_m2: 61.0,
        airport: field,
        cl_max_to: 2.1,
        cl_max_land: 2.6,
        sea_level_static_tw: 80_000.0 / (23_000.0 * 9.81),
        landing_distance_factor: config.k_land,
        config,
    }
}

fn propeller(
    inputs: FieldInputs<'_>,
    thrust: &dyn Fn(f64) -> Result<f64, String>,
) -> Result<FieldPerformance, String> {
    compute_field_performance_for_propulsion(
        inputs,
        FieldPropulsion::Propeller {
            engine_count: 2,
            thrust_n: thrust,
            engine_out_thrust_n: None,
            zero_lift_drag_coefficient: 0.04,
            induced_drag_factor: 0.04,
            failed_engine_drag_coefficient: 0.0,
        },
    )
}

#[test]
fn reference_speed_is_the_modern_one_g_stall_multiple() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let input = inputs(&config, &field);
    let thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    for result in [
        compute_field_performance_for_propulsion(input, FieldPropulsion::Jet)?,
        propeller(input, &thrust)?,
    ] {
        assert_eq!(
            result.v_speeds.v_app_ms,
            VREF_OVER_VS1G * result.v_speeds.v_stall_land_ms
        );
    }
    Ok(())
}

#[test]
fn greater_mass_lengthens_both_propeller_field_distances() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    let light = inputs(&config, &field);
    let heavy = FieldInputs {
        takeoff_mass_kg: 25_000.0,
        landing_mass_kg: 23_000.0,
        sea_level_static_tw: 80_000.0 / (25_000.0 * 9.81),
        ..light
    };
    let light_result = propeller(light, &thrust)?;
    let heavy_result = propeller(heavy, &thrust)?;
    assert!(heavy_result.todr_m > light_result.todr_m);
    assert!(heavy_result.bfl_m > light_result.bfl_m);
    assert!(heavy_result.ldr_m > light_result.ldr_m);
    assert!(heavy_result.v_speeds.v_app_ms > light_result.v_speeds.v_app_ms);
    Ok(())
}

#[test]
fn altitude_and_power_lapse_lengthen_propeller_distances() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let sea_level = airport(0.0);
    let high_field = airport(1_500.0);
    // A continuous speed-dependent test deck isolates the direction of density
    // and power lapse; it is not used as an aircraft performance calibration.
    let reference_thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    let sigma = density_ratio(high_field.elevation_m, 0.0);
    let altitude_thrust = |speed| Ok(80_000.0 * sigma / (1.0 + speed / 100.0));
    let reference = propeller(inputs(&config, &sea_level), &reference_thrust)?;
    let altitude = propeller(inputs(&config, &high_field), &altitude_thrust)?;
    assert!(altitude.todr_m > reference.todr_m);
    assert!(altitude.bfl_m > reference.bfl_m);
    assert!(altitude.ldr_m > reference.ldr_m);
    Ok(())
}

#[test]
fn speed_lapse_cannot_shorten_takeoff_at_equal_static_thrust() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let constant_thrust = |_speed| Ok(80_000.0);
    let lapse_thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    let constant = propeller(inputs(&config, &field), &constant_thrust)?;
    let lapsing = propeller(inputs(&config, &field), &lapse_thrust)?;
    assert!(lapsing.todr_m > constant.todr_m);
    assert!(lapsing.bfl_m > constant.bfl_m);
    assert_eq!(lapsing.ldr_m, constant.ldr_m);
    Ok(())
}

#[test]
fn jets_use_top_once_and_the_legacy_path_remains_selectable() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let input = inputs(&config, &field);
    let translated = compute_field_performance_at_masses(
        input.takeoff_mass_kg,
        input.landing_mass_kg,
        input.wing_area_m2,
        input.airport,
        input.cl_max_to,
        input.cl_max_land,
        input.sea_level_static_tw,
        input.landing_distance_factor,
        config.bfl_factor,
        &config,
    );
    let corrected = compute_field_performance_for_propulsion(input, FieldPropulsion::Jet)?;
    assert_eq!(corrected.todr_m, translated.todr_m);
    assert_eq!(corrected.bfl_m, corrected.todr_m);
    assert_eq!(translated.bfl_m, corrected.bfl_m * config.bfl_factor);
    assert_eq!(corrected.ldr_m, translated.ldr_m);
    let legacy_config = PerformanceConfig {
        legacy_field_correlations: true,
        ..config.clone()
    };
    let legacy = compute_field_performance_for_propulsion(
        FieldInputs {
            config: &legacy_config,
            ..input
        },
        FieldPropulsion::Jet,
    )?;
    assert_eq!(legacy, translated);
    Ok(())
}

#[test]
fn jets_also_lengthen_with_mass_and_altitude() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let reference_airport = airport(0.0);
    let high_airport = airport(1_500.0);
    let reference_input = inputs(&config, &reference_airport);
    let reference =
        compute_field_performance_for_propulsion(reference_input, FieldPropulsion::Jet)?;
    let heavy = compute_field_performance_for_propulsion(
        FieldInputs {
            takeoff_mass_kg: 25_000.0,
            landing_mass_kg: 23_000.0,
            sea_level_static_tw: 80_000.0 / (25_000.0 * 9.81),
            ..reference_input
        },
        FieldPropulsion::Jet,
    )?;
    let high = compute_field_performance_for_propulsion(
        FieldInputs {
            airport: &high_airport,
            ..reference_input
        },
        FieldPropulsion::Jet,
    )?;
    assert!(heavy.todr_m > reference.todr_m && heavy.ldr_m > reference.ldr_m);
    assert!(high.todr_m > reference.todr_m && high.ldr_m > reference.ldr_m);
    Ok(())
}

#[test]
fn failed_engine_drag_and_weaker_brakes_cannot_improve_balanced_field() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    let input = inputs(&config, &field);
    let reference = propeller(input, &thrust)?;
    let failed_drag = compute_field_performance_for_propulsion(
        input,
        FieldPropulsion::Propeller {
            engine_count: 2,
            thrust_n: &thrust,
            engine_out_thrust_n: None,
            zero_lift_drag_coefficient: 0.04,
            induced_drag_factor: 0.04,
            failed_engine_drag_coefficient: 0.02,
        },
    )?;
    let weak_config = PerformanceConfig {
        propeller_takeoff_stop_deceleration_g: 0.25,
        ..config.clone()
    };
    let weaker_brakes = propeller(
        FieldInputs {
            config: &weak_config,
            ..input
        },
        &thrust,
    )?;
    assert!(failed_drag.bfl_m > reference.bfl_m);
    assert!(weaker_brakes.bfl_m > reference.bfl_m);
    Ok(())
}

#[test]
fn unusable_engine_out_or_invalid_inputs_return_evidence_gaps() {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let input = inputs(&config, &field);
    let insufficient_thrust = |_speed| Ok(25_000.0);
    assert!(propeller(input, &insufficient_thrust).is_err());
    assert!(compute_field_performance_for_propulsion(
        FieldInputs {
            cl_max_land: f64::NAN,
            ..input
        },
        FieldPropulsion::Jet,
    )
    .is_err());
}

#[test]
fn greater_engine_out_reserve_thrust_cannot_lengthen_balanced_field() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let thrust = |speed| Ok(80_000.0 / (1.0 + speed / 100.0));
    let reserve_thrust = |speed| Ok(44_000.0 / (1.0 + speed / 100.0));
    let input = inputs(&config, &field);
    let same_rating = propeller(input, &thrust)?;
    let reserve = compute_field_performance_for_propulsion(
        input,
        FieldPropulsion::Propeller {
            engine_count: 2,
            thrust_n: &thrust,
            engine_out_thrust_n: Some(&reserve_thrust),
            zero_lift_drag_coefficient: 0.04,
            induced_drag_factor: 0.04,
            failed_engine_drag_coefficient: 0.0,
        },
    )?;
    assert!(reserve.bfl_m < same_rating.bfl_m);
    assert!(reserve.todr_m <= same_rating.todr_m);
    assert_eq!(reserve.ldr_m, same_rating.ldr_m);
    Ok(())
}

#[test]
fn landing_remains_available_outside_the_balanced_field_domain() -> Result<(), String> {
    let config = PerformanceConfig::default();
    let field = airport(0.0);
    let input = inputs(&config, &field);
    let thrust = |_speed| Ok(80_000.0);
    let single_engine = compute_field_performance_for_propulsion(
        input,
        FieldPropulsion::Propeller {
            engine_count: 1,
            thrust_n: &thrust,
            engine_out_thrust_n: None,
            zero_lift_drag_coefficient: 0.04,
            induced_drag_factor: 0.04,
            failed_engine_drag_coefficient: 0.0,
        },
    );
    assert!(single_engine.is_err());
    assert!(compute_propeller_landing_distance(input)?.is_finite());
    Ok(())
}

#[test]
fn modern_methods_reject_nonphysical_reference_speed_schedules() {
    let field = airport(0.0);
    let thrust = |_speed| Ok(80_000.0);
    let invalid_configs = [
        PerformanceConfig {
            vmc_vstall_factor: f64::NAN,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            vmc_vstall_factor: 0.0,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            vtd_vstall_land_factor: f64::INFINITY,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            vtd_vstall_land_factor: 1.0,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            vtd_vstall_land_factor: 1.24,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            v1_vr_factor: 1.1,
            ..PerformanceConfig::default()
        },
        PerformanceConfig {
            vr_vmc_factor: 0.8,
            vr_vstall_factor: 0.9,
            ..PerformanceConfig::default()
        },
    ];
    for config in invalid_configs {
        let input = inputs(&config, &field);
        assert!(compute_field_performance_for_propulsion(input, FieldPropulsion::Jet).is_err());
        assert!(propeller(input, &thrust).is_err());
        assert!(compute_propeller_landing_distance(input).is_err());
    }
}

#[test]
fn legacy_replay_retains_the_unvalidated_speed_schedule() -> Result<(), String> {
    let field = airport(0.0);
    let config = PerformanceConfig {
        legacy_field_correlations: true,
        vmc_vstall_factor: f64::NAN,
        vtd_vstall_land_factor: f64::INFINITY,
        ..PerformanceConfig::default()
    };
    let legacy =
        compute_field_performance_for_propulsion(inputs(&config, &field), FieldPropulsion::Jet)?;
    assert!(legacy.v_speeds.v_mc_ms.is_nan());
    assert!(legacy.v_speeds.v_td_ms.is_infinite());
    Ok(())
}
