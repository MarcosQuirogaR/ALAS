// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Payload-range corners are reserve-inclusive, the report chart and the
//! sandbox Quick Analysis publish the same corners, and the design-mission
//! band check follows the range at each band end.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, DesignMode, FuelScheme};
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchStatus};
use alas_pipeline::quick_analysis::band::{classify_band, design_mission_band_check, BandStatus};
use alas_pipeline::quick_analysis::corners::RangeBasis;
use alas_pipeline::quick_analysis::{payload_range_corners, report_oew_kg};
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};
use alas_report::families::performance;

use std::sync::OnceLock;

const NMI: f64 = 1852.0;

/// The A320 baseline run, shared by the tests that need one.
fn a320() -> &'static PipelineResult {
    static RESULT: OnceLock<PipelineResult> = OnceLock::new();
    RESULT.get_or_init(|| run_baseline("A320-200"))
}

fn run_baseline(name: &str) -> PipelineResult {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap()
}

#[test]
fn the_chart_and_quick_analysis_agree_and_reserves_shorten_the_range() {
    let result = a320();
    let report = result.baseline_analysis.as_ref().unwrap();
    let config = &result.config;

    let chart = performance::payload_range_data(report, config).unwrap();
    let quick = payload_range_corners(config, report).unwrap();
    assert_eq!(chart.range_basis, RangeBasis::ReservesIncluded);
    assert_eq!(quick.range_basis, RangeBasis::ReservesIncluded);
    assert!(chart
        .method_note
        .contains(config.fuel_policy.scheme.as_str()));
    for (index, (range_m, payload_kg)) in quick.points.iter().enumerate() {
        let point = chart.points[index];
        assert!(
            (point.payload_kg - payload_kg).abs() < 1.0,
            "corner {index} payload"
        );
        assert!(
            (point.range_nm * NMI - range_m).abs() < 1.0,
            "corner {index}: chart {} nmi vs quick {} m",
            point.range_nm,
            range_m
        );
        assert_eq!(chart.reserve_fuel_kg[index], quick.reserve_fuel_kg[index]);
    }
    assert!(chart.reserve_fuel_kg[1] > 0.0 && chart.reserve_fuel_kg[3] > 0.0);
    assert!(chart.points[1].range_nm > 0.0);
    assert!(chart.points[1].range_nm <= chart.points[2].range_nm);
    assert!(chart.points[2].range_nm <= chart.points[3].range_nm);

    let mut trip_only = config.clone();
    trip_only.fuel_policy.scheme = FuelScheme::TripFuelOnly;
    let bare = performance::payload_range_data(report, &trip_only).unwrap();
    for index in 1..4 {
        assert!(
            chart.points[index].range_nm < bare.points[index].range_nm,
            "corner {index}: reserves must shorten the range"
        );
    }
}

/// Along a line of constant payload the takeoff mass the fuel policy
/// requires grows with range: every further metre burns fuel, and the fuel
/// carried for it is itself carried, so dTOW/dR > 0 on the report's model.
#[test]
fn the_required_takeoff_mass_grows_with_range() {
    let result = a320();
    let report = result.baseline_analysis.as_ref().unwrap();
    let config = &result.config;
    let model = alas_pipeline::fuel_model::report_mission_model(config, report).unwrap();
    let zero_fuel_mass_kg = report_oew_kg(report) + 15_000.0;
    let limits = DispatchLimits {
        mtow_kg: 2.0 * config.requirements.mtow_kg,
        mzfw_kg: None,
        mlw_kg: None,
        usable_capacity_kg: None,
    };
    let mut previous_kg = zero_fuel_mass_kg;
    for range_nmi in [300.0, 800.0, 1_300.0, 1_800.0, 2_300.0, 2_800.0] {
        let solution = solve_dispatch(
            zero_fuel_mass_kg,
            range_nmi * NMI,
            &config.fuel_policy,
            &model,
            &limits,
            50,
            0.01,
        );
        assert_eq!(
            solution.status,
            DispatchStatus::Converged,
            "{range_nmi} nmi"
        );
        assert!(
            solution.takeoff_mass_kg > previous_kg,
            "{range_nmi} nmi: {} kg after {previous_kg} kg",
            solution.takeoff_mass_kg
        );
        previous_kg = solution.takeoff_mass_kg;
    }
}

#[test]
fn the_band_check_follows_the_range_at_each_band_end() {
    assert_eq!(classify_band(1.0e6, 2.0e6, 1.5e6), BandStatus::Inside);
    assert_eq!(classify_band(2.0e6, 3.0e6, 1.5e6), BandStatus::BandTooHeavy);
    assert_eq!(classify_band(1.0e6, 1.4e6, 1.5e6), BandStatus::BandTooLight);

    let result = a320();
    let report = result.baseline_analysis.as_ref().unwrap();
    let config = &result.config;
    let mtow_kg = config.requirements.mtow_kg;
    let payload_kg = 15_000.0;
    let band = |range_nmi: f64, lo: f64, hi: f64| {
        design_mission_band_check(
            config,
            report,
            payload_kg,
            range_nmi * NMI,
            lo * mtow_kg,
            hi * mtow_kg,
        )
    };

    let inside = band(1_500.0, 0.85, 1.0);
    assert_eq!(inside.status, BandStatus::Inside, "{inside:?}");
    assert!(inside.range_at_lo_m < inside.range_at_hi_m);
    assert!(inside.note.contains("quick estimate"));
    assert_eq!(band(1_500.0, 0.55, 0.6).status, BandStatus::BandTooLight);
    let heavy = band(200.0, 0.85, 1.0);
    assert_eq!(heavy.status, BandStatus::BandTooHeavy, "{heavy:?}");
    assert_eq!(band(1_500.0, 1.0, 0.9).status, BandStatus::Unavailable);
}
