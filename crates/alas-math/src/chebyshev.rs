// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Utilities/Chebyshev/chebyshev_data.py
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The Chebyshev pseudospectral differentiation and integration operators.
//!
//! A mission segment is a boundary-value problem: a differential equation in
//! time, subject to conditions at both ends. A pseudospectral method turns
//! that into an algebraic system a solver can converge on, by representing
//! the unknown as its values at a fixed set of nodes and replacing the
//! derivative operator with a dense matrix `D` such that `D @ f` approximates
//! `df/dx` at every node at once. `I`, built from the same nodes, does the
//! same for the running integral. This is what mission analysis model's mission segment
//! solver discretizes with; the solver itself is a separate, later module
//! (`alas-mission::numerics`).
//!
//! The construction is the standard barycentric-weight one for Chebyshev
//! points of the second kind: see Trefethen, *Spectral Methods in MATLAB*
//! (SIAM, 2000), chapter 6, `cheb.m`. mission analysis model's version differs from that
//! reference in one respect worth naming: it spaces the nodes over `[0, 1]`
//! rather than `[-1, 1]`, which rescales `D` by a factor of two and leaves
//! its structure otherwise unchanged.

/// Nodes and pseudospectral operators returned by [`chebyshev_data`].
///
/// `differentiation` and `integration` are dense and not symmetric.
/// `differentiation[i]` is row `i`: `differentiation[i][j]` is the weight
/// applied to node `j` when approximating the derivative at node `i`, so a
/// derivative sampled at every node is each row dotted with the sampled
/// function. `integration` reads the same way for the running integral.
#[derive(Debug, Clone, PartialEq)]
pub struct ChebyshevData {
    /// `N` cosine-spaced nodes in `[0, 1]`, ascending, with `x[0] == 0` and
    /// `x[N - 1] == 1`.
    pub x: Vec<f64>,
    /// The `N x N` differentiation operator.
    pub differentiation: Vec<Vec<f64>>,
    /// The `N x N` integration operator, or `None` when not requested.
    pub integration: Option<Vec<Vec<f64>>>,
}

/// What can go wrong building the operators.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ChebyshevError {
    /// `N` was zero or negative. mission analysis model raises a `RuntimeError` with the same
    /// message for the same input; this is that same guard, as a `Result`
    /// rather than an exception.
    #[error("N = {0}, must be > 0")]
    NonPositiveN(i64),
    /// The `(N - 1) x (N - 1)` submatrix of `differentiation` could not be
    /// inverted. Not expected to occur for the construction above -- it is
    /// the classical, well-conditioned Chebyshev quadrature matrix -- but
    /// library code does not panic on a numerical surprise, so a caller sees
    /// this instead.
    #[error(
        "the (N-1)x(N-1) submatrix of D was numerically singular; \
         cannot build the integration operator"
    )]
    SingularIntegrationOperator,
}

/// Build the Chebyshev pseudospectral nodes and operators for `n` points.
///
/// `differentiation` approximates `d/dx`; `integration`, when requested,
/// approximates `int_0^x`. Both act on values sampled at `x`: `D @ f` is the
/// derivative of `f` at every node, `I @ f` the running integral.
///
/// # Errors
///
/// Returns [`ChebyshevError::NonPositiveN`] when `n <= 0`, matching mission analysis model's
/// guard. Returns [`ChebyshevError::SingularIntegrationOperator`] if the
/// dense inverse the integration operator needs could not be computed.
pub fn chebyshev_data(n: i64, integration: bool) -> Result<ChebyshevData, ChebyshevError> {
    if n <= 0 {
        return Err(ChebyshevError::NonPositiveN(n));
    }
    // Non-negative and checked above; safe to treat as a point count.
    let n = n as usize;

    let x = cosine_spaced_points(n);
    let differentiation = differentiation_matrix(&x);
    let integration = if integration {
        Some(integration_matrix(&differentiation)?)
    } else {
        None
    };

    Ok(ChebyshevData {
        x,
        differentiation,
        integration,
    })
}

/// Cosine-spaced (Chebyshev-Gauss-Lobatto) nodes in `[0, 1]`.
fn cosine_spaced_points(n: usize) -> Vec<f64> {
    let last = (n - 1) as f64;
    (0..n)
        .map(|k| 0.5 * (1.0 - (std::f64::consts::PI * k as f64 / last).cos()))
        .collect()
}

/// The dense differentiation matrix for nodes `x`.
///
/// Follows the source's exact sequence of operations -- weights, a pairwise
/// distance matrix with its diagonal forced away from zero, an
/// elementwise divide, then a row-sum correction of the diagonal -- rather
/// than a shorter but differently-ordered equivalent, so the two accumulate
/// rounding error the same way.
fn differentiation_matrix(x: &[f64]) -> Vec<Vec<f64>> {
    let n = x.len();

    // Barycentric weights: 2 at the endpoints, 1 elsewhere, alternating sign.
    let c: Vec<f64> = (0..n)
        .map(|k| {
            let magnitude = if k == 0 || k == n - 1 { 2.0 } else { 1.0 };
            if k % 2 == 0 {
                magnitude
            } else {
                -magnitude
            }
        })
        .collect();
    let c_inv: Vec<f64> = c.iter().map(|&ck| 1.0 / ck).collect();

    let mut d = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            // x[i] - x[j] is exactly zero on the diagonal, so forcing it to
            // one there (the source's `+ eye(N)`) rather than computing the
            // subtraction changes nothing but the divide-by-zero it avoids.
            let da = if i == j { 1.0 } else { x[i] - x[j] };
            d[i][j] = (c[i] * c_inv[j]) / da;
        }
    }

    // Force each row to sum to zero, so differentiating a constant vector
    // gives zero: subtract the row's own sum from its diagonal entry only.
    for (i, row) in d.iter_mut().enumerate() {
        let row_sum: f64 = row.iter().sum();
        row[i] -= row_sum;
    }
    d
}

/// The integration operator: the inverse of `differentiation` with its first
/// row and column dropped, then re-embedded with a zero border.
///
/// Dropping the first row and column pins the integral's constant of
/// integration to zero at `x[0]`, which is what makes the remaining
/// `(N-1) x (N-1)` block invertible -- `differentiation` itself is singular,
/// since differentiating any constant gives zero.
fn integration_matrix(differentiation: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, ChebyshevError> {
    let n = differentiation.len();
    let m = n - 1;

    let mut sub = vec![vec![0.0; m]; m];
    for (i, row) in sub.iter_mut().enumerate() {
        row.copy_from_slice(&differentiation[i + 1][1..]);
    }

    let inverse = invert(&sub)?;

    let mut integration = vec![vec![0.0; n]; n];
    for i in 0..m {
        integration[i + 1][1..].copy_from_slice(&inverse[i]);
    }
    Ok(integration)
}

/// Inverts a square matrix by Gauss-Jordan elimination with partial pivoting.
///
/// `m` is small -- the caller uses it for `N - 1` up to 15 -- so a hand-rolled
/// dense inverse is in scope; pulling in a linear-algebra crate for one-off
/// inverses this size would be a larger dependency than the problem needs.
fn invert(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, ChebyshevError> {
    let m = matrix.len();
    if m == 0 {
        return Ok(Vec::new());
    }

    // Augment with the identity; row-reducing the left half to identity
    // turns the right half into the inverse.
    let mut aug = vec![vec![0.0; 2 * m]; m];
    for (i, row) in aug.iter_mut().enumerate() {
        row[..m].copy_from_slice(&matrix[i]);
        row[m + i] = 1.0;
    }

    for col in 0..m {
        let pivot_row = (col..m)
            .max_by(|&a, &b| aug[a][col].abs().total_cmp(&aug[b][col].abs()))
            .ok_or(ChebyshevError::SingularIntegrationOperator)?;
        if aug[pivot_row][col].abs() < f64::EPSILON {
            return Err(ChebyshevError::SingularIntegrationOperator);
        }
        aug.swap(col, pivot_row);

        let pivot = aug[col][col];
        for value in &mut aug[col] {
            *value /= pivot;
        }

        // Cloned once per column rather than borrowed, since every other row
        // in this elimination step is subtracted against it and Rust cannot
        // hold that row and a mutable one from the same `Vec` at once.
        let pivot_row = aug[col].clone();
        for (row, target) in aug.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = target[col];
            if factor == 0.0 {
                continue;
            }
            for (value, &pivot_value) in target.iter_mut().zip(&pivot_row) {
                *value -= factor * pivot_value;
            }
        }
    }

    Ok(aug.into_iter().map(|row| row[m..].to_vec()).collect())
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    // Property tests assert what parity cannot see, because a fixture only
    // covers the N values it was generated at.
    #[test]
    fn differentiating_a_constant_is_zero_for_every_n() {
        for n in [1, 2, 3, 4, 5, 8, 16, 24] {
            let data = chebyshev_data(n, false).expect("positive n");
            for row in &data.differentiation {
                let row_sum: f64 = row.iter().sum();
                assert!(
                    row_sum.abs() < 1e-9,
                    "N={n}: row sum {row_sum} should be ~0"
                );
            }
        }
    }

    #[test]
    fn x_is_ascending_and_spans_zero_to_one() {
        for n in [2, 3, 4, 8, 16] {
            let data = chebyshev_data(n, false).expect("positive n");
            assert_eq!(data.x[0], 0.0);
            assert_eq!(*data.x.last().unwrap(), 1.0);
            assert!(data.x.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    #[test]
    fn nonpositive_n_is_an_error_not_a_panic() {
        assert_eq!(
            chebyshev_data(0, true),
            Err(ChebyshevError::NonPositiveN(0))
        );
        assert_eq!(
            chebyshev_data(-3, true),
            Err(ChebyshevError::NonPositiveN(-3))
        );
    }

    #[test]
    fn integration_false_yields_no_integration_matrix() {
        let data = chebyshev_data(8, false).expect("positive n");
        assert!(data.integration.is_none());
    }

    #[test]
    fn integration_true_yields_a_matrix_with_a_zero_border() {
        let data = chebyshev_data(6, true).expect("positive n");
        let integration = data.integration.expect("integration was requested");
        assert!(integration[0].iter().all(|&v| v == 0.0));
        assert!(integration.iter().all(|row| row[0] == 0.0));
    }

    // A matrix product indexes both operands by the summation index and one
    // more of its own, which `enumerate()` over either matrix cannot express;
    // the explicit ranges are the clearer translation of `D @ I` here.
    #[allow(clippy::needless_range_loop)]
    #[test]
    fn integrating_then_differentiating_recovers_the_original_rows() {
        // I is built as the inverse of D's interior block, so D @ I should
        // act as identity on that block: a structural check parity does not
        // exercise directly.
        let data = chebyshev_data(10, true).expect("positive n");
        let d = &data.differentiation;
        let integration = data.integration.expect("integration was requested");
        let n = d.len();
        for i in 1..n {
            for j in 1..n {
                let mut sum = 0.0;
                for k in 0..n {
                    sum += d[i][k] * integration[k][j];
                }
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (sum - expected).abs() < 1e-8,
                    "(D @ I)[{i}][{j}] = {sum}, expected {expected}"
                );
            }
        }
    }
}
