// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plain 3-vector arithmetic for the horseshoe kernel and the VLM panel
//! geometry.
//!
//! Ordinary closed-form linear algebra local to this crate, not `alas-math`:
//! that crate exists for numerics with state to get wrong: a spline fit, a
//! factorization, and this is a handful of formulas with two callers both
//! inside `alas-aero`. `alas-geom::aircraft::vector3` makes the identical
//! argument for its own copy of the same handful of formulas; the two are not
//! shared because `alas-geom`'s is `pub(super)`, private to that crate's
//! aircraft geometry module.

/// `a + b`, componentwise.
pub(crate) fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a - b`, componentwise.
pub(crate) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a * s`, componentwise.
pub(crate) fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The dot product of `a` and `b`.
pub(crate) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The cross product `a x b`.
pub(crate) fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The Euclidean length of `a`.
pub(crate) fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross3_of_x_and_y_axes_is_the_z_axis() {
        assert_eq!(cross3([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn norm3_of_a_3_4_0_vector_is_5() {
        assert!((norm3([3.0, 4.0, 0.0]) - 5.0).abs() < 1e-15);
    }

    #[test]
    fn dot3_of_perpendicular_vectors_is_zero() {
        assert_eq!(dot3([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), 0.0);
    }
}
