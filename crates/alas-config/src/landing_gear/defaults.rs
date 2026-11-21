// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Default values of the landing-gear and takeoff-rotation inputs.

use super::LandingGearConfig;

pub(super) const fn default_rotation_rolling_friction_coefficient() -> f64 {
    // Conceptual-design value for a dry hard runway (engineering estimate).
    0.02
}

pub(super) const fn default_cl_ground_attitude_frac_of_cl_max_to() -> f64 {
    // Torenbeek order-of-magnitude: the ground/pre-rotation attitude is a
    // fraction of the flaps-down takeoff CLmax, not the full value.
    0.40
}

pub(super) const fn default_nlg_dynamic_braking_decel_g() -> f64 {
    // 14 CFR 25.733(b)(2): 1.0g down combined with 0.31g forward.
    0.31
}

pub(super) fn default_auto() -> String {
    "auto".to_owned()
}

pub(super) fn is_auto(value: &str) -> bool {
    value == "auto"
}

pub(super) const fn default_tire_dynamic_rating_factor() -> f64 {
    1.5
}

pub(super) const fn default_min_tip_back_deg() -> f64 {
    // Torenbeek's criterion alone: the tip-back angle must clear the
    // tail-down angle, with no flat floor on top of it.
    0.0
}

pub(super) const fn default_required_rotation_angle_deg() -> f64 {
    10.0
}

impl Default for LandingGearConfig {
    fn default() -> Self {
        Self {
            derived_main_gear: None,
            tire_safety_factor: 1.07,
            n_nlg_wheels: 0,
            nlg_dual_wheel_mtow_kg: 15_000.0,
            n_mlg_struts: 0,
            mlg_body_gear_mtow_kg: 300_000.0,
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.85,
            reference_wheelbase_m: None,
            reference_station_frame: None,
            reference_station_fuselage_length_m: None,
            reference_nlg_x_fraction: None,
            reference_mlg_x_fractions: None,
            reference_body_wheelbase_m: None,
            reference_track_m: None,
            mlg_strut_bogie_wheels: None,
            tire_class: "auto".to_owned(),
            mlg_tire: "auto".to_owned(),
            nlg_tire: "auto".to_owned(),
            strut_material: "auto".to_owned(),
            turnover_angle_limit_deg: 63.0,
            nlg_dynamic_braking_decel_g: default_nlg_dynamic_braking_decel_g(),
            tire_dynamic_rating_factor: default_tire_dynamic_rating_factor(),
            min_tip_back_deg: default_min_tip_back_deg(),
            required_rotation_angle_deg: default_required_rotation_angle_deg(),
            required_rotation_basis: super::RotationAngleBasis::Assumed,
            fuselage_ground_clearance_m: None,
            rotation_pitch_acceleration_deg_s2: None,
            pitch_radius_of_gyration_frac_mac: None,
            trimmable_stabilizer: false,
            takeoff_stabilizer_nose_up_deg: None,
            stabilizer_nose_up_travel_deg: None,
            elevator_up_travel_deg: None,
            nose_gear_max_static_load_kg: None,
            elevator_trim_tab_nose_up_deg: None,
            elevator_balance_tab_gearing: None,
            rotation_minimum_control_speed_m_s: None,
            published_vmca_m_s: None,
            cl_ground_attitude_frac_of_cl_max_to: default_cl_ground_attitude_frac_of_cl_max_to(),
            rotation_rolling_friction_coefficient: default_rotation_rolling_friction_coefficient(),
        }
    }
}

/// The designations of the certified data-book tires `mlg_tire` and
/// `nlg_tire` accept after `auto`. alas-perf owns the rated loads; a test
/// there pins this list to its `CERTIFIED_TIRES`.
pub const CERTIFIED_TIRE_CODES: [&str; 20] = [
    "H49x19.0-22/32PR",
    "49x17/32PR",
    "52x20.5-23",
    "40x15.5-16",
    "450x190-5/10PR",
    "1400x530R23/36PR",
    "1400x530R23/40PR",
    "H34x10.0R16/14PR",
    "46x17.0R20/30PR",
    "46x17.0R20",
    "54x21.0R23/38PR",
    "H42x15.0R21/26PR",
    "H42x16.0R20/24PR",
    "43x15.5-17/22PR",
    "40x16.0R16/26PR",
    "30x8.8R15/16PR",
    "1050x395R16/28PR",
    "1270x455R22/32PR",
    "27x8.5R12/16PR",
    "37x14.0-14/22PR",
];
