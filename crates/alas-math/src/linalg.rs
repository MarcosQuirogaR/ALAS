// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dense linear algebra with no upstream counterpart: a general square solve
//! and the LU factorization behind it, factored out once it gained a second
//! caller.
//!
//! [`solve`] began inside `bspline.rs`, written against that module's own
//! collocation systems -- a few tens of rows, and banded, since a B-spline's
//! basis functions are locally supported. [`crate::BicubicSpline`] took it as
//! a second, in-crate caller without moving it, since both lived in the same
//! crate already. `alas-aero::vlm`'s AIC matrix is a third caller, and not
//! in this crate: dense rather than banded (an aircraft's induced-velocity
//! field couples every panel to every other one, so there is no locality to
//! exploit), and it runs to several hundred rows at the fine product mesh.
//! `alas-payload::numeric`'s module doc states the rule this follows: private
//! code that gains a second crate as a consumer moves to `alas-math` rather
//! than being copied.
//!
//! # Why the kernel is `faer`
//!
//! The first implementation here was a textbook partial-pivot elimination
//! over a vector of row vectors: correct, and adequate for the collocation
//! systems it was written for. The vortex-lattice AIC at the fine product
//! mesh is 800 by 800, and there that scalar loop measured 164 ms per solve at
//! about 2 Gflop/s on one core (2026-09-11), 90 % of every VLM call. NumPy's
//! `numpy.linalg.solve` hands the same matrix to LAPACK `dgesv`, which is
//! blocked and vectorized, so the port lost to the reference on the one stage
//! that dominates the full analysis. `faer` is a kernel of that class in pure
//! Rust: blocked LU with partial pivoting, SIMD inner kernels, and a rayon
//! pool for the trailing update. It picks the same pivots partial pivoting
//! always picks -- the largest magnitude in the column -- so results agree
//! with the previous elimination to rounding, well inside the `linalg`
//! parity tier.
//!
//! # Factor once, solve many
//!
//! [`LuFactorization`] exists because a caller that changes only the
//! right-hand side -- a polar sweep over angle of attack, where the influence
//! matrix depends on geometry alone -- should pay the O(n^3) factorization
//! once and the O(n^2) substitution per point. [`solve`] and
//! [`solve_with_diagnostics`] are the one-shot form over the same kernel.
//!
//! # Diagnostics
//!
//! The pivot statistics are read from the diagonal of `U`. Those are exactly
//! the pivots an elimination records as it goes, so [`SolveDiagnostics`]
//! keeps its meaning across the change of kernel. The residual is computed
//! against the original matrix, which the factorization keeps for that
//! purpose.
//!
//! # Why the factorization runs single-threaded
//!
//! faer's default is to spread the trailing update over a rayon pool. On a
//! 16-thread desktop that was measured slower than the sequential kernel at
//! every size this program factors (n = 200: 3.3 ms against 0.42 ms;
//! n = 800: 25 ms against 12 ms; 2026-09-11) -- the matrices are too small
//! for the fork/join to pay for itself, and callers already parallelize
//! above this level (the screening's candidates, the optimizers' batches).
//! The kernel is therefore pinned to [`faer::Par::Seq`] once, before the
//! first factorization.

use std::sync::OnceLock;

use faer::linalg::solvers::{PartialPivLu, Solve};
use faer::{Mat, Par};

/// Pin faer to its sequential kernels; see the module doc for the measurement.
fn sequential_kernels() {
    static PINNED: OnceLock<()> = OnceLock::new();
    PINNED.get_or_init(|| faer::set_global_parallelism(Par::Seq));
}

/// Diagnostics emitted by a dense solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveDiagnostics {
    /// Maximum absolute residual, `max_i |(A x - b)_i|`.
    pub residual_norm: f64,
    /// Residual normalized by `max(1, ||A||_infinity ||x||_infinity, ||b||_infinity)`.
    pub normalized_residual: f64,
    /// Pivot-ratio conditioning proxy (`max |pivot| / min |pivot|`).
    ///
    /// This is deliberately named a proxy: it is cheap enough for every VLM
    /// solve, while a full singular-value condition number belongs to an
    /// explicit mesh-study campaign.
    pub pivot_ratio: f64,
    /// Smallest absolute pivot encountered after partial pivoting.
    pub minimum_pivot: f64,
}

/// A dense square matrix in contiguous column-major storage, the input to
/// [`DenseMatrix::factor`].
///
/// Callers that assemble a large matrix should fill one of these (or a
/// row-major slice for [`DenseMatrix::from_row_major`]) rather than a vector
/// of row vectors: the factorization wants contiguous storage, and copying
/// out of nested vectors is an O(n^2) pass that a 800-row assembly does not
/// need to pay twice.
#[derive(Debug, Clone)]
pub struct DenseMatrix {
    inner: Mat<f64>,
}

impl DenseMatrix {
    /// The `n`-by-`n` zero matrix.
    pub fn zeros(n: usize) -> Self {
        Self {
            inner: Mat::zeros(n, n),
        }
    }

    /// A matrix from `rows`, each of which must hold one entry per row.
    pub fn from_rows(rows: &[Vec<f64>]) -> Self {
        let n = rows.len();
        Self {
            inner: Mat::from_fn(n, n, |i, j| rows[i][j]),
        }
    }

    /// A matrix from a row-major slice of `n * n` entries.
    pub fn from_row_major(n: usize, data: &[f64]) -> Self {
        Self {
            inner: Mat::from_fn(n, n, |i, j| data[i * n + j]),
        }
    }

    /// The number of rows (and columns).
    pub fn dimension(&self) -> usize {
        self.inner.nrows()
    }

    /// Read one entry.
    pub fn get(&self, row: usize, column: usize) -> f64 {
        self.inner[(row, column)]
    }

    /// Write one entry.
    pub fn set(&mut self, row: usize, column: usize, value: f64) {
        self.inner[(row, column)] = value;
    }

    /// Factor the matrix as `P A = L U` with partial pivoting.
    ///
    /// The error is the first diagonal position of `U` that is exactly zero:
    /// the elimination step that found no usable pivot, in the same terms
    /// the elimination this replaced reported.
    pub fn factor(self) -> Result<LuFactorization, usize> {
        let n = self.inner.nrows();
        if n == 0 {
            return Ok(LuFactorization {
                a: self.inner,
                lu: None,
                minimum_pivot: 0.0,
                pivot_ratio: 1.0,
            });
        }
        sequential_kernels();
        let lu = PartialPivLu::new(self.inner.as_ref());
        let mut max_pivot = 0.0_f64;
        let mut min_pivot = f64::INFINITY;
        {
            let u = lu.U();
            for i in 0..n {
                let pivot = u[(i, i)].abs();
                if pivot == 0.0 {
                    return Err(i);
                }
                max_pivot = max_pivot.max(pivot);
                min_pivot = min_pivot.min(pivot);
            }
        }
        let (minimum_pivot, pivot_ratio) = if min_pivot > 0.0 {
            (min_pivot, max_pivot / min_pivot)
        } else {
            (min_pivot, f64::INFINITY)
        };
        Ok(LuFactorization {
            a: self.inner,
            lu: Some(lu),
            minimum_pivot,
            pivot_ratio,
        })
    }
}

/// The LU factorization of a [`DenseMatrix`], ready to solve any number of
/// right-hand sides.
#[derive(Debug, Clone)]
pub struct LuFactorization {
    a: Mat<f64>,
    /// `None` only for the empty system, which has nothing to factor.
    lu: Option<PartialPivLu<f64>>,
    minimum_pivot: f64,
    pivot_ratio: f64,
}

impl LuFactorization {
    /// The number of unknowns.
    pub fn dimension(&self) -> usize {
        self.a.nrows()
    }

    /// Smallest absolute pivot of the factorization.
    pub fn minimum_pivot(&self) -> f64 {
        self.minimum_pivot
    }

    /// `max |pivot| / min |pivot|`, the conditioning proxy.
    pub fn pivot_ratio(&self) -> f64 {
        self.pivot_ratio
    }

    /// Solve `A x = b` for one right-hand side vector.
    pub fn solve_vector(&self, b: &[f64]) -> (Vec<f64>, SolveDiagnostics) {
        let n = self.dimension();
        let rhs = Mat::from_fn(n, 1, |i, _| b[i]);
        let solution = self.solve_matrix(&rhs);
        let diagnostics = self.diagnostics(&solution, &rhs);
        (solution.col_as_slice(0).to_vec(), diagnostics)
    }

    /// Solve `A X = B` for a many-columned `b`, given and returned as `n`
    /// rows of `k` entries -- the layout [`solve`] takes.
    pub fn solve_columns(&self, b: &[Vec<f64>]) -> (Vec<Vec<f64>>, SolveDiagnostics) {
        let n = self.dimension();
        let k = b.first().map_or(0, Vec::len);
        let rhs = Mat::from_fn(n, k, |i, j| b[i][j]);
        let solution = self.solve_matrix(&rhs);
        let diagnostics = self.diagnostics(&solution, &rhs);
        let rows = (0..n)
            .map(|i| (0..k).map(|j| solution[(i, j)]).collect())
            .collect();
        (rows, diagnostics)
    }

    fn solve_matrix(&self, rhs: &Mat<f64>) -> Mat<f64> {
        let mut solution = rhs.clone();
        if let Some(lu) = &self.lu {
            lu.solve_in_place(solution.as_mut());
        }
        solution
    }

    /// The residual evidence for one solve, against the matrix as given.
    fn diagnostics(&self, x: &Mat<f64>, b: &Mat<f64>) -> SolveDiagnostics {
        let n = self.dimension();
        let k = b.ncols();
        let mut max_a = 0.0_f64;
        let mut max_b = 0.0_f64;
        let mut max_x = 0.0_f64;
        let mut residual_norm = 0.0_f64;
        for i in 0..n {
            let row_a: f64 = (0..n).map(|j| self.a[(i, j)].abs()).sum();
            max_a = max_a.max(row_a);
            let row_b: f64 = (0..k).map(|j| b[(i, j)].abs()).sum();
            max_b = max_b.max(row_b);
            for column in 0..k {
                max_x = max_x.max(x[(i, column)].abs());
                let predicted: f64 = (0..n).map(|j| self.a[(i, j)] * x[(j, column)]).sum();
                residual_norm = residual_norm.max((predicted - b[(i, column)]).abs());
            }
        }
        let scale = (max_a * max_x).max(max_b).max(1.0);
        SolveDiagnostics {
            residual_norm,
            normalized_residual: residual_norm / scale,
            pivot_ratio: self.pivot_ratio,
            minimum_pivot: self.minimum_pivot,
        }
    }
}

/// Solve `a * result = b` and retain residual/conditioning evidence.
pub fn solve_with_diagnostics(
    a: &[Vec<f64>],
    b: &[Vec<f64>],
) -> Result<(Vec<Vec<f64>>, SolveDiagnostics), usize> {
    if a.is_empty() {
        return Ok((
            b.to_vec(),
            SolveDiagnostics {
                residual_norm: 0.0,
                normalized_residual: 0.0,
                pivot_ratio: 1.0,
                minimum_pivot: 0.0,
            },
        ));
    }
    let factorization = DenseMatrix::from_rows(a).factor()?;
    Ok(factorization.solve_columns(b))
}

/// Solve `a * result = b` for a square `a` and a many-columned `b`. The
/// error is the elimination step that found no usable pivot.
///
/// General dense LU, with no assumption of bandedness or a particular size --
/// the two conditions its first caller's collocation systems happened to
/// satisfy, but which nothing in this implementation relies on.
pub fn solve(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, usize> {
    solve_with_diagnostics(a, b).map(|(solution, _)| solution)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// The scalar partial-pivot elimination this module shipped before the
    /// kernel change, kept as an independent oracle for the new one.
    fn reference_elimination(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, usize> {
        let n = a.len();
        let mut matrix: Vec<Vec<f64>> = a.to_vec();
        let mut rhs: Vec<Vec<f64>> = b.to_vec();
        for column in 0..n {
            let pivot = (column..n)
                .max_by(|&i, &j| matrix[i][column].abs().total_cmp(&matrix[j][column].abs()))
                .unwrap_or(column);
            if matrix[pivot][column] == 0.0 {
                return Err(column);
            }
            matrix.swap(column, pivot);
            rhs.swap(column, pivot);
            let pivot_row = matrix[column].clone();
            let pivot_rhs = rhs[column].clone();
            let pivot_value = pivot_row[column];
            for row in column + 1..n {
                let factor = matrix[row][column] / pivot_value;
                if factor == 0.0 {
                    continue;
                }
                for j in column..n {
                    matrix[row][j] -= factor * pivot_row[j];
                }
                for j in 0..pivot_rhs.len() {
                    rhs[row][j] -= factor * pivot_rhs[j];
                }
            }
        }
        for column in (0..n).rev() {
            for later in column + 1..n {
                let coefficient = matrix[column][later];
                if coefficient == 0.0 {
                    continue;
                }
                let later_row = rhs[later].clone();
                for (value, other) in rhs[column].iter_mut().zip(&later_row) {
                    *value -= coefficient * other;
                }
            }
            let pivot_value = matrix[column][column];
            for value in rhs[column].iter_mut() {
                *value /= pivot_value;
            }
        }
        Ok(rhs)
    }

    fn xorshift(seed: &mut u64) -> f64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        (*seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    }

    fn random_system(n: usize, k: usize, seed: u64) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
        let mut seed = seed;
        let a = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        if i == j {
                            2.0 + n as f64 * 0.05
                        } else {
                            xorshift(&mut seed)
                        }
                    })
                    .collect()
            })
            .collect();
        let b = (0..n)
            .map(|_| (0..k).map(|_| xorshift(&mut seed)).collect())
            .collect();
        (a, b)
    }

    #[test]
    fn solving_with_the_identity_returns_the_right_hand_side_unchanged() {
        let identity = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        let rhs = vec![vec![2.0], vec![-3.0], vec![5.0]];
        let solved = solve(&identity, &rhs).expect("nonsingular");
        assert_eq!(solved, rhs);
    }

    #[test]
    fn a_hand_checkable_system_matches_its_known_solution() {
        // 2x + y = 5, x - y = 1 -> x = 2, y = 1.
        let a = vec![vec![2.0, 1.0], vec![1.0, -1.0]];
        let b = vec![vec![5.0], vec![1.0]];
        let solved = solve(&a, &b).expect("nonsingular");
        assert!((solved[0][0] - 2.0).abs() < 1e-12);
        assert!((solved[1][0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn multiple_right_hand_side_columns_are_solved_independently() {
        let a = vec![vec![2.0, 0.0], vec![0.0, 4.0]];
        let b = vec![vec![6.0, 2.0], vec![8.0, -4.0]];
        let solved = solve(&a, &b).expect("nonsingular");
        assert_eq!(solved, vec![vec![3.0, 1.0], vec![2.0, -1.0]]);
    }

    #[test]
    fn a_singular_matrix_is_an_error_naming_the_stuck_column() {
        let singular = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let b = vec![vec![1.0], vec![2.0]];
        assert_eq!(solve(&singular, &b), Err(1));
    }

    #[test]
    fn the_empty_system_solves_to_the_empty_right_hand_side() {
        let (solved, diagnostics) = solve_with_diagnostics(&[], &[]).expect("empty");
        assert!(solved.is_empty());
        assert_eq!(diagnostics.pivot_ratio, 1.0);
        assert_eq!(diagnostics.minimum_pivot, 0.0);
    }

    #[test]
    fn diagnostics_report_a_small_residual_and_pivot_condition_proxy() {
        let a = vec![vec![2.0, 1.0], vec![1.0, -1.0]];
        let b = vec![vec![5.0], vec![1.0]];
        let (_, diagnostics) = solve_with_diagnostics(&a, &b).expect("nonsingular");
        assert!(diagnostics.normalized_residual < 1.0e-14);
        assert!(diagnostics.pivot_ratio.is_finite());
        assert!(diagnostics.minimum_pivot > 0.0);
    }

    #[test]
    fn the_kernel_agrees_with_the_scalar_elimination_it_replaced() {
        for (n, seed) in [(7usize, 11u64), (60, 23), (257, 37)] {
            let (a, b) = random_system(n, 3, seed);
            let expected = reference_elimination(&a, &b).expect("well conditioned");
            let (actual, diagnostics) = solve_with_diagnostics(&a, &b).expect("well conditioned");
            for (row_actual, row_expected) in actual.iter().zip(&expected) {
                for (x, y) in row_actual.iter().zip(row_expected) {
                    assert!(
                        (x - y).abs() <= 1e-12 * (1.0 + y.abs()),
                        "n={n}: {x} differs from the elimination's {y}"
                    );
                }
            }
            assert!(
                diagnostics.normalized_residual < 1e-13,
                "n={n}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn one_factorization_serves_many_right_hand_sides() {
        // The many-column substitution runs through a blocked kernel and the
        // one-column path through a vector kernel, so the two agree to
        // rounding rather than bit for bit.
        let (a, b) = random_system(40, 5, 5);
        let factorization = DenseMatrix::from_rows(&a).factor().expect("nonsingular");
        let (all_at_once, _) = factorization.solve_columns(&b);
        for column in 0..5 {
            let rhs: Vec<f64> = b.iter().map(|row| row[column]).collect();
            let (one, diagnostics) = factorization.solve_vector(&rhs);
            for (i, value) in one.iter().enumerate() {
                let batched = all_at_once[i][column];
                assert!(
                    (value - batched).abs() <= 1e-13 * (1.0 + batched.abs()),
                    "column {column}, row {i}: {value} against {batched}"
                );
            }
            assert!(diagnostics.normalized_residual < 1e-14);
        }
    }

    #[test]
    fn row_major_and_nested_row_construction_factor_identically() {
        let (a, b) = random_system(12, 1, 99);
        let flat: Vec<f64> = a.iter().flatten().copied().collect();
        let from_rows = DenseMatrix::from_rows(&a).factor().expect("nonsingular");
        let from_flat = DenseMatrix::from_row_major(12, &flat)
            .factor()
            .expect("nonsingular");
        let rhs: Vec<f64> = b.iter().map(|row| row[0]).collect();
        assert_eq!(
            from_rows.solve_vector(&rhs).0,
            from_flat.solve_vector(&rhs).0
        );
        assert_eq!(from_rows.pivot_ratio(), from_flat.pivot_ratio());
    }

    #[test]
    fn a_dense_matrix_reads_back_what_was_written() {
        let mut matrix = DenseMatrix::zeros(3);
        matrix.set(1, 2, 4.5);
        assert_eq!(matrix.get(1, 2), 4.5);
        assert_eq!(matrix.get(2, 1), 0.0);
        assert_eq!(matrix.dimension(), 3);
    }
}
