// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bit-identical fast path of [`calculate_induced_velocity_horseshoe_cored`]
//! for the influence-matrix and near-field-kernel fill.
//!
//! The cored kernel evaluates the bare horseshoe and then, for an
//! inter-surface pair, three point-to-leg distances (three square roots and
//! a division) only to find that the field point is outside every leg core,
//! when it returns the bare value unchanged. Almost every pair is. This path
//! first proves the field point is farther than the core from each leg with
//! a conservative, division-free bound; only if that proof fails does it
//! call the ordinary cored kernel. The bound:
//!
//! * bound leg: the distance to the segment is at least the gap to the
//!   segment's axis-aligned bounding box;
//! * trailing legs (half-lines along `+x`): at least the lateral offset
//!   `max(|dy|, |dz|)`.
//!
//! The threshold is the core radius plus a relative `1e-6` and an absolute
//! `1e-9` m, many orders above the `~1e-14` m rounding of the computed
//! distances, so a proof implies the ordinary kernel's `distance >= core`
//! test is true for all three legs and it returns the bare value: the same
//! arithmetic, hence the same bits. The fast path is selected by
//! [`crate::vlm::speed::fast_kernel`].

use super::super::{TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};
use super::Panel;
use crate::singularities::calculate_induced_velocity_horseshoe_cored;
use crate::vector3::{cross3, dot3, norm3, sub3};

/// Relative and absolute (m) margin of the outside-the-core proof.
pub(super) const REL_MARGIN: f64 = 1.0e-6;
pub(super) const ABS_MARGIN: f64 = 1.0e-9;

/// True if `field` is provably farther than `core` from the bound leg
/// `left`-`right` and from both trailing half-lines.
#[inline(always)]
fn clear_of_every_leg(field: [f64; 3], left: [f64; 3], right: [f64; 3], core: f64) -> bool {
    let threshold = core * (1.0 + REL_MARGIN) + ABS_MARGIN;
    let lateral = |leg: [f64; 3]| (field[1] - leg[1]).abs().max((field[2] - leg[2]).abs());
    if lateral(left) > threshold && lateral(right) > threshold {
        // Bound leg, by bounding box (the trailing legs are cleared above).
        let mut gap = 0.0_f64;
        for axis in 0..3 {
            let (lo, hi) = if left[axis] < right[axis] {
                (left[axis], right[axis])
            } else {
                (right[axis], left[axis])
            };
            gap = gap.max(lo - field[axis]).max(field[axis] - hi);
        }
        return gap > threshold;
    }
    false
}

/// `1 / x` smoothed by the Kaufmann core, `x / (x^2 + r^2)`, the only
/// branch ([`VORTEX_CORE_RADIUS`] is nonzero) of the upstream `smoothed_inv`.
#[inline(always)]
fn smoothed_inv(x: f64) -> f64 {
    x / (x * x + VORTEX_CORE_RADIUS * VORTEX_CORE_RADIUS)
}

/// [`crate::singularities::calculate_induced_velocity_horseshoe`] with the
/// strength constant `gamma / (4 pi)` supplied by the caller (it is the same
/// division for every entry of a fill) and the core radius fixed at
/// [`VORTEX_CORE_RADIUS`]: the same operations in the same order.
#[inline(always)]
pub(super) fn bare_horseshoe(
    field: [f64; 3],
    left: [f64; 3],
    right: [f64; 3],
    constant: f64,
) -> [f64; 3] {
    let a = sub3(field, left);
    let b = sub3(field, right);
    let u = TRAILING_VORTEX_DIRECTION;
    let a_cross_b = cross3(a, b);
    let a_dot_b = dot3(a, b);
    let a_cross_u = cross3(a, u);
    let a_dot_u = dot3(a, u);
    let b_cross_u = cross3(b, u);
    let b_dot_u = dot3(b, u);
    let norm_a = norm3(a);
    let norm_b = norm3(b);
    let norm_a_inv = smoothed_inv(norm_a);
    let norm_b_inv = smoothed_inv(norm_b);
    let term1 = (norm_a_inv + norm_b_inv) * smoothed_inv(norm_a * norm_b + a_dot_b);
    let term2 = norm_a_inv * smoothed_inv(norm_a - a_dot_u);
    let term3 = norm_b_inv * smoothed_inv(norm_b - b_dot_u);
    [
        constant * (a_cross_b[0] * term1 + a_cross_u[0] * term2 - b_cross_u[0] * term3),
        constant * (a_cross_b[1] * term1 + a_cross_u[1] * term2 - b_cross_u[1] * term3),
        constant * (a_cross_b[2] * term1 + a_cross_u[2] * term2 - b_cross_u[2] * term3),
    ]
}

/// [`calculate_induced_velocity_horseshoe_cored`] with this module's proof
/// first when `fast`; bit-identical either way. `constant` must equal
/// `gamma / (4.0 * PI)` (the caller hoists that division).
#[inline]
pub(super) fn horseshoe(
    fast: bool,
    field: [f64; 3],
    left: [f64; 3],
    right: [f64; 3],
    (gamma, constant): (f64, f64),
    leg_core_radius: f64,
) -> [f64; 3] {
    if fast
        && TRAILING_VORTEX_DIRECTION == [1.0, 0.0, 0.0]
        && leg_core_radius > 0.0
        && leg_core_radius.is_finite()
        && field
            .iter()
            .chain(&left)
            .chain(&right)
            .all(|v| v.is_finite())
        && clear_of_every_leg(field, left, right, leg_core_radius)
    {
        return bare_horseshoe(field, left, right, constant);
    }
    calculate_induced_velocity_horseshoe_cored(
        field,
        left,
        right,
        TRAILING_VORTEX_DIRECTION,
        gamma,
        VORTEX_CORE_RADIUS,
        leg_core_radius,
    )
}

/// One source panel's horseshoe, contiguous for the fill loop.
pub(super) struct Source {
    pub(super) left: [f64; 3],
    pub(super) right: [f64; 3],
    pub(super) wing_index: usize,
    pub(super) leg_core_radius: f64,
}

fn sources_of(panels: &[Panel]) -> Vec<Source> {
    panels
        .iter()
        .map(|panel| Source {
            left: panel.left_vortex_vertex,
            right: panel.right_vortex_vertex,
            wing_index: panel.wing_index,
            leg_core_radius: super::inter_surface_core(usize::MAX, panel),
        })
        .collect()
}

/// Every panel's horseshoe as one run of columns, when `fast` and the row
/// form applies (see [`super::fast_rows`]); `None` selects the scalar path.
pub(super) fn all_columns(panels: &[Panel], fast: bool) -> Option<super::fast_rows::Columns> {
    (fast && TRAILING_VORTEX_DIRECTION == [1.0, 0.0, 0.0]).then(|| {
        super::fast_rows::Columns::gather(&sources_of(panels), (0..panels.len()).collect())
    })
}

/// Recompute the influence-matrix and near-field-kernel entries whose row or
/// column panel is `changed`: `aic[i * n + j]` is the velocity of horseshoe
/// `j` (unit strength) at collocation point `i` along its normal, and
/// `kernel[i * n + j]` the `4 pi`-strength bracket at vortex center `i`, as
/// the ordinary loops computed them. Both entries of a pair come from one
/// pass over a contiguous source table.
pub(super) fn fill_changed(
    panels: &[Panel],
    changed: &[bool],
    fast: bool,
    aic: &mut [f64],
    mut kernel: Option<&mut [[f64; 3]]>,
) {
    let n = panels.len();
    let sources = sources_of(panels);
    // `gamma / (4 pi)` for the two strengths used: 1 and 4 pi (exactly 1).
    let four_pi = 4.0 * std::f64::consts::PI;
    let unit_constant = 1.0 / four_pi;
    // IEEE 754 division of a finite nonzero number by itself is exactly 1.0
    // (the kernel-cache doc relies on the same fact), hence the `1.0` below.
    let columns: Vec<usize> = (0..n).filter(|&j| changed[j]).collect();
    if fast && TRAILING_VORTEX_DIRECTION == [1.0, 0.0, 0.0] {
        use super::fast_rows::{evaluate_row, Columns, RowScratch};
        let (every, moved) = (
            Columns::gather(&sources, (0..n).collect()),
            Columns::gather(&sources, columns),
        );
        let mut scratch = RowScratch::default();
        for (i, (&row_changed, field)) in changed.iter().zip(panels).enumerate() {
            let run = if row_changed { &every } else { &moved };
            let at = |scratch: &RowScratch, k: usize| {
                [
                    scratch.velocity[0][k],
                    scratch.velocity[1][k],
                    scratch.velocity[2][k],
                ]
            };
            evaluate_row(
                field.collocation_point,
                field.wing_index,
                run,
                (1.0, unit_constant),
                &mut scratch,
            );
            for (k, &j) in run.index.iter().enumerate() {
                aic[i * n + j] = dot3(at(&scratch, k), field.normal_direction);
            }
            if let Some(kernel) = kernel.as_deref_mut() {
                evaluate_row(
                    field.vortex_center,
                    field.wing_index,
                    run,
                    (four_pi, 1.0),
                    &mut scratch,
                );
                for (k, &j) in run.index.iter().enumerate() {
                    kernel[i * n + j] = at(&scratch, k);
                }
            }
        }
        return;
    }
    let mut entry = |i: usize, j: usize| {
        let (field, source) = (&panels[i], &sources[j]);
        let core = if source.wing_index == field.wing_index {
            0.0
        } else {
            source.leg_core_radius
        };
        let induced = horseshoe(
            fast,
            field.collocation_point,
            source.left,
            source.right,
            (1.0, unit_constant),
            core,
        );
        aic[i * n + j] = dot3(induced, field.normal_direction);
        if let Some(kernel) = kernel.as_deref_mut() {
            kernel[i * n + j] = horseshoe(
                fast,
                field.vortex_center,
                source.left,
                source.right,
                (four_pi, 1.0),
                core,
            );
        }
    };
    for (i, &row_changed) in changed.iter().enumerate() {
        if row_changed {
            (0..n).for_each(|j| entry(i, j));
        } else {
            columns.iter().for_each(|&j| entry(i, j));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random doubles in [-1, 1).
    fn stream(state: &mut u64) -> f64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((*state >> 11) as f64) / ((1_u64 << 52) as f64) - 1.0
    }

    #[test]
    fn fast_path_is_bit_identical_including_inside_and_near_cores() {
        let mut state = 7_u64;
        let mut proofs = 0_usize;
        for case in 0..200_000_u32 {
            // Mix near-field draws (inside the cores) with far-field draws.
            let spread = if case % 3 == 0 { 0.2 } else { 30.0 };
            let point =
                |s: &mut u64, scale: f64| [scale * stream(s), scale * stream(s), scale * stream(s)];
            let left = point(&mut state, 5.0);
            let mut right = point(&mut state, 5.0);
            right[0] = left[0] + 0.1 * stream(&mut state);
            let mut field = point(&mut state, spread);
            if case % 7 == 0 {
                // On a leg line, exactly at a vertex, or just beside a trailing leg.
                field = [
                    left[0] + 3.0 * stream(&mut state).abs(),
                    left[1] + 1.0e-4 * stream(&mut state),
                    left[2],
                ];
            }
            if case % 11 == 0 {
                field = left;
            }
            let core = 0.05 * (1.0 + stream(&mut state).abs());
            if clear_of_every_leg(field, left, right, core) {
                proofs += 1;
            }
            let fast = horseshoe(
                true,
                field,
                left,
                right,
                (1.0, 1.0 / (4.0 * std::f64::consts::PI)),
                core,
            );
            let slow = horseshoe(
                false,
                field,
                left,
                right,
                (1.0, 1.0 / (4.0 * std::f64::consts::PI)),
                core,
            );
            for axis in 0..3 {
                assert_eq!(fast[axis].to_bits(), slow[axis].to_bits(), "case {case}");
            }
        }
        assert!(
            proofs > 50_000,
            "the proof should cover most far pairs, got {proofs}"
        );
    }
}
