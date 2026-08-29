// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from
// native aerodynamic model/aerodynamics/aero_3D/singularities/uniform_strength_horseshoe_singularities.py
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The induced-velocity kernel of a single horseshoe vortex --
//! `calculate_induced_velocity_horseshoe` -- the potential-flow element
//! [`crate::vlm`] panels every wing into.
//!
//! Upstream's function is written for NumPy broadcasting: every argument can
//! be a scalar or an array, and the field point and the vortex vertices
//! broadcast against each other to fill an `N x M` matrix of results in one
//! call. `alas-aero::vlm` is the only caller here, and it needs the same
//! *values* -- one field point against many panels for the AIC matrix, one
//! field point against many gamma-weighted panels for the near-field
//! velocity -- but reaches them with an explicit loop rather than array
//! broadcasting, matching how [`crate::operating_point::OperatingPoint::rotation_velocity_geometry_axes`]
//! resolves the same upstream broadcasting pattern one point at a time. This
//! module therefore has one scalar entry point,
//! [`calculate_induced_velocity_horseshoe`], operating on one field point and
//! one horseshoe at a time; the broadcast and the sum both live in the
//! caller.
//!
//! # `vortex_core_radius` and the branch this program never takes
//!
//! `smoothed_inv`'s `vortex_core_radius == 0` branch (plain `1/x`, upstream's
//! own default) is translated -- it costs nothing and keeps this function a
//! complete port of the one upstream function it stands in for -- but no
//! input this crate constructs ever reaches it: `VortexLatticeMethod`'s
//! constructor default is `1e-8`, and its two call sites in
//! `alas/physics/aerodynamics.py` and `alas/physics/stability.py` never
//! override it (grepped directly against both files). The fixture and every
//! caller in [`crate::vlm`] therefore only exercise the smoothed branch.
//!
//! # `trailing_vortex_direction`
//!
//! Upstream defaults this to `[1, 0, 0]` when `None` is passed; every caller
//! here always supplies it explicitly (`vlm` resolves
//! `align_trailing_vortices_with_wind`, always `false` at this program's call
//! sites, into the constant `[1, 0, 0]` before calling in), so this function
//! takes it as a required argument with no `Option` to unwrap.

use crate::vector3::{cross3, dot3, norm3, sub3};

/// `1 / x`, smoothed near `x = 0` by a Kaufmann vortex core model when
/// `vortex_core_radius != 0` -- `smoothed_inv` in the upstream module,
/// inlined as a closure there and named here for the same reason every other
/// port in this crate turns a nested Python function into a named one: it is
/// called several times per field point and reads better named than repeated.
fn smoothed_inv(x: f64, vortex_core_radius: f64) -> f64 {
    if vortex_core_radius != 0.0 {
        x / (x * x + vortex_core_radius * vortex_core_radius)
    } else {
        1.0 / x
    }
}

/// The velocity a single horseshoe vortex -- bound leg from `left` to
/// `right`, trailing legs extending along `trailing_vortex_direction` from
/// each end -- induces at `field`, for a filament of strength `gamma` --
/// `calculate_induced_velocity_horseshoe`, one field point and one horseshoe
/// at a time (see the module doc).
///
/// `vortex_core_radius` governs the Kaufmann vortex core model's smoothing
/// radius; it should be well below the shortest bound leg in the analysis,
/// which is what keeps the induced velocity finite as `field` approaches a
/// vertex or either leg's own line.
pub fn calculate_induced_velocity_horseshoe(
    field: [f64; 3],
    left: [f64; 3],
    right: [f64; 3],
    trailing_vortex_direction: [f64; 3],
    gamma: f64,
    vortex_core_radius: f64,
) -> [f64; 3] {
    let a = sub3(field, left);
    let b = sub3(field, right);
    let u = trailing_vortex_direction;

    let a_cross_b = cross3(a, b);
    let a_dot_b = dot3(a, b);

    let a_cross_u = cross3(a, u);
    let a_dot_u = dot3(a, u);

    let b_cross_u = cross3(b, u);
    let b_dot_u = dot3(b, u);

    let norm_a = norm3(a);
    let norm_b = norm3(b);
    let norm_a_inv = smoothed_inv(norm_a, vortex_core_radius);
    let norm_b_inv = smoothed_inv(norm_b, vortex_core_radius);

    let term1 =
        (norm_a_inv + norm_b_inv) * smoothed_inv(norm_a * norm_b + a_dot_b, vortex_core_radius);
    let term2 = norm_a_inv * smoothed_inv(norm_a - a_dot_u, vortex_core_radius);
    let term3 = norm_b_inv * smoothed_inv(norm_b - b_dot_u, vortex_core_radius);

    let constant = gamma / (4.0 * std::f64::consts::PI);

    [
        constant * (a_cross_b[0] * term1 + a_cross_u[0] * term2 - b_cross_u[0] * term3),
        constant * (a_cross_b[1] * term1 + a_cross_u[1] * term2 - b_cross_u[1] * term3),
        constant * (a_cross_b[2] * term1 + a_cross_u[2] * term2 - b_cross_u[2] * term3),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAILING_X: [f64; 3] = [1.0, 0.0, 0.0];

    #[test]
    fn a_horseshoe_induces_no_velocity_on_a_field_point_infinitely_far_away() {
        // Not literally infinite, but far enough that every term should be
        // small relative to a nearby evaluation.
        let near = calculate_induced_velocity_horseshoe(
            [0.0, 0.0, 0.1],
            [-1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            TRAILING_X,
            1.0,
            1e-8,
        );
        let far = calculate_induced_velocity_horseshoe(
            [0.0, 0.0, 1.0e6],
            [-1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            TRAILING_X,
            1.0,
            1e-8,
        );
        let near_norm = (near[0] * near[0] + near[1] * near[1] + near[2] * near[2]).sqrt();
        let far_norm = (far[0] * far[0] + far[1] * far[1] + far[2] * far[2]).sqrt();
        assert!(
            far_norm < near_norm * 1e-6,
            "near={near_norm} far={far_norm}"
        );
    }

    #[test]
    fn the_induced_velocity_is_linear_in_gamma() {
        let left = [-1.0, -1.0, 0.0];
        let right = [-1.0, 1.0, 0.0];
        let field = [0.3, 0.2, 0.5];
        let base = calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 1.0, 1e-8);
        let doubled =
            calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 2.0, 1e-8);
        for i in 0..3 {
            assert!((doubled[i] - 2.0 * base[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn a_smaller_core_radius_produces_a_larger_velocity_arbitrarily_close_to_a_vertex() {
        // The whole point of the smoothing term: without it, this would be a
        // division by (near) zero rather than a large finite number.
        let left = [-1.0, -1.0, 0.0];
        let right = [-1.0, 1.0, 0.0];
        let field = [-1.0 + 1e-6, -1.0, 0.0]; // near the left vertex
        let wide_core =
            calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 1.0, 1e-2);
        let narrow_core =
            calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 1.0, 1e-8);
        let wide_norm = (wide_core[0] * wide_core[0]
            + wide_core[1] * wide_core[1]
            + wide_core[2] * wide_core[2])
            .sqrt();
        let narrow_norm = (narrow_core[0] * narrow_core[0]
            + narrow_core[1] * narrow_core[1]
            + narrow_core[2] * narrow_core[2])
            .sqrt();
        assert!(
            narrow_norm > wide_norm,
            "narrow={narrow_norm} wide={wide_norm}"
        );
    }

    #[test]
    fn the_zero_core_radius_branch_agrees_with_the_smoothed_branch_far_from_every_leg() {
        // The two branches of `smoothed_inv` only disagree near a
        // singularity; far away, `x / (x^2 + r^2)` with a tiny `r` and `1/x`
        // itself should be indistinguishable.
        let left = [-1.0, -1.0, 0.0];
        let right = [-1.0, 1.0, 0.0];
        let field = [2.0, 3.0, 4.0];
        let smoothed =
            calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 1.0, 1e-8);
        let plain = calculate_induced_velocity_horseshoe(field, left, right, TRAILING_X, 1.0, 0.0);
        for i in 0..3 {
            assert!((smoothed[i] - plain[i]).abs() < 1e-9, "component {i}");
        }
    }
}
