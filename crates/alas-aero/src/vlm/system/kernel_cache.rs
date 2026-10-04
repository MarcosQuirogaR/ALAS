// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`VlmSystem`]'s horseshoe-kernel cache, and the near-field velocity
//! evaluation that reads it: precomputed through [`build_kernel_cache`] for
//! candidate geometry reuse, or by an ordinary system's second solve, and
//! read by subsequent [`VlmSystem::solve`] calls through [`velocity_at_points`].
//!
//! Split out of `system.rs` to keep that file under its production-line
//! budget. Both assembly paths use the same exact brackets; their only
//! consumer is `velocity_at_points`, called from `VlmSystem::solve_about`.
//!
//! [`VlmSystem`]: super::VlmSystem

use rayon::prelude::*;

use super::super::{TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};
use super::inter_surface_core;
use super::{Panel, PARALLEL_PANEL_THRESHOLD};
use crate::operating_point::OperatingPoint;
use crate::singularities::calculate_induced_velocity_horseshoe_cored;
use crate::vector3::add3;

/// Panel count above which [`super::VlmSystem`] skips this cache
/// and every `solve` falls back to evaluating the near-field velocity
/// directly, as it always did before the cache existed.
///
/// The cache holds three `f64` per (vortex center, panel) pair, so it grows
/// as `3 * n^2 * 8` bytes: 54 MB at this threshold, and unbounded above it.
/// A polar sweep or a stability-derivative stencil solves the same geometry
/// many times, which is what the cache is for; a single solve at a mesh this
/// fine is rare enough, and the matrix factorization already dominates its
/// cost, that paying the cache's own O(n^2) fill for one solve is not worth
/// the memory.
pub(super) const KERNEL_CACHE_MAX_PANELS: usize = 1500;

/// Precompute the horseshoe kernel's gamma-independent bracket
/// (`a_cross_b*term1 + a_cross_u*term2 - b_cross_u*term3` in
/// [`calculate_induced_velocity_horseshoe`]'s own terms) for every
/// (vortex center, panel) pair, or `None` above
/// [`KERNEL_CACHE_MAX_PANELS`].
///
/// Calling the kernel with `gamma = 4*PI` rather than reimplementing its
/// arithmetic is what keeps this bit-identical rather than merely close:
/// `constant = gamma / (4.0 * PI)` is then `(4*PI) / (4*PI)`, which IEEE 754
/// division rounds to exactly `1.0` for any nonzero finite divisor, and
/// `1.0 * bracket_component` is exact too, so the cached value *is* the
/// bracket, not an approximation of it. `velocity_at_points` then multiplies
/// that same bracket by the real `gamma / (4*PI)` per solve, which is the
/// same two floating-point operations the uncached kernel call performs,
/// only with the bracket computed once instead of once per solve.
///
/// Row-parallel above the same [`PARALLEL_PANEL_THRESHOLD`] the AIC assembly
/// in `system.rs` uses, for the same reason: one row (one field point
/// against every panel) is independent of every other row.
pub(super) fn build_kernel_cache(
    vortex_centers: &[[f64; 3]],
    panels: &[Panel],
) -> Option<Vec<[f64; 3]>> {
    let n = panels.len();
    if n == 0 || n > KERNEL_CACHE_MAX_PANELS {
        return None;
    }
    let mut kernel = vec![[0.0_f64; 3]; n * n];
    let fill_row = |(row, (&point, field_panel)): (&mut [[f64; 3]], (&[f64; 3], &Panel))| {
        for (entry, source_panel) in row.iter_mut().zip(panels) {
            *entry = calculate_induced_velocity_horseshoe_cored(
                point,
                source_panel.left_vortex_vertex,
                source_panel.right_vortex_vertex,
                TRAILING_VORTEX_DIRECTION,
                4.0 * std::f64::consts::PI,
                VORTEX_CORE_RADIUS,
                inter_surface_core(field_panel.wing_index, source_panel),
            );
        }
    };
    if n < PARALLEL_PANEL_THRESHOLD {
        kernel
            .chunks_mut(n)
            .zip(vortex_centers.iter().zip(panels))
            .for_each(fill_row);
    } else {
        kernel
            .par_chunks_mut(n)
            .zip(vortex_centers.par_iter().zip(panels.par_iter()))
            .for_each(fill_row);
    }
    Some(kernel)
}

/// The velocity every horseshoe vortex (strength `vortex_strengths[j]`)
/// induces at `points[i]`, summed over every panel, plus the freestream and
/// rotation-induced velocity at that point: `get_velocity_at_points`
/// (through `get_induced_velocity_at_points`), scoped to the internal use
/// `VlmSystem::solve` makes of it. Parallel over points; the sum over panels
/// for one point is sequential and in panel order, so the result does not
/// depend on the pool.
///
/// `kernel`, when given, is [`build_kernel_cache`]'s cached horseshoe
/// bracket for exactly these `points` against these `panels`, row `i`
/// holding panel `0..panels.len()`'s bracket at `points[i]`: see that
/// function's own doc for why reading `constant * kernel[i * panels.len() +
/// j]` here reproduces `calculate_induced_velocity_horseshoe`'s own
/// arithmetic bit-for-bit rather than approximating it.
pub(super) fn velocity_at_points(
    points: &[[f64; 3]],
    panels: &[Panel],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
    steady_freestream_velocity: [f64; 3],
    reference: [f64; 3],
    kernel: Option<&[[f64; 3]]>,
) -> Vec<[f64; 3]> {
    let rotation_velocities = op_point.rotation_velocity_geometry_axes_about(points, reference);
    let n = panels.len();
    // One exact division per source, rather than the same division at every
    // field point. The cached and direct kernels retain their operation order.
    let constants: Vec<_> = if kernel.is_some() {
        vortex_strengths
            .iter()
            .map(|gamma| gamma / (4.0 * std::f64::consts::PI))
            .collect()
    } else {
        Vec::new()
    };
    let at_point = |point_index: usize, point: [f64; 3], rotation_velocity: [f64; 3]| {
        let induced = match kernel {
            Some(kernel) => {
                let row = &kernel[point_index * n..point_index * n + n];
                row.iter()
                    .zip(&constants)
                    .fold([0.0, 0.0, 0.0], |acc, (&bracket, &constant)| {
                        // The exact same `constant * bracket_component`
                        // upstream's kernel computes inline; only the
                        // bracket itself (independent of gamma) was
                        // precomputed, in `build_kernel_cache`.
                        let contribution = [
                            constant * bracket[0],
                            constant * bracket[1],
                            constant * bracket[2],
                        ];
                        add3(acc, contribution)
                    })
            }
            None => {
                panels
                    .iter()
                    .zip(vortex_strengths)
                    .fold([0.0, 0.0, 0.0], |acc, (panel, &gamma)| {
                        let contribution = calculate_induced_velocity_horseshoe_cored(
                            point,
                            panel.left_vortex_vertex,
                            panel.right_vortex_vertex,
                            TRAILING_VORTEX_DIRECTION,
                            gamma,
                            VORTEX_CORE_RADIUS,
                            inter_surface_core(panels[point_index].wing_index, panel),
                        );
                        add3(acc, contribution)
                    })
            }
        };
        add3(induced, add3(steady_freestream_velocity, rotation_velocity))
    };
    if panels.len() < PARALLEL_PANEL_THRESHOLD {
        points
            .iter()
            .zip(&rotation_velocities)
            .enumerate()
            .map(|(i, (&point, &rotation_velocity))| at_point(i, point, rotation_velocity))
            .collect()
    } else {
        points
            .par_iter()
            .zip(rotation_velocities.par_iter())
            .enumerate()
            .map(|(i, (&point, &rotation_velocity))| at_point(i, point, rotation_velocity))
            .collect()
    }
}
