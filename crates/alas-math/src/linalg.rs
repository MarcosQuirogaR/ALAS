// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dense linear algebra with no upstream counterpart: a general Gaussian
//! elimination solve, factored out once it gained a second caller.
//!
//! [`solve`] began inside `bspline.rs`, written against that module's own
//! collocation systems -- a few tens of rows, and banded, since a B-spline's
//! basis functions are locally supported. [`crate::BicubicSpline`] took it as
//! a second, in-crate caller without moving it, since both lived in the same
//! crate already. `alas-aero::vlm`'s AIC matrix is a third caller, and not
//! in this crate: dense rather than banded (an aircraft's induced-velocity
//! field couples every panel to every other one, so there is no locality to
//! exploit), and it can run to a few hundred rows for a spanwise/chordwise
//! mesh worth resolving. `alas-payload::numeric`'s module doc states the rule
//! this follows: private code that gains a second crate as a consumer moves
//! to `alas-math` rather than being copied.

/// Diagnostics emitted by a dense elimination solve.
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

/// Solve `a * result = b` and retain residual/conditioning evidence.
pub fn solve_with_diagnostics(
    a: &[Vec<f64>],
    b: &[Vec<f64>],
) -> Result<(Vec<Vec<f64>>, SolveDiagnostics), usize> {
    let n = a.len();
    let mut matrix: Vec<Vec<f64>> = a.to_vec();
    let mut rhs: Vec<Vec<f64>> = b.to_vec();
    let mut max_pivot = 0.0_f64;
    let mut min_pivot = f64::INFINITY;

    for column in 0..n {
        let pivot = (column..n)
            .max_by(|&i, &j| matrix[i][column].abs().total_cmp(&matrix[j][column].abs()))
            .unwrap_or(column);
        if matrix[pivot][column] == 0.0 {
            return Err(column);
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);

        let (eliminated, remaining) = matrix.split_at_mut(column + 1);
        let (rhs_eliminated, rhs_remaining) = rhs.split_at_mut(column + 1);
        let pivot_row = &eliminated[column];
        let pivot_rhs = &rhs_eliminated[column];
        let pivot_value = pivot_row[column];
        let abs_pivot = pivot_value.abs();
        max_pivot = max_pivot.max(abs_pivot);
        min_pivot = min_pivot.min(abs_pivot);

        for (row, rhs_row) in remaining.iter_mut().zip(rhs_remaining.iter_mut()) {
            let factor = row[column] / pivot_value;
            if factor == 0.0 {
                continue;
            }
            for (target, source) in row.iter_mut().zip(pivot_row).skip(column) {
                *target -= factor * source;
            }
            for (target, source) in rhs_row.iter_mut().zip(pivot_rhs) {
                *target -= factor * source;
            }
        }
    }

    for column in (0..n).rev() {
        let (solved_here, solved_later) = rhs.split_at_mut(column + 1);
        let row = &mut solved_here[column];
        for (offset, later) in solved_later.iter().enumerate() {
            let coefficient = matrix[column][column + 1 + offset];
            if coefficient == 0.0 {
                continue;
            }
            for (value, other) in row.iter_mut().zip(later) {
                *value -= coefficient * other;
            }
        }
        let pivot_value = matrix[column][column];
        for value in row.iter_mut() {
            *value /= pivot_value;
        }
    }

    let max_a = a
        .iter()
        .map(|row| row.iter().map(|value| value.abs()).sum::<f64>())
        .fold(0.0_f64, f64::max);
    let max_x = rhs
        .iter()
        .flat_map(|row| row.iter().copied())
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    let max_b = b
        .iter()
        .map(|row| row.iter().map(|value| value.abs()).sum::<f64>())
        .fold(0.0_f64, f64::max);
    let mut residual_norm = 0.0_f64;
    for (row, rhs_row) in a.iter().zip(b) {
        for column in 0..rhs_row.len() {
            let predicted: f64 = row
                .iter()
                .zip(&rhs)
                .map(|(coefficient, solution_row)| coefficient * solution_row[column])
                .sum();
            residual_norm = residual_norm.max((predicted - rhs_row[column]).abs());
        }
    }
    let scale = (max_a * max_x).max(max_b).max(1.0);
    let normalized_residual = residual_norm / scale;
    let (minimum_pivot, pivot_ratio) = if n == 0 {
        (0.0, 1.0)
    } else if min_pivot > 0.0 {
        (min_pivot, max_pivot / min_pivot)
    } else {
        (min_pivot, f64::INFINITY)
    };
    Ok((
        rhs,
        SolveDiagnostics {
            residual_norm,
            normalized_residual,
            pivot_ratio,
            minimum_pivot,
        },
    ))
}

/// Solve `a * result = b` for a square `a` and a many-columned `b`, by
/// Gaussian elimination with partial pivoting. The error is the elimination
/// step that found no usable pivot.
///
/// General dense elimination, with no assumption of bandedness or a
/// particular size -- the two conditions its first caller's collocation
/// systems happened to satisfy, but which nothing in this implementation
/// relies on.
pub fn solve(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, usize> {
    solve_with_diagnostics(a, b).map(|(solution, _)| solution)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

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
    fn diagnostics_report_a_small_residual_and_pivot_condition_proxy() {
        let a = vec![vec![2.0, 1.0], vec![1.0, -1.0]];
        let b = vec![vec![5.0], vec![1.0]];
        let (_, diagnostics) = solve_with_diagnostics(&a, &b).expect("nonsingular");
        assert!(diagnostics.normalized_residual < 1.0e-14);
        assert!(diagnostics.pivot_ratio.is_finite());
        assert!(diagnostics.minimum_pivot > 0.0);
    }
}
