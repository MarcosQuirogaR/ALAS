// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on registered presets, so a failed unwrap is the assertion
// failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! A payload-range corner is not the declared sector.
//!
//! The report's mission model carries the preset's declared operational
//! cruise level (`alas_mission::route_cruise_altitude_m`): FL280 for the
//! A320-200's 546 km sector, FL250 for the A220-300's 251 nmi sector. Under
//! `OptimumStep` that level is the ceiling the cruise steps up to. A corner
//! several times longer than the sector is flown to the aircraft's design
//! cruise altitude instead ([`alas_opt::SegmentMissionModel::for_range_search`]);
//! a preset whose declared level is not below its design altitude is
//! unchanged to the bit.

use alas_config::AlasConfig;
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use alas_opt::mdo::PlannedTrips;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::quick_analysis::corners::CORNER_TOLERANCE_M;
use alas_pipeline::quick_analysis::payload_range_corners;
use alas_pipeline::FullAnalysis;

/// The tank-limited corner C of `preset` as the product prices it, and the
/// range of the same load flown at the route's declared level, m.
fn corner_c_ranges_m(preset: &str) -> (f64, f64, f64, f64) {
    let design = alas_config::presets::get(preset).unwrap().design_vector;
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let corners = payload_range_corners(&config, &report).unwrap();
    let model = report_mission_model(&config, &report).unwrap();
    let taxi_kg = model.taxi_fuel_flow_kg_s().unwrap() * config.fuel_policy.taxi_time_min * 60.0;
    let fuel_kg = corners.fuel_capacity_kg - taxi_kg;
    let takeoff_kg = corners.oew_kg + corners.points[2].1 + fuel_kg;
    let declared = max_range_with_reserves(
        takeoff_kg,
        fuel_kg,
        &config.fuel_policy,
        &PlannedTrips(&model),
        CORNER_TOLERANCE_M,
    );
    assert!(
        matches!(declared.status, RangeStatus::Solved),
        "{preset}: {:?}",
        declared.status
    );
    (
        corners.points[2].0,
        declared.range_m,
        model.cruise_altitude_m,
        model.design_cruise_altitude_m,
    )
}

#[test]
fn a_corner_longer_than_the_declared_sector_is_not_capped_at_the_sectors_level() {
    let (product_m, declared_level_m, declared_altitude_m, design_altitude_m) =
        corner_c_ranges_m("A320-200");
    assert!(
        declared_altitude_m < design_altitude_m,
        "the A320-200 declares {declared_altitude_m} m against design {design_altitude_m} m"
    );
    assert!(
        product_m > 1.02 * declared_level_m,
        "corner C {product_m:.0} m against {declared_level_m:.0} m flown at the declared level"
    );
}

#[test]
fn a_preset_declaring_its_design_level_is_priced_exactly_as_before() {
    let (product_m, declared_level_m, declared_altitude_m, design_altitude_m) =
        corner_c_ranges_m("A380-800");
    assert!(declared_altitude_m >= design_altitude_m);
    assert_eq!(product_m.to_bits(), declared_level_m.to_bits());
}
