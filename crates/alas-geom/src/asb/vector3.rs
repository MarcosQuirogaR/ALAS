// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from aerosandbox/numpy/rotations.py (the `rotation_matrix_3D`
// vector-axis branch) and the plain vector arithmetic `aerosandbox.numpy`
// performs on 3-element geometry-axis arrays throughout `wing.py`.
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! Plain 3-vector arithmetic and the axis-angle rotation matrix
//! [`Wing`](super::wing::Wing)'s frame computation needs.
//!
//! Ordinary closed-form linear algebra local to `asb`, not `alas-math`: that
//! crate exists for numerics with state to get wrong -- a spline fit, a
//! factorization -- and this is a handful of formulas with one caller.

/// `a + b`, componentwise.
pub(super) fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a - b`, componentwise.
pub(super) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a * s`, componentwise.
pub(super) fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The dot product of `a` and `b`.
pub(super) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The cross product `a x b`.
pub(super) fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The Euclidean length of `a`.
pub(super) fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

/// Zero the X component and normalize to unit length --
/// `project_to_YZ_plane_and_normalize`, `_compute_frame_of_WingXSec`'s
/// local helper.
pub(super) fn project_to_yz_and_normalize(v: [f64; 3]) -> [f64; 3] {
    let magnitude = (v[1] * v[1] + v[2] * v[2]).sqrt();
    [0.0, v[1] / magnitude, v[2] / magnitude]
}

/// The weighted blend `a_weight * a + b_weight * b`, componentwise --
/// `xsec_a.xyz_le * a_weight + xsec_b.xyz_le * b_weight` in
/// `Wing.subdivide_sections`.
pub(super) fn blend3(a: [f64; 3], b: [f64; 3], a_weight: f64, b_weight: f64) -> [f64; 3] {
    add3(scale3(a, a_weight), scale3(b, b_weight))
}

/// The 3x3 rotation matrix for a right-handed rotation by `angle_rad` about
/// `axis`, Rodrigues' formula -- `aerosandbox.numpy.rotations.rotation_matrix_3D`'s
/// vector-axis branch, always with `axis_already_normalized=False` (its
/// every call site here passes an already-unit vector, but upstream
/// normalizes anyway, so this does too).
///
/// An implementation of
/// <https://en.wikipedia.org/wiki/Rotation_matrix#Rotation_matrix_from_axis_and_angle>.
pub(super) fn rotation_matrix_3d(angle_rad: f64, axis: [f64; 3]) -> [[f64; 3]; 3] {
    let s = angle_rad.sin();
    let c = angle_rad.cos();
    let norm = norm3(axis);
    let (ux, uy, uz) = (axis[0] / norm, axis[1] / norm, axis[2] / norm);
    [
        [
            c + ux * ux * (1.0 - c),
            ux * uy * (1.0 - c) - uz * s,
            ux * uz * (1.0 - c) + uy * s,
        ],
        [
            uy * ux * (1.0 - c) + uz * s,
            c + uy * uy * (1.0 - c),
            uy * uz * (1.0 - c) - ux * s,
        ],
        [
            uz * ux * (1.0 - c) - uy * s,
            uz * uy * (1.0 - c) + ux * s,
            c + uz * uz * (1.0 - c),
        ],
    ]
}

/// `m @ v`.
pub(super) fn matvec3(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matrix_3d_of_zero_angle_is_the_identity() {
        let m = rotation_matrix_3d(0.0, [1.0, 0.0, 0.0]);
        assert!((matvec3(m, [1.0, 2.0, 3.0])[0] - 1.0).abs() < 1e-15);
        assert!((matvec3(m, [1.0, 2.0, 3.0])[1] - 2.0).abs() < 1e-15);
        assert!((matvec3(m, [1.0, 2.0, 3.0])[2] - 3.0).abs() < 1e-15);
    }

    #[test]
    fn rotation_matrix_3d_of_ninety_degrees_about_z_rotates_x_to_y() {
        let m = rotation_matrix_3d(std::f64::consts::FRAC_PI_2, [0.0, 0.0, 1.0]);
        let rotated = matvec3(m, [1.0, 0.0, 0.0]);
        assert!((rotated[0]).abs() < 1e-12);
        assert!((rotated[1] - 1.0).abs() < 1e-12);
        assert!((rotated[2]).abs() < 1e-12);
    }

    #[test]
    fn cross3_of_x_and_y_axes_is_the_z_axis() {
        let result = cross3([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(result, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn norm3_of_a_unit_axis_vector_is_one() {
        assert!((norm3([0.0, 1.0, 0.0]) - 1.0).abs() < 1e-15);
    }
}
