// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::profile_values::{
    active_cruise_count, active_descent_count, set_active_cruise_count, set_active_descent_count,
};
use alas_config::MissionProfileConfig;

fn profile_value() -> serde_json::Value {
    serde_json::to_value(MissionProfileConfig::default()).unwrap_or_default()
}

#[test]
fn active_phase_controls_preserve_a_complete_cruise_share() {
    let mut profile = profile_value();
    set_active_cruise_count(&mut profile, 2);
    assert_eq!(active_cruise_count(&profile), 2);
    let total = profile["cruise_1_distance_fraction"]
        .as_f64()
        .unwrap_or_default()
        + profile["cruise_2_distance_fraction"]
            .as_f64()
            .unwrap_or_default();
    assert!((total - 1.0).abs() < 1.0e-12);
    assert_eq!(profile["cruise_3_distance_fraction"], 0.0);
}

#[test]
fn descent_phase_controls_remove_unused_rungs_from_the_profile() {
    let mut profile = profile_value();
    set_active_descent_count(&mut profile, 2);
    assert_eq!(active_descent_count(&profile), 2);
    assert_eq!(profile["descent_3_altitude_ft"], 0.0);
    assert_eq!(profile["descent_4_altitude_ft"], 0.0);
}
