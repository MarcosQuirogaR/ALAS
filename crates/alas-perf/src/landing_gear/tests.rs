// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn a_wider_track_lowers_the_turnover_angle() {
    let narrow = LandingGearConfig {
        track_diameter_factor: 1.0,
        ..Default::default()
    };
    let wide = LandingGearConfig {
        track_diameter_factor: 3.0,
        ..Default::default()
    };

    let common = |cfg: &LandingGearConfig| {
        size_landing_gear(120_000.0, 6.0, 12.0, 7.0, 11.0, 6.0, 6.0, cfg).turnover_angle_deg
    };
    assert!(common(&wide) < common(&narrow));
}

#[test]
fn a_three_leg_arrangement_keeps_the_centreline_bogie_and_each_count() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
        track_diameter_factor: 10.684 / 5.64,
        reference_track_m: Some(10.684),
        reference_wheelbase_m: Some(25.375),
        ..Default::default()
    };
    let layout = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);

    assert_eq!(layout.n_mlg_struts, 3);
    assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 2]);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.group == "MLG")
            .count(),
        10
    );
    assert_eq!(layout.track_width_m, 10.684);
    // The source wheelbase is metadata. Model-derived stations and the
    // reaction geometry continue to use the caller's x stations.
    assert_eq!(layout.reference_wheelbase_m, Some(25.375));
    assert_eq!(layout.wheelbase_m, 25.0);
    let centreline: Vec<&Wheel> = layout
        .wheels
        .iter()
        .filter(|wheel| wheel.strut_label == "MLG-Body-C")
        .collect();
    assert_eq!(centreline.len(), 2);
    assert!((centreline[0].y + centreline[1].y).abs() < 1e-12);
    assert!(centreline
        .iter()
        .all(|wheel| (wheel.x - 31.0).abs() < 1e-12));
}

#[test]
fn four_leg_source_topology_uses_wing_and_body_bogie_sizes() {
    let config = LandingGearConfig {
        n_mlg_struts: 4,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
        track_diameter_factor: 14.34 / 7.14,
        reference_track_m: Some(14.34),
        reference_wheelbase_m: Some(28.61),
        reference_body_wheelbase_m: Some(31.88),
        ..Default::default()
    };
    let layout = size_landing_gear(560_000.0, 6.0, 35.0, 8.0, 31.0, 7.14, 8.0, &config);
    assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 6, 6]);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.group == "MLG")
            .count(),
        20
    );
    assert_eq!(layout.track_width_m, 14.34);
    assert_eq!(layout.reference_wheelbase_m, Some(28.61));
    assert_eq!(layout.reference_body_wheelbase_m, Some(31.88));
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-L")
            .count(),
        6
    );
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-R")
            .count(),
        6
    );
    // Declared (source-backed) bogie counts: the strength check is a
    // meaningful adequacy check on this layout.
    assert!(layout.capacity_basis_declared);
}

#[test]
fn reference_track_scales_with_active_fuselage_diameter() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
        track_diameter_factor: 10.684 / 5.64,
        reference_track_m: Some(10.684),
        ..Default::default()
    };

    let baseline = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);
    let resized = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 6.20, 6.0, &config);

    assert!((baseline.track_width_m - 10.684).abs() < 1e-12);
    assert!((resized.track_width_m - 6.20 * (10.684 / 5.64)).abs() < 1e-12);
    assert_eq!(resized.reference_track_m, Some(10.684));
    assert!(resized.track_width_m > baseline.track_width_m);
}

#[test]
fn malformed_bogie_list_falls_back_without_dropping_a_leg() {
    let config = LandingGearConfig {
        n_mlg_struts: 3,
        mlg_strut_bogie_wheels: Some(vec![4, 4]),
        ..Default::default()
    };
    let layout = size_landing_gear(120_000.0, 6.0, 25.0, 8.0, 22.0, 5.0, 5.0, &config);
    assert_eq!(layout.mlg_wheels_per_strut.len(), 3);
    assert_eq!(
        layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-C")
            .count(),
        layout.mlg_wheels_per_strut[2] as usize
    );
}

#[test]
fn group_station_sizing_keeps_each_main_gear_axle_in_its_source_position() {
    let config = LandingGearConfig {
        n_mlg_struts: 4,
        mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
        track_diameter_factor: 14.34 / 7.14,
        reference_track_m: Some(14.34),
        ..Default::default()
    };
    let main_gear_x_m = [33.58, 33.58, 36.85, 36.85];
    let layout = size_landing_gear_with_group_stations(
        560_000.0,
        4.97,
        33.58,
        20.0,
        30.0,
        7.14,
        8.0,
        &main_gear_x_m,
        &config,
    );
    assert_eq!(layout.main_gear_x_m, main_gear_x_m);
    assert!((layout.x_mlg_aft_axle_m - 36.85).abs() < 1e-12);
    let body_wheels: Vec<&Wheel> = layout
        .wheels
        .iter()
        .filter(|wheel| wheel.strut_label.starts_with("MLG-Body"))
        .collect();
    let body_wheel_centroid =
        body_wheels.iter().map(|wheel| wheel.x).sum::<f64>() / body_wheels.len() as f64;
    assert!((body_wheel_centroid - 36.85).abs() < 1.0e-12);
    assert!((layout.wheelbase_m - (33.58 - 4.97)).abs() < 1.0e-12);
    assert!((layout.effective_x_mlg_m - 35.542).abs() < 1.0e-12);
    assert!((layout.effective_wheelbase_m - (35.542 - 4.97)).abs() < 1.0e-12);
}

#[test]
fn the_dynamic_nose_reaction_is_reported_and_exceeds_the_static_one() {
    let config = LandingGearConfig::default();
    let layout = size_landing_gear(80_000.0, 5.0, 18.0, 8.0, 16.0, 4.0, 3.6, &config);
    assert!(layout.r_nlg_dynamic_kg > 0.0);
    assert!(layout.r_nlg_dynamic_kg > layout.r_nlg_design_kg);
}

#[test]
fn a_registered_maximum_ramp_weight_raises_the_main_gear_design_load() {
    let config = LandingGearConfig::default();
    let at_mtow = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    let at_mrw = size_landing_gear_at_design_state(
        80_000.0,
        Some(82_000.0),
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    assert!(at_mrw.r_mlg_total_design_kg > at_mtow.r_mlg_total_design_kg);
}

#[test]
fn a_most_aft_loading_state_can_widen_the_main_gear_design_envelope() {
    let config = LandingGearConfig::default();
    let aero_limit_only = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        None,
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    let with_aft_state = size_landing_gear_at_design_state(
        80_000.0,
        None,
        5.0,
        18.0,
        8.0,
        16.0,
        Some(17.5),
        4.0,
        3.6,
        &[18.0],
        &config,
    );
    assert!(with_aft_state.r_mlg_total_design_kg >= aero_limit_only.r_mlg_total_design_kg);
}

#[test]
fn an_undersized_forced_wheel_count_is_flagged_overloaded_not_hidden() {
    let config = LandingGearConfig {
        n_nlg_wheels: 1,
        wheels_per_mlg_strut: 2,
        tire_class: "light".to_owned(),
        ..Default::default()
    };
    // A heavy aircraft forced onto a single light nose wheel and 2 light
    // main tires per strut: massively under-rated on both ends.
    let layout = size_landing_gear(300_000.0, 6.0, 35.0, 10.0, 30.0, 6.0, 5.0, &config);
    assert!(layout.tire_overloaded);
    assert!(layout.nlg_tire_margin < 1.0);
    assert!(layout.mlg_tire_margin < 1.0);
}

#[test]
fn tip_back_angle_and_floor_are_reported_on_the_layout() {
    let config = LandingGearConfig::default();
    let layout = size_landing_gear(80_000.0, 5.0, 18.0, 8.0, 16.0, 4.0, 3.6, &config);
    // atan((18 - 16)/3.6) = 29.05 deg > default 15 deg floor.
    assert!((layout.tip_back_angle_deg - 29.054_604).abs() < 1e-3);
    assert!(layout.tip_back_ok);
}
