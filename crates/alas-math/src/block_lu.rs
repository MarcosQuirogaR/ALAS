// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Schur-complement solve of a dense system when only a few rows and columns
//! change between factorizations.
//!
//! Split the unknowns into a frozen set `U` (the block `A_UU` is the same
//! as in a stored base) and a moving set `T`:
//!
//! ```text
//!     [ A_UU  A_UT ]
//! A = [            ]
//!     [ A_TU  A_TT ]
//! ```
//!
//! `A_UU` is factored once, `P A_UU = L U` ([`BlockBase`]). Each updated
//! matrix then needs only `W = A_UU^-1 A_UT` (`2 m^2 k` flop for `m = |U|`,
//! `k = |T|`) and the Schur complement `S = A_TT - A_TU W` with its LU
//! (`2 k^3 / 3`), instead of the `2 n^3 / 3` of a full LU
//! ([`BlockFactorization`]). A solve of `A x = b` is
//!
//! ```text
//! y   = A_UU^-1 b_U,   x_T = S^-1 (b_T - A_TU y),   x_U = y - W x_T
//! ```
//!
//! the block elimination of the same matrix: it differs from the ordinary
//! partial-pivot LU only by rounding (pivoting is confined to each block).
//! Nothing is linearized; the updated matrix is exact. The conditioning
//! proxy is `max |pivot| / min |pivot|` over both blocks, the same quantity
//! [`crate::linalg::SolveDiagnostics`] reports for a full LU, and a ratio
//! above the caller's limit (or a zero or non-finite pivot) makes
//! [`BlockFactorization::build`] return `None` so the caller falls back to
//! the full LU and its own gate. Matrices are row-major `n x n` slices.

use std::sync::Arc;

use faer::linalg::solvers::{PartialPivLu, Solve};
use faer::Mat;

use crate::linalg::{sequential_kernels, SolveDiagnostics};

/// `A_UU`'s LU for a frozen index set.
#[derive(Debug)]
pub struct BlockBase {
    n: usize,
    /// Indices of the frozen unknowns, ascending.
    frozen: Vec<usize>,
    /// Indices of the remaining unknowns, ascending.
    moving: Vec<usize>,
    lu: PartialPivLu<f64>,
    max_pivot: f64,
    min_pivot: f64,
}

/// The pivots' max and min absolute value, or `None` for a zero or
/// non-finite pivot.
fn pivot_range(lu: &PartialPivLu<f64>, n: usize) -> Option<(f64, f64)> {
    let u = lu.U();
    let (mut max, mut min) = (0.0_f64, f64::INFINITY);
    for i in 0..n {
        let pivot = u[(i, i)].abs();
        if !(pivot > 0.0 && pivot.is_finite()) {
            return None;
        }
        max = max.max(pivot);
        min = min.min(pivot);
    }
    Some((max, min))
}

impl BlockBase {
    /// Factor the `frozen` block of `aic` (row-major, `n x n`); `moving`
    /// must be the ascending complement. `None` if the block is empty,
    /// singular or non-finite, or `moving` is empty.
    pub fn build(aic: &[f64], n: usize, frozen: Vec<usize>, moving: Vec<usize>) -> Option<Self> {
        let m = frozen.len();
        if m == 0 || moving.is_empty() || aic.len() != n * n || m + moving.len() != n {
            return None;
        }
        sequential_kernels();
        let block = Mat::from_fn(m, m, |i, j| aic[frozen[i] * n + frozen[j]]);
        let lu = PartialPivLu::new(block.as_ref());
        let (max_pivot, min_pivot) = pivot_range(&lu, m)?;
        Some(Self {
            n,
            frozen,
            moving,
            lu,
            max_pivot,
            min_pivot,
        })
    }

    /// The frozen indices, ascending.
    pub fn frozen(&self) -> &[usize] {
        &self.frozen
    }

    /// Number of moving unknowns.
    pub fn moving_count(&self) -> usize {
        self.moving.len()
    }
}

/// Block factorization of one assembled matrix against a [`BlockBase`].
#[derive(Debug)]
pub struct BlockFactorization {
    base: Arc<BlockBase>,
    /// The whole matrix, row-major, for the residual evidence.
    aic: Arc<Vec<f64>>,
    /// `A_UU^-1 A_UT`, `m x k`.
    w: Mat<f64>,
    /// `A_TU`, `k x m`.
    a_tu: Mat<f64>,
    schur_lu: PartialPivLu<f64>,
    pivot_ratio: f64,
    minimum_pivot: f64,
    norm_inf: f64,
}

impl BlockFactorization {
    /// Factor `aic`, whose frozen block must be the base's. `None` when the
    /// Schur block has a bad pivot or the pivot ratio is not finite and at
    /// most `max_pivot_ratio`.
    pub fn build(base: Arc<BlockBase>, aic: Arc<Vec<f64>>, max_pivot_ratio: f64) -> Option<Self> {
        let n = base.n;
        if aic.len() != n * n {
            return None;
        }
        sequential_kernels();
        let (m, k) = (base.frozen.len(), base.moving.len());
        let (frozen, moving) = (&base.frozen, &base.moving);
        let mut w = Mat::from_fn(m, k, |i, j| aic[frozen[i] * n + moving[j]]);
        base.lu.solve_in_place(w.as_mut());
        let a_tu = Mat::from_fn(k, m, |i, j| aic[moving[i] * n + frozen[j]]);
        let product = &a_tu * &w;
        let schur = Mat::from_fn(k, k, |i, j| {
            aic[moving[i] * n + moving[j]] - product[(i, j)]
        });
        let schur_lu = PartialPivLu::new(schur.as_ref());
        let (max_s, min_s) = pivot_range(&schur_lu, k)?;
        let minimum_pivot = base.min_pivot.min(min_s);
        let pivot_ratio = base.max_pivot.max(max_s) / minimum_pivot;
        if !(pivot_ratio.is_finite() && pivot_ratio <= max_pivot_ratio) {
            return None;
        }
        let norm_inf = aic
            .chunks_exact(n)
            .map(|row| row.iter().map(|value| value.abs()).sum::<f64>())
            .fold(0.0_f64, f64::max);
        Some(Self {
            base,
            aic,
            w,
            a_tu,
            schur_lu,
            pivot_ratio,
            minimum_pivot,
            norm_inf,
        })
    }

    /// Solve `A x = b` with the residual evidence of a full LU solve.
    pub fn solve_vector(&self, b: &[f64]) -> (Vec<f64>, SolveDiagnostics) {
        let base = &self.base;
        let n = base.n;
        let (m, k) = (base.frozen.len(), base.moving.len());
        let mut y = Mat::from_fn(m, 1, |i, _| b[base.frozen[i]]);
        base.lu.solve_in_place(y.as_mut());
        let reduced = &self.a_tu * &y;
        let mut x_t = Mat::from_fn(k, 1, |i, _| b[base.moving[i]] - reduced[(i, 0)]);
        self.schur_lu.solve_in_place(x_t.as_mut());
        let correction = &self.w * &x_t;
        let mut x = vec![0.0_f64; n];
        for (i, &index) in base.frozen.iter().enumerate() {
            x[index] = y[(i, 0)] - correction[(i, 0)];
        }
        for (i, &index) in base.moving.iter().enumerate() {
            x[index] = x_t[(i, 0)];
        }
        let diagnostics = self.diagnostics(&x, b);
        (x, diagnostics)
    }

    /// The residual evidence against the matrix as assembled, summed in
    /// ascending column order like [`crate::linalg::LuFactorization`].
    fn diagnostics(&self, x: &[f64], b: &[f64]) -> SolveDiagnostics {
        let n = self.base.n;
        let max_b = b.iter().fold(0.0_f64, |acc, value| acc.max(value.abs()));
        let max_x = x.iter().fold(0.0_f64, |acc, value| acc.max(value.abs()));
        // Four rows advance together to overlap the add latency of their
        // running sums; each row still sums its columns in ascending order.
        let mut residual_norm = 0.0_f64;
        for (rows, rhs) in self.aic.chunks(4 * n).zip(b.chunks(4)) {
            let mut predicted = [0.0_f64; 4];
            if rhs.len() == 4 {
                let (r0, rest) = rows.split_at(n);
                let (r1, rest) = rest.split_at(n);
                let (r2, r3) = rest.split_at(n);
                for (j, &xj) in x.iter().enumerate() {
                    predicted[0] += r0[j] * xj;
                    predicted[1] += r1[j] * xj;
                    predicted[2] += r2[j] * xj;
                    predicted[3] += r3[j] * xj;
                }
            } else {
                for (row, slot) in rows.chunks(n).zip(&mut predicted) {
                    *slot = row.iter().zip(x).fold(0.0_f64, |acc, (a, x)| acc + a * x);
                }
            }
            for (value, &target) in predicted.iter().zip(rhs) {
                residual_norm = residual_norm.max((value - target).abs());
            }
        }
        let scale = (self.norm_inf * max_x).max(max_b).max(1.0);
        SolveDiagnostics {
            residual_norm,
            normalized_residual: residual_norm / scale,
            pivot_ratio: self.pivot_ratio,
            minimum_pivot: self.minimum_pivot,
        }
    }
}

// Tests construct valid systems; a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::DenseMatrix;

    /// A deterministic diagonally dominant matrix with dense coupling.
    fn matrix(n: usize, seed: f64) -> Vec<f64> {
        (0..n * n)
            .map(|index| {
                let (i, j) = (index / n, index % n);
                let coupling = ((i as f64 + 1.0) * (j as f64 + 2.0) * seed).sin() / n as f64;
                if i == j {
                    2.0 + coupling
                } else {
                    coupling
                }
            })
            .collect()
    }

    #[test]
    fn block_solve_matches_full_lu_to_rounding() {
        let n = 60;
        let moving: Vec<usize> = (40..60).collect();
        let frozen: Vec<usize> = (0..40).collect();
        let base_matrix = matrix(n, 0.7);
        let base = Arc::new(BlockBase::build(&base_matrix, n, frozen.clone(), moving).unwrap());
        // Replace the moving rows and columns.
        let mut updated = base_matrix;
        let other = matrix(n, 1.3);
        for i in 0..n {
            for j in 0..n {
                if i >= 40 || j >= 40 {
                    updated[i * n + j] = other[i * n + j];
                }
            }
        }
        let block = BlockFactorization::build(base, Arc::new(updated.clone()), 1.0e4).unwrap();
        let full = DenseMatrix::from_row_major(n, &updated).factor().unwrap();
        let rhs: Vec<f64> = (0..n).map(|i| (i as f64).cos()).collect();
        let (x_block, diagnostics) = block.solve_vector(&rhs);
        let (x_full, full_diagnostics) = full.solve_vector(&rhs);
        for (a, b) in x_block.iter().zip(&x_full) {
            assert!((a - b).abs() <= 1.0e-12 * (1.0 + b.abs()), "{a} vs {b}");
        }
        assert!(diagnostics.normalized_residual < 1.0e-13);
        assert!((diagnostics.pivot_ratio / full_diagnostics.pivot_ratio - 1.0).abs() < 0.2);
    }

    #[test]
    fn singular_moving_block_is_refused() {
        let n = 6;
        let base_matrix = matrix(n, 0.7);
        let base =
            Arc::new(BlockBase::build(&base_matrix, n, vec![0, 1, 2], vec![3, 4, 5]).unwrap());
        let mut updated = base_matrix;
        for i in 0..n {
            for j in 0..n {
                if i >= 3 || j >= 3 {
                    updated[i * n + j] = 0.0;
                }
            }
        }
        assert!(BlockFactorization::build(base, Arc::new(updated), 1.0e4).is_none());
    }
}
