// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Row-at-a-time form of the fast kernel fill, written so the compiler can
//! evaluate several sources per instruction.
//!
//! One field point against a run of sources is a branch-free loop over
//! structure-of-arrays inputs: the bare horseshoe of
//! [`super::fast_kernel`] for every source, plus a flag marking the sources
//! whose leg cores the division-free proof of that module could not clear.
//! The flagged few are then recomputed with the ordinary cored kernel. Every
//! lane performs exactly the scalar operations of the scalar path (IEEE
//! `+ - * / sqrt` are correctly rounded per lane and Rust never fuses
//! multiply-adds), so the entries are bit-identical to it.

use super::super::{TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};
use super::fast_kernel::{bare_horseshoe, Source, ABS_MARGIN, REL_MARGIN};
use crate::singularities::calculate_induced_velocity_horseshoe_cored;

/// A run of sources, one array per coordinate, with the original panel
/// index of each.
pub(super) struct Columns {
    pub(super) index: Vec<usize>,
    left: [Vec<f64>; 3],
    right: [Vec<f64>; 3],
    wing_index: Vec<usize>,
    leg_core_radius: Vec<f64>,
}

impl Columns {
    pub(super) fn gather(sources: &[Source], indices: Vec<usize>) -> Self {
        let pick = |f: &dyn Fn(&Source) -> f64| -> Vec<f64> {
            indices.iter().map(|&j| f(&sources[j])).collect()
        };
        Self {
            left: [
                pick(&|s| s.left[0]),
                pick(&|s| s.left[1]),
                pick(&|s| s.left[2]),
            ],
            right: [
                pick(&|s| s.right[0]),
                pick(&|s| s.right[1]),
                pick(&|s| s.right[2]),
            ],
            wing_index: indices.iter().map(|&j| sources[j].wing_index).collect(),
            leg_core_radius: pick(&|s| s.leg_core_radius),
            index: indices,
        }
    }

    fn len(&self) -> usize {
        self.index.len()
    }
}

/// Results for one field point against a [`Columns`] run.
#[derive(Default)]
pub(super) struct RowScratch {
    pub(super) velocity: [Vec<f64>; 3],
    core: Vec<f64>,
    exact: Vec<bool>,
}

/// Velocity of every source in `columns` (strength `gamma`, strength constant
/// `constant = gamma / (4 pi)`) at `field`, a point of wing `field_wing`.
pub(super) fn evaluate_row(
    field: [f64; 3],
    field_wing: usize,
    columns: &Columns,
    (gamma, constant): (f64, f64),
    scratch: &mut RowScratch,
) {
    let m = columns.len();
    scratch.core.clear();
    scratch.core.extend(
        columns
            .wing_index
            .iter()
            .zip(&columns.leg_core_radius)
            .map(|(&wing, &core)| if wing == field_wing { 0.0 } else { core }),
    );
    for axis in &mut scratch.velocity {
        axis.clear();
        axis.resize(m, 0.0);
    }
    scratch.exact.clear();
    scratch.exact.resize(m, false);
    let [lx, ly, lz] = [
        &columns.left[0][..m],
        &columns.left[1][..m],
        &columns.left[2][..m],
    ];
    let [rx, ry, rz] = [
        &columns.right[0][..m],
        &columns.right[1][..m],
        &columns.right[2][..m],
    ];
    let core = &scratch.core[..m];
    let [vx, vy, vz] = &mut scratch.velocity;
    let (vx, vy, vz) = (&mut vx[..m], &mut vy[..m], &mut vz[..m]);
    let exact = &mut scratch.exact[..m];
    for j in 0..m {
        let left = [lx[j], ly[j], lz[j]];
        let right = [rx[j], ry[j], rz[j]];
        let bare = bare_horseshoe(field, left, right, constant);
        vx[j] = bare[0];
        vy[j] = bare[1];
        vz[j] = bare[2];
        // Comparisons only, so a NaN anywhere fails the proof and the
        // source takes the exact path.
        let t = core[j] * (1.0 + REL_MARGIN) + ABS_MARGIN;
        let trailing_left = (field[1] - left[1]).abs() > t || (field[2] - left[2]).abs() > t;
        let trailing_right = (field[1] - right[1]).abs() > t || (field[2] - right[2]).abs() > t;
        let mut bound = false;
        for axis in 0..3 {
            let (lo, hi) = if left[axis] < right[axis] {
                (left[axis], right[axis])
            } else {
                (right[axis], left[axis])
            };
            bound |= lo - field[axis] > t || field[axis] - hi > t;
        }
        let finite = left[0].is_finite() && right[0].is_finite() && field[0].is_finite();
        let clear = trailing_left && trailing_right && bound && finite;
        exact[j] = core[j] > 0.0 && !clear;
    }
    for j in 0..m {
        if scratch.exact[j] {
            let v = calculate_induced_velocity_horseshoe_cored(
                field,
                [lx[j], ly[j], lz[j]],
                [rx[j], ry[j], rz[j]],
                TRAILING_VORTEX_DIRECTION,
                gamma,
                VORTEX_CORE_RADIUS,
                scratch.core[j],
            );
            scratch.velocity[0][j] = v[0];
            scratch.velocity[1][j] = v[1];
            scratch.velocity[2][j] = v[2];
        }
    }
}
