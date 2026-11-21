// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Serde default values of the performance configuration.

pub(super) fn default_takeoff_lift_source() -> String {
    "Raymer, Aircraft Design: A Conceptual Approach, 5th ed., 2012, Sec. 5.4 and Ch. 17; retained conceptual high-lift CLmax_TO input, not a manufacturer measurement or a recovered certified stall limit".to_owned()
}

pub(super) fn default_landing_lift_source() -> String {
    "Mattingly et al., Aircraft Engine Design, 2nd ed., Table 2.1, p. 36 (credits Torenbeek 1976): Fowler landing CLmax/cos(quarter-chord sweep) 2.5-2.9; CLmax 2.60 is a retained engineering selection at nominal 25 deg (normalized 2.87), a conceptual class default rather than an aircraft coefficient; https://doczz.net/doc/8595015/2-constraint-analysis".to_owned()
}

pub(super) fn default_takeoff_rolling_friction() -> f64 {
    0.02
}
pub(super) fn default_takeoff_stop_deceleration() -> f64 {
    0.37
}
pub(super) fn default_takeoff_inertia_distance() -> f64 {
    200.0
}
pub(super) fn default_landing_mean_drag_to_weight() -> f64 {
    0.10
}
pub(super) fn default_landing_deceleration() -> f64 {
    0.40
}
pub(super) fn default_propeller_landing_distance_share() -> f64 {
    0.70
}
pub(super) fn default_takeoff_gear_delta_cd() -> f64 {
    0.020
}
pub(super) fn default_landing_flap_delta_cd() -> f64 {
    0.040
}
pub(super) fn default_landing_gear_delta_cd() -> f64 {
    0.020
}
pub(super) fn default_go_around_spool_thrust_fraction() -> f64 {
    1.0
}
pub(super) fn default_v2_min_vsr_factor() -> f64 {
    1.13
}
pub(super) fn default_engine_match_tolerance() -> f64 {
    0.15
}
