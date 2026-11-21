// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the unknowns of a segment's solve start.

/// The throttle every segment's unknowns start at, `ones_row(1) * 0.5`.
pub(super) const INITIAL_THROTTLE: f64 = 0.5;

/// The body angle a climb or descent segment's unknowns start at, radians:
/// `ones_row(1) * 3.0 * Units.degrees`.
pub(super) const INITIAL_CLIMB_BODY_ANGLE_RAD: f64 = 3.0 * std::f64::consts::PI / 180.0;

/// The body angle a cruise segment's unknowns start at, radians:
/// `ones_row(1) * 1.0 * Units.deg`. A different number from the climb's, and
/// the starting point is what MINPACK's whole path is a function of.
pub(super) const INITIAL_CRUISE_BODY_ANGLE_RAD: f64 = std::f64::consts::PI / 180.0;
