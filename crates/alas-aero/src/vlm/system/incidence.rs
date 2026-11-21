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

/// How the flow-tangency normals differ from the meshed ones in one solve.
///
/// The influence matrix and its factorization depend on the meshed
/// geometry alone; only the right-hand side reads the normals.
#[derive(Clone, Copy)]
pub(super) enum NormalChange<'a> {
    /// The meshed normals.
    None,
    /// Per-panel incidence offsets about each panel's bound-vortex leg, see
    /// [`rotated_normal`].
    Offsets(&'a [f64]),
    /// The listed surfaces rotated rigidly about the geometry y axis by the
    /// angle in radians, positive nose-up (an aft point moves down).
    SurfacePitch(&'a [usize], f64),
}

impl NormalChange<'_> {
    pub(super) fn normal(&self, panel: &Panel, index: usize) -> [f64; 3] {
        match *self {
            Self::None => panel.normal_direction,
            Self::Offsets(offsets) => rotated_normal(panel, offsets[index]),
            Self::SurfacePitch(surfaces, angle_rad) => {
                if surfaces.contains(&panel.wing_index) {
                    pitched_normal(panel.normal_direction, angle_rad)
                } else {
                    panel.normal_direction
                }
            }
        }
    }
}

/// `normal` rotated about the geometry y axis by `angle_rad` (right-handed,
/// x aft and z up, so a positive angle is nose-up: an aft point moves down).
/// This is the change of a flat panel's normal when its whole surface is
/// twisted about the pitch axis by `angle_rad`.
fn pitched_normal(normal: [f64; 3], angle_rad: f64) -> [f64; 3] {
    let (sin, cos) = angle_rad.sin_cos();
    [
        normal[0] * cos + normal[2] * sin,
        normal[1],
        -normal[0] * sin + normal[2] * cos,
    ]
}

impl<'a> super::VlmSystem<'a> {
    /// Solve at `op_point` with the surfaces `wing_indices` (indices into
    /// `Airplane::wings`) pitched rigidly by `pitch_rad` about the geometry y
    /// axis as far as the flow-tangency condition is concerned: the
    /// small-angle boundary-condition change of Katz and Plotkin,
    /// *Low-Speed Aerodynamics*, 2nd ed., ch. 12. The mesh, the influence
    /// matrix and its factorization are those of the unrotated airplane, so
    /// this costs one substitution, not an assembly.
    ///
    /// It is the linear-theory image of rebuilding the airplane with those
    /// surfaces twisted about the pitch axis, exact to first order in
    /// `pitch_rad`; the second-order geometry change (the surface's own
    /// vortex positions and its position relative to the wake) is not
    /// represented. Zero pitch reproduces [`Self::solve`] exactly.
    ///
    /// # Errors
    ///
    /// [`VlmError::NonFiniteResult`] for a non-finite angle, and as
    /// [`Self::solve`].
    pub fn solve_with_surface_pitch(
        &self,
        op_point: &crate::operating_point::OperatingPoint,
        wing_indices: &[usize],
        pitch_rad: f64,
    ) -> Result<super::VlmResult, super::VlmError> {
        if !pitch_rad.is_finite() {
            return Err(super::VlmError::NonFiniteResult);
        }
        self.solve_about(
            op_point,
            self.airplane.xyz_ref,
            NormalChange::SurfacePitch(wing_indices, pitch_rad),
        )
    }
}
