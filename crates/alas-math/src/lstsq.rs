// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The overdetermined least-squares solve, with no upstream counterpart: the
//! primitive `numpy.linalg.lstsq` supplies to the modules that fit something.
//!
//! [`crate::linalg::solve`] answers a square system, which is what a spline's
//! collocation matrix and a vortex-lattice AIC matrix both are. A curve fit is
//! not square: `alas-aero::kulfan` expresses a few hundred airfoil vertices as
//! a linear combination of eighteen shape parameters and asks which eighteen
//! come closest, and there is no exact answer to that, only a residual to
//! minimize. That problem has its own numerics and its own failure mode, so it
//! lives beside the square solve rather than inside it.
//!
//! # Why Householder QR and not the normal equations
//!
//! `A^T A x = A^T b` is one line and is the wrong line. Forming `A^T A` squares
//! the condition number, so a fit whose matrix is conditioned at 1e3, which
//! is where the Kulfan fits actually sit: comes back with about 1e-10
//! relative error where the data supports 1e-13. That is outside `linalg`
//! (1e-9) once anything is squared twice. Householder QR reduces `A` to
//! triangular form by orthogonal reflections, which do not amplify the
//! condition number at all, so the error stays proportional to `cond(A)`
//! rather than to `cond(A)^2`.
//!
//! # Where this differs from `numpy.linalg.lstsq`, and why it does not matter
//!
//! NumPy calls LAPACK's `gelsd`, which is SVD-based: it computes the singular
//! values, discards those below `max(m, n) * eps * s_max` (what `rcond=None`
//! selects), and returns the minimum-norm solution over whatever subspace
//! remains. That truncation is the whole reason to prefer an SVD, and it is
//! also unreachable here: every fit this workspace performs is full rank with
//! a condition number near 1.1e3, six orders clear of the cutoff, and on such
//! a problem QR and the SVD agree to a few units in the last place.
//!
//! So this module detects rank deficiency and refuses it ([`LeastSquaresError::RankDeficient`])
//! rather than reproducing the minimum-norm branch of a routine that this
//! program never enters. Refusing is the honest answer: a caller that reached
//! it would be asking a question this implementation has not been shown to
//! answer the same way the reference does. It is a documented boundary, not a
//! `deviation-candidate`; whoever first needs a rank-deficient fit adds the
//! SVD and a fixture that exercises it.

/// Why a least-squares problem could not be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LeastSquaresError {
    /// Fewer equations than unknowns. An underdetermined system has a solution
    /// manifold rather than a solution, and picking a point on it (the
    /// minimum-norm one, as LAPACK does) is the branch this module declines.
    #[error("an underdetermined system: {rows} equations for {columns} unknowns")]
    Underdetermined {
        /// Equations supplied.
        rows: usize,
        /// Unknowns solved for.
        columns: usize,
    },
    /// The right-hand side does not have one entry per equation.
    #[error("{rows} equations but {values} right-hand-side values")]
    Mismatched {
        /// Equations supplied.
        rows: usize,
        /// Right-hand-side values supplied.
        values: usize,
    },
    /// Some rows are not all the same length, so `a` is not a matrix.
    #[error("row {row} has {columns} entries, but row 0 has {expected}")]
    Ragged {
        /// The row that disagreed.
        row: usize,
        /// Its length.
        columns: usize,
        /// The length row 0 established.
        expected: usize,
    },
    /// The columns of `a` are linearly dependent to working precision, so the
    /// minimizer is not unique. See this module's doc for why that is refused
    /// rather than resolved.
    #[error(
        "rank-deficient: the triangular factor's pivot {column} is negligible against its largest"
    )]
    RankDeficient {
        /// The column whose pivot vanished.
        column: usize,
    },
}

/// Minimize `||a * result - b||_2` over `result`, by Householder QR.
///
/// `a` is row-major and must have at least as many rows as columns; `b` has
/// one entry per row of `a`. The answer is unique exactly when `a` has full
/// column rank, which is checked rather than assumed.
///
/// # Errors
///
/// [`LeastSquaresError`] when the shapes do not describe an overdetermined
/// system, or when `a`'s columns turn out to be linearly dependent.
pub fn least_squares(a: &[Vec<f64>], b: &[f64]) -> Result<Vec<f64>, LeastSquaresError> {
    let rows = a.len();
    let columns = a.first().map_or(0, Vec::len);

    for (index, row) in a.iter().enumerate() {
        if row.len() != columns {
            return Err(LeastSquaresError::Ragged {
                row: index,
                columns: row.len(),
                expected: columns,
            });
        }
    }
    if b.len() != rows {
        return Err(LeastSquaresError::Mismatched {
            rows,
            values: b.len(),
        });
    }
    if rows < columns {
        return Err(LeastSquaresError::Underdetermined { rows, columns });
    }

    let mut matrix: Vec<Vec<f64>> = a.to_vec();
    let mut rhs: Vec<f64> = b.to_vec();

    // Reduce `matrix` to upper triangular form one column at a time. Each step
    // reflects the sub-column below the diagonal onto the first axis; the same
    // reflection is applied to `rhs`, so that afterwards `rhs[..columns]` holds
    // `(Q^T b)` restricted to the range of the reduced matrix and
    // `rhs[columns..]` holds the residual, which is discarded.
    for column in 0..columns {
        let norm = (column..rows)
            .map(|row| matrix[row][column] * matrix[row][column])
            .sum::<f64>()
            .sqrt();
        if norm == 0.0 {
            return Err(LeastSquaresError::RankDeficient { column });
        }

        // Reflect away from the pivot rather than toward it: choosing the sign
        // that makes `v[0]` grow avoids the cancellation that a pivot already
        // pointing along the axis would otherwise produce.
        let alpha = if matrix[column][column] > 0.0 {
            -norm
        } else {
            norm
        };

        let mut reflector = vec![0.0; rows - column];
        for (offset, value) in reflector.iter_mut().enumerate() {
            *value = matrix[column + offset][column];
        }
        // The sign chosen above makes this strictly nonzero (it is the pivot
        // moved away from zero by a nonzero norm, never toward it) so the
        // norm below cannot vanish and there is no degenerate case to guard.
        reflector[0] -= alpha;

        let reflector_norm_squared: f64 = reflector.iter().map(|v| v * v).sum();

        // Every remaining column's projection onto the reflector is read off
        // the matrix before any of them is written back. The two orders give
        // identical values (a column's projection depends only on itself)
        // but reading first lets the write walk whole rows.
        let projections: Vec<f64> = (column..columns)
            .map(|target| {
                reflector
                    .iter()
                    .enumerate()
                    .map(|(offset, &v)| v * matrix[column + offset][target])
                    .sum::<f64>()
                    * 2.0
                    / reflector_norm_squared
            })
            .collect();
        for (offset, &v) in reflector.iter().enumerate() {
            let row = &mut matrix[column + offset];
            for (target, &projection) in projections.iter().enumerate() {
                row[column + target] -= projection * v;
            }
        }

        let projection: f64 = reflector
            .iter()
            .enumerate()
            .map(|(offset, &v)| v * rhs[column + offset])
            .sum::<f64>()
            * 2.0
            / reflector_norm_squared;
        for (offset, &v) in reflector.iter().enumerate() {
            rhs[column + offset] -= projection * v;
        }
    }

    // A pivot small against the largest one means the columns are dependent to
    // working precision. The threshold mirrors what `rcond=None` asks LAPACK
    // for, applied to the triangular factor's diagonal rather than to singular
    // values: the two differ by a modest factor and this branch is unreached
    // on every problem in this workspace, which sit six orders clear of it.
    let largest = (0..columns).fold(0.0_f64, |best, k| best.max(matrix[k][k].abs()));
    let threshold = rows.max(columns) as f64 * f64::EPSILON * largest;
    if let Some(column) = (0..columns).find(|&k| matrix[k][k].abs() <= threshold) {
        return Err(LeastSquaresError::RankDeficient { column });
    }

    let mut result = vec![0.0; columns];
    for column in (0..columns).rev() {
        let mut value = rhs[column];
        for later in (column + 1)..columns {
            value -= matrix[column][later] * result[later];
        }
        result[column] = value / matrix[column][column];
    }

    Ok(result)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_nonsingular_system_is_solved_exactly() {
        // 2x + y = 5, x - y = 1 -> x = 2, y = 1. With as many equations as
        // unknowns the residual is zero and least squares is an exact solve.
        let a = vec![vec![2.0, 1.0], vec![1.0, -1.0]];
        let b = vec![5.0, 1.0];
        let solved = least_squares(&a, &b).expect("full rank");
        assert!((solved[0] - 2.0).abs() < 1e-12);
        assert!((solved[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_line_through_collinear_points_is_recovered() {
        // y = 3 + 2x sampled exactly at four abscissae: the fit must return
        // the line itself, with no residual to trade off.
        let xs = [0.0, 1.0, 2.0, 3.0];
        let a: Vec<Vec<f64>> = xs.iter().map(|&x| vec![1.0, x]).collect();
        let b: Vec<f64> = xs.iter().map(|&x| 3.0 + 2.0 * x).collect();
        let solved = least_squares(&a, &b).expect("full rank");
        assert!((solved[0] - 3.0).abs() < 1e-12);
        assert!((solved[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn an_inconsistent_fit_returns_the_hand_checkable_minimizer() {
        // Fitting a constant to 1, 2, 6 minimizes the sum of squares at the
        // mean, 3, which is the one answer that can be checked by inspection.
        let a = vec![vec![1.0], vec![1.0], vec![1.0]];
        let b = vec![1.0, 2.0, 6.0];
        let solved = least_squares(&a, &b).expect("full rank");
        assert!((solved[0] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn the_residual_is_orthogonal_to_every_column() {
        // The defining property of a least-squares minimizer: whatever is left
        // over cannot be reduced by moving along any direction the columns
        // span, so `A^T (A x - b)` is zero.
        let a = vec![
            vec![1.0, 0.5, 0.25],
            vec![1.0, 1.5, 2.25],
            vec![1.0, 2.5, 6.25],
            vec![1.0, 3.5, 12.25],
            vec![1.0, 4.5, 20.25],
        ];
        let b = vec![0.3, -1.2, 2.7, 4.1, 3.3];
        let solved = least_squares(&a, &b).expect("full rank");

        for column in 0..3 {
            let projected: f64 = a
                .iter()
                .zip(&b)
                .map(|(row, &target)| {
                    let modeled: f64 = row.iter().zip(&solved).map(|(&e, &x)| e * x).sum();
                    row[column] * (modeled - target)
                })
                .sum();
            assert!(projected.abs() < 1e-10, "column {column}: {projected:e}");
        }
    }

    #[test]
    fn a_column_repeated_is_rank_deficient_rather_than_a_wrong_answer() {
        let a = vec![
            vec![1.0, 2.0, 1.0],
            vec![2.0, 4.0, 3.0],
            vec![3.0, 6.0, 1.0],
            vec![4.0, 8.0, 5.0],
        ];
        let b = vec![1.0, 2.0, 3.0, 4.0];
        assert!(matches!(
            least_squares(&a, &b),
            Err(LeastSquaresError::RankDeficient { .. })
        ));
    }

    #[test]
    fn fewer_equations_than_unknowns_is_refused() {
        let a = vec![vec![1.0, 2.0, 3.0]];
        let b = vec![1.0];
        assert_eq!(
            least_squares(&a, &b),
            Err(LeastSquaresError::Underdetermined {
                rows: 1,
                columns: 3
            })
        );
    }

    #[test]
    fn a_right_hand_side_of_the_wrong_length_is_refused() {
        let a = vec![vec![1.0], vec![1.0]];
        let b = vec![1.0];
        assert_eq!(
            least_squares(&a, &b),
            Err(LeastSquaresError::Mismatched { rows: 2, values: 1 })
        );
    }

    #[test]
    fn a_ragged_matrix_names_the_row_that_disagreed() {
        let a = vec![vec![1.0, 2.0], vec![1.0]];
        let b = vec![1.0, 1.0];
        assert_eq!(
            least_squares(&a, &b),
            Err(LeastSquaresError::Ragged {
                row: 1,
                columns: 1,
                expected: 2
            })
        );
    }

    #[test]
    fn an_already_triangular_matrix_is_solved_without_disturbing_it() {
        // Nothing needs eliminating here, so this exercises the reflection on
        // a sub-column that is already along its axis: the case where a sign
        // choice made toward the pivot rather than away from it would cancel.
        let a = vec![vec![-2.0, 1.0], vec![0.0, 3.0], vec![0.0, 0.0]];
        let b = vec![-4.0, 6.0, 0.0];
        // 3y = 6 and -2x + y = -4, so y = 2 and x = 3, with no residual.
        let solved = least_squares(&a, &b).expect("full rank");
        assert!((solved[0] - 3.0).abs() < 1e-12);
        assert!((solved[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_badly_conditioned_fit_stays_far_more_accurate_than_the_normal_equations() {
        // A Vandermonde system on [0, 1] at degree 8: conditioned around 1e8,
        // so `A^T A` would be numerically singular while QR still recovers the
        // planted coefficients. This is the property that makes `linalg` (1e-9)
        // reachable for the Kulfan fits, which sit five orders better than
        // this.
        let planted = [0.5, -1.25, 2.0, 0.75, -3.5, 1.5, 0.25, -0.75, 1.125];
        let rows = 40;
        let mut a = Vec::with_capacity(rows);
        let mut b = Vec::with_capacity(rows);
        for index in 0..rows {
            let x = f64::from(u32::try_from(index).unwrap())
                / f64::from(u32::try_from(rows - 1).unwrap());
            let row: Vec<f64> = (0..planted.len())
                .map(|power| x.powi(i32::try_from(power).unwrap()))
                .collect();
            b.push(row.iter().zip(&planted).map(|(&e, &c)| e * c).sum());
            a.push(row);
        }
        let solved = least_squares(&a, &b).expect("full rank");
        for (index, (&got, &want)) in solved.iter().zip(&planted).enumerate() {
            assert!(
                (got - want).abs() < 1e-9,
                "coefficient {index}: got {got:e}, planted {want:e}"
            );
        }
    }
}
