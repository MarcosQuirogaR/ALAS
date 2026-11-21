// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Several right-hand sides solved at once with their residual vectors, for
//! a caller that combines the solutions linearly (the superposed VLM solves).

use faer::Mat;

use super::{max_or_nan, LuFactorization, SolveDiagnostics};

/// The solutions of a fixed set of right-hand sides `b_k` and their residual
/// vectors `A x_k - b_k`, for a caller that combines them linearly.
///
/// For `x = sum c_k x_k`, the solution of `b = sum c_k b_k` to rounding, the
/// residual `A x - b` is `sum c_k (A x_k - b_k)`, also to rounding, so
/// [`Self::diagnostics`] reports the same evidence a direct solve's
/// [`SolveDiagnostics`] would without another product with `A`.
#[derive(Debug, Clone)]
pub struct SolvedColumns {
    /// One solution per right-hand side, `n` entries each.
    pub solutions: Vec<Vec<f64>>,
    /// `A x_k - b_k` per right-hand side, against the matrix as given.
    pub residuals: Vec<Vec<f64>>,
    norm_inf: f64,
    pivot_ratio: f64,
    minimum_pivot: f64,
}

impl SolvedColumns {
    /// The diagnostics of the combined solution `x` of the right-hand side
    /// `b`, given its combined residual vector `residual`: the quantities and
    /// normalization of a direct solve's [`SolveDiagnostics`].
    pub fn diagnostics(&self, residual: &[f64], x: &[f64], b: &[f64]) -> SolveDiagnostics {
        let max_b = b.iter().fold(0.0_f64, |acc, value| acc.max(value.abs()));
        let max_x = x
            .iter()
            .fold(0.0_f64, |acc, value| max_or_nan(acc, value.abs()));
        let mut residual_norm = residual
            .iter()
            .fold(0.0_f64, |acc, value| max_or_nan(acc, value.abs()));
        if max_x.is_nan() {
            residual_norm = f64::NAN;
        }
        let scale = (self.norm_inf * max_x).max(max_b).max(1.0);
        SolveDiagnostics {
            residual_norm,
            normalized_residual: residual_norm / scale,
            pivot_ratio: self.pivot_ratio,
            minimum_pivot: self.minimum_pivot,
        }
    }

    pub(crate) fn new(
        solutions: Vec<Vec<f64>>,
        residuals: Vec<Vec<f64>>,
        (norm_inf, pivot_ratio, minimum_pivot): (f64, f64, f64),
    ) -> Self {
        Self {
            solutions,
            residuals,
            norm_inf,
            pivot_ratio,
            minimum_pivot,
        }
    }
}

impl LuFactorization {
    /// Solve `A x_k = b_k` for each of `columns`, `n` entries each, in one
    /// blocked substitution, and keep each residual vector: see
    /// [`SolvedColumns`].
    pub fn solve_with_residuals(&self, columns: &[Vec<f64>]) -> SolvedColumns {
        let n = self.dimension();
        let rhs = Mat::from_fn(n, columns.len(), |i, k| columns[k][i]);
        let solution = self.solve_matrix(&rhs);
        // One blocked product for every column (faer is pinned sequential,
        // so the sums do not depend on the thread count).
        let predicted = &self.a * &solution;
        let mut solutions = Vec::with_capacity(columns.len());
        let mut residuals = Vec::with_capacity(columns.len());
        for (k, b) in columns.iter().enumerate() {
            residuals.push(
                predicted
                    .col_as_slice(k)
                    .iter()
                    .zip(b)
                    .map(|(p, b)| p - b)
                    .collect(),
            );
            solutions.push(solution.col_as_slice(k).to_vec());
        }
        SolvedColumns::new(
            solutions,
            residuals,
            (self.norm_inf, self.pivot_ratio, self.minimum_pivot),
        )
    }
}
