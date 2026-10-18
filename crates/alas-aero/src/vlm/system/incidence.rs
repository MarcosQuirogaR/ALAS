// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flow-tangency normal of a panel with an added incidence offset.
//!
//! [`rotated_normal`] turns the panel's unit normal about the panel's
//! spanwise axis (the bound-vortex leg direction) by the offset angle. The
//! rotation is oriented so that the normal tilts toward the aft chord
//! direction for a positive offset: for a flat panel in the xy plane
//! (geometry axes, x aft, z up) with normal `(0, 0, 1)` and freestream
//! direction `(cos a, 0, sin a)` the tangency condition becomes
//! `-V sin(a + offset)`, i.e. the panel behaves as if its incidence had been
//! raised by `offset` (nose-up positive). It enters only the right-hand side
//! of the circulation solve, never the influence matrix.

use super::Panel;
use crate::vector3::{add3, cross3, dot3, norm3, scale3, sub3};

/// `panel`'s unit normal rotated by `offset_rad` about its spanwise axis, so
/// that a positive offset raises the effective incidence.
pub(super) fn rotated_normal(panel: &Panel, offset_rad: f64) -> [f64; 3] {
    let leg_length = norm3(panel.vortex_bound_leg);
    if offset_rad == 0.0 || !leg_length.is_finite() || leg_length <= 0.0 {
        return panel.normal_direction;
    }
    let n = panel.normal_direction;
    let mut axis = scale3(panel.vortex_bound_leg, 1.0 / leg_length);
    let front = scale3(add3(panel.front_left, panel.front_right), 0.5);
    let back = scale3(add3(panel.back_left, panel.back_right), 0.5);
    let aft = sub3(back, front);
    // Orient the axis so that `axis x n` points aft whatever the panel's
    // winding: then the rotation below tilts the normal aft for a positive
    // offset, i.e. raises the incidence.
    if dot3(cross3(axis, n), aft) < 0.0 {
        axis = scale3(axis, -1.0);
    }
    let (sin, cos) = offset_rad.sin_cos();
    // Rodrigues: n cos + (k x n) sin + k (k . n)(1 - cos); k is in the panel
    // plane so k . n = 0 up to rounding and the last term is kept for
    // exactness on warped panels.
    let rotated = add3(
        add3(scale3(n, cos), scale3(cross3(axis, n), sin)),
        scale3(axis, dot3(axis, n) * (1.0 - cos)),
    );
    let length = norm3(rotated);
    scale3(rotated, 1.0 / length)
}
