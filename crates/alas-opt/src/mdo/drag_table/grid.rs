// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Grid construction and interpolation kernels of [`super::TrimmedDragTable`].

use super::{
    DragTableError, CL_STEP, MACH_ABOVE_CRUISE, MACH_COARSE_STEP, MACH_FINE_FROM, MACH_FINE_STEP,
    MACH_MIN,
};

/// CL nodes from [`CL_STEP`] to `cl_max`, the last one exactly `cl_max`.
pub(super) fn cl_grid(cl_max: f64) -> Result<Vec<f64>, DragTableError> {
    if !cl_max.is_finite() || cl_max < 2.0 * CL_STEP {
        return Err(DragTableError::NonPhysical("clean CLmax"));
    }
    let mut cl: Vec<f64> = (1..)
        .map(|n| n as f64 * CL_STEP)
        .take_while(|&c| c < cl_max - 1e-9)
        .collect();
    cl.push(cl_max);
    Ok(cl)
}

/// Mach nodes per the module doc, with the wave-onset Mach as a node.
pub(super) fn mach_grid(cruise_mach: f64, onset_mach: f64) -> Vec<f64> {
    let top = cruise_mach + MACH_ABOVE_CRUISE;
    let mut mach: Vec<f64> = (0..)
        .map(|n| MACH_MIN + n as f64 * MACH_COARSE_STEP)
        .take_while(|&m| m < MACH_FINE_FROM.min(top) - 1e-9)
        .collect();
    mach.extend(
        (0..)
            .map(|n| MACH_FINE_FROM + n as f64 * MACH_FINE_STEP)
            .take_while(|&m| m < top - 1e-9),
    );
    mach.push(top);
    if onset_mach > MACH_MIN && onset_mach < top {
        mach.push(onset_mach);
    }
    mach.sort_by(f64::total_cmp);
    mach.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    mach
}

/// Index of the cell holding `x` and the linear weight in it, clamped.
pub(super) fn cell(nodes: &[f64], x: f64) -> (usize, f64) {
    let last = nodes.len() - 2;
    let x = x.clamp(nodes[0], nodes[last + 1]);
    let index = nodes
        .partition_point(|&node| node <= x)
        .saturating_sub(1)
        .min(last);
    let weight = (x - nodes[index]) / (nodes[index + 1] - nodes[index]);
    (index, weight)
}

pub(super) fn lerp(a: f64, b: f64, weight: f64) -> f64 {
    a + weight * (b - a)
}

/// Cubic Hermite value and derivative on `[x0, x1]` from end values and slopes.
pub(super) fn hermite(
    x0: f64,
    x1: f64,
    (f0, s0): (f64, f64),
    (f1, s1): (f64, f64),
    x: f64,
) -> (f64, f64) {
    let h = x1 - x0;
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    let value = (2.0 * t3 - 3.0 * t2 + 1.0) * f0
        + (t3 - 2.0 * t2 + t) * h * s0
        + (-2.0 * t3 + 3.0 * t2) * f1
        + (t3 - t2) * h * s1;
    let slope = (6.0 * t2 - 6.0 * t) / h * f0
        + (3.0 * t2 - 4.0 * t + 1.0) * s0
        + (6.0 * t - 6.0 * t2) / h * f1
        + (3.0 * t2 - 2.0 * t) * s1;
    (value, slope)
}
