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
use super::fast_rows::{evaluate_row, RowScratch};
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
    let fast = crate::vlm::speed::fast_kernel();
    let columns = super::fast_kernel::all_columns(panels, fast);
    let fill_row =
        |scratch: &mut RowScratch,
         (row, (&point, field_panel)): (&mut [[f64; 3]], (&[f64; 3], &Panel))| {
            if let Some(columns) = &columns {
                evaluate_row(
                    point,
                    field_panel.wing_index,
                    columns,
                    (4.0 * std::f64::consts::PI, 1.0),
                    scratch,
                );
                for (j, entry) in row.iter_mut().enumerate() {
                    *entry = [
                        scratch.velocity[0][j],
                        scratch.velocity[1][j],
                        scratch.velocity[2][j],
                    ];
                }
                return;
            }
            for (entry, source_panel) in row.iter_mut().zip(panels) {
                *entry = super::fast_kernel::horseshoe(
                    fast,
                    point,
                    source_panel.left_vortex_vertex,
                    source_panel.right_vortex_vertex,
                    (4.0 * std::f64::consts::PI, 1.0),
                    inter_surface_core(field_panel.wing_index, source_panel),
                );
            }
        };
    if n < PARALLEL_PANEL_THRESHOLD {
        let mut scratch = RowScratch::default();
        kernel
            .chunks_mut(n)
            .zip(vortex_centers.iter().zip(panels))
            .for_each(|item| fill_row(&mut scratch, item));
    } else {
        kernel
            .par_chunks_mut(n)
            .zip(vortex_centers.par_iter().zip(panels.par_iter()))
            .for_each_init(RowScratch::default, |scratch, item| {
                fill_row(scratch, item);
            });
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
    let far_field = |i: usize| add3(steady_freestream_velocity, rotation_velocities[i]);
    if let Some(kernel) = kernel {
        // One exact division per source, rather than the same division at
        // every field point. The cached and direct kernels retain their
        // operation order.
        let constants: Vec<f64> = vortex_strengths
            .iter()
            .map(|gamma| gamma / (4.0 * std::f64::consts::PI))
            .collect();
        // Four field points advance together so their independent running
        // sums overlap the add latency; each point still adds its panels in
        // ascending order, so every sum is the one-point-at-a-time sum.
        let mut induced = vec![[0.0_f64; 3]; points.len()];
        if n < PARALLEL_PANEL_THRESHOLD {
            kernel
                .chunks(n * KERNEL_ROW_BLOCK)
                .zip(induced.chunks_mut(KERNEL_ROW_BLOCK))
                .for_each(|(rows, out)| kernel_rows(rows, n, &constants, out));
        } else {
            kernel
                .par_chunks(n * KERNEL_ROW_BLOCK)
                .zip(induced.par_chunks_mut(KERNEL_ROW_BLOCK))
                .for_each(|(rows, out)| kernel_rows(rows, n, &constants, out));
        }
        return induced
            .iter()
            .enumerate()
            .map(|(i, &sum)| add3(sum, far_field(i)))
            .collect();
    }
    let at_point = |point_index: usize, point: [f64; 3], rotation_velocity: [f64; 3]| {
        let induced =
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
                });
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

/// Field points advanced together by [`kernel_rows`].
const KERNEL_ROW_BLOCK: usize = 4;

/// `out[r] = sum_j constants[j] * rows[r * n + j]` for up to
/// [`KERNEL_ROW_BLOCK`] kernel rows, each row summed in ascending `j` as
/// `acc = acc + [c * b0, c * b1, c * b2]` from `[0, 0, 0]`: exactly the sum
/// of one row alone, whose `constant * bracket_component` is the product
/// upstream's kernel computes inline.
fn kernel_rows(rows: &[[f64; 3]], n: usize, constants: &[f64], out: &mut [[f64; 3]]) {
    if out.len() == KERNEL_ROW_BLOCK {
        let (r0, rest) = rows.split_at(n);
        let (r1, rest) = rest.split_at(n);
        let (r2, r3) = rest.split_at(n);
        let mut acc = [[0.0_f64; 3]; KERNEL_ROW_BLOCK];
        for (j, &constant) in constants.iter().enumerate() {
            for (sum, bracket) in acc.iter_mut().zip([r0[j], r1[j], r2[j], r3[j]]) {
                *sum = add3(
                    *sum,
                    [
                        constant * bracket[0],
                        constant * bracket[1],
                        constant * bracket[2],
                    ],
                );
            }
        }
        out.copy_from_slice(&acc);
    } else {
        for (row, sum) in rows.chunks(n).zip(out.iter_mut()) {
            *sum = row
                .iter()
                .zip(constants)
                .fold([0.0, 0.0, 0.0], |acc, (&bracket, &constant)| {
                    add3(
                        acc,
                        [
                            constant * bracket[0],
                            constant * bracket[1],
                            constant * bracket[2],
                        ],
                    )
                });
        }
    }
}
