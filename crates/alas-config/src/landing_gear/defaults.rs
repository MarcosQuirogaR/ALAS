// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Default values of the landing-gear and takeoff-rotation inputs, and the
//! takeoff setting of a trimmable horizontal stabiliser.

/// Takeoff nose-up setting of a transport's trimmable horizontal stabiliser,
/// deg, for a registered preset that has one.
///
/// Source: Airbus, *Safety First* 7, "Incorrect pitch trim setting at
/// takeoff" (A330 event, 2008): a takeoff CG of 26.3 %MAC calls for 4.3 deg
/// nose-up and 26.7 %MAC for 4.1 deg, so the setting grows by about
/// 0.5 deg per %MAC as the CG moves forward. Every forward-limit CG lies
/// ahead of 26.3 %MAC, so 4.3 deg is a sourced lower bound on the setting a
/// crew selects there: a conservative class value [E], not any aircraft's
/// green-band edge, which no accessible source states. The full travel is
/// larger (A320 FCOM: 13.5 deg nose-up).
pub const TRANSPORT_THS_TAKEOFF_NOSE_UP_DEG: f64 = 4.3;

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
