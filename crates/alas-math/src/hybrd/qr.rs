// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from MINPACK-1's qrfac.f, qform.f, r1updt.f and r1mpyq.f.
// Upstream: MINPACK-1 (Argonne National Laboratory, 1980), public domain,
// as vendored in SciPy 1.11.4 and reached through scipy.optimize.fsolve.
// Reference: alas @ rust-port-baseline.

//! The QR factorization `hybrd` carries between iterations, and the rank-one
//! update that lets it skip rebuilding one.
//!
//! A Jacobian costs `n` residual evaluations, and a residual evaluation is a
//! whole mission-segment analysis chain -- atmosphere, propulsion, drag
//! buildup, weights. So `hybrd` builds the Jacobian rarely and carries `Q` and
//! `R` forward across steps, applying Broyden's rank-one correction to the
//! factors directly rather than refactoring. [`r1updt`] and [`r1mpyq`] are
//! that correction: a chain of Givens rotations that restores triangularity
//! after `R` has had a spike introduced into it.
//!
//! # Scope
//!
//! Every routine here is square (`m == n`), which is the only shape `hybrd`
//! calls them with -- it solves `n` equations in `n` unknowns and factors an
//! `n`-by-`n` Jacobian. [`qrfac`] is scoped to `pivot = .false.`, the value
//! `hybrd` passes, so the column-pivoting branch and the `ipvt` permutation it
//! maintains are not translated. The one exception is [`r1mpyq`], which
//! `hybrd` calls twice with different row counts -- once on the `n`-by-`n`
//! accumulated `Q` and once on the single row `qtf` -- so that one keeps its
//! row count as a parameter.
//!
//! `R` is stored packed by rows, upper triangle only: `r[0]` is `R(1,1)`,
//! `r[n-1]` is `R(1,n)`, `r[n]` is `R(2,2)`, and so on, for `n * (n + 1) / 2`
//! entries. Square matrices are stored column-major, matching the Fortran, so
//! that `a(i, j)` is `a[i + j * n]` and a column is a contiguous slice.

use super::enorm::enorm;

/// Factor the square `a` into `Q * R` by Householder reflections, without
/// column pivoting.
///
/// `a` is overwritten: its strict lower triangle and diagonal carry the
/// Householder vectors that [`qform`] expands into `Q`, and `rdiag` receives
/// the diagonal of `R`, whose strict upper triangle is left in `a`. `acnorm`
/// receives the original column norms, which `hybrd` uses to set its scaling
/// vector on the first iteration.
pub(super) fn qrfac(n: usize, a: &mut [f64], rdiag: &mut [f64], acnorm: &mut [f64]) {
    for j in 0..n {
        let norm = enorm(&a[j * n..j * n + n]);
        acnorm[j] = norm;
        rdiag[j] = norm;
    }

    for j in 0..n {
        // The Householder transformation reducing column `j` below the
        // diagonal to a multiple of the `j`-th unit vector.
        let mut ajnorm = enorm(&a[j * n + j..j * n + n]);
        if ajnorm != 0.0 {
            if a[j * n + j] < 0.0 {
                ajnorm = -ajnorm;
            }
            for i in j..n {
                a[j * n + i] /= ajnorm;
            }
            a[j * n + j] += 1.0;

            // Apply it to the remaining columns.
            for k in (j + 1)..n {
                let mut sum = 0.0;
                for i in j..n {
                    sum += a[j * n + i] * a[k * n + i];
                }
                let temp = sum / a[j * n + j];
                for i in j..n {
                    a[k * n + i] -= temp * a[j * n + i];
                }
            }
        }
        rdiag[j] = -ajnorm;
    }
}

/// Expand the Householder vectors [`qrfac`] left in `q` into the orthogonal
/// factor itself, in place.
pub(super) fn qform(n: usize, q: &mut [f64]) {
    for j in 1..n {
        for i in 0..j {
            q[j * n + i] = 0.0;
        }
    }
    // The Fortran also initializes columns `n + 1 .. m` to those of the
    // identity; with `m == n` there are none, so that loop has no counterpart.

    let mut wa = vec![0.0_f64; n];
    for l in 0..n {
        let k = n - 1 - l;
        for i in k..n {
            wa[i] = q[k * n + i];
            q[k * n + i] = 0.0;
        }
        q[k * n + k] = 1.0;
        if wa[k] != 0.0 {
            for j in k..n {
                let mut sum = 0.0;
                for i in k..n {
                    sum += q[j * n + i] * wa[i];
                }
                let temp = sum / wa[k];
                for i in k..n {
                    q[j * n + i] -= temp * wa[i];
                }
            }
        }
    }
}

/// One Givens rotation, in the form MINPACK stores it: `tau` is the single
/// number from which `r1mpyq` later recovers both `cos` and `sin`.
struct Givens {
    cos: f64,
    sin: f64,
    tau: f64,
}

/// The rotation eliminating `b` against `a`, chosen so that the larger of the
/// two is the one divided by. `tau` encodes the rotation as a single number:
/// a magnitude at most one is the sine, and anything larger is the reciprocal
/// of the cosine, which is how [`r1mpyq`] tells the two cases apart.
fn givens(a: f64, b: f64) -> Givens {
    if a.abs() >= b.abs() {
        let tang = b / a;
        let cos = 0.5 / (0.25 + 0.25 * tang * tang).sqrt();
        let sin = cos * tang;
        Givens { cos, sin, tau: sin }
    } else {
        let cotan = a / b;
        let sin = 0.5 / (0.25 + 0.25 * cotan * cotan).sqrt();
        let cos = sin * cotan;
        // Guard against a reciprocal that would overflow; the Fortran leaves
        // `tau` at one in that case, which `r1mpyq` reads as a sine.
        let tau = if cos.abs() * f64::MAX > 1.0 {
            1.0 / cos
        } else {
            1.0
        };
        Givens { cos, sin, tau }
    }
}

/// Update the packed triangular `s` for a rank-one change, leaving it
/// triangular again.
///
/// **The outer product is `v * u^T`, not `u * v^T`, and the transposition is
/// not a slip.** The Fortran documents `s` as *lower trapezoidal stored by
/// columns* and produces an orthogonal `q` making `(s + u v^T) q` lower
/// trapezoidal again. For a square matrix the lower triangle stored by
/// columns and the upper triangle stored by rows are the same linear layout,
/// so the `s` this routine walks is `R^T` -- and transposing its stated
/// contract gives `q^T (R + v u^T)` upper triangular. `hybrd` relies on
/// exactly that reading: it passes the scaled step as `u` and the residual
/// mismatch as `v`, which is Broyden's correction `R + (Q^T \delta) p^T` only
/// under this order. Swapping the two arguments still produces a triangular
/// matrix, so nothing panics and no test of triangularity notices; what comes
/// out is the factor of a different matrix.
///
/// `v` and `w` come back holding the two chains of Givens rotations that
/// [`r1mpyq`] applies to `Q` so the factorization stays consistent. Returns
/// whether the updated `s` has a zero on its diagonal; `hybrd` does not read
/// that flag, but it is what the Fortran reports and dropping it would make
/// the signature a different function.
pub(super) fn r1updt(n: usize, s: &mut [f64], u: &[f64], v: &mut [f64], w: &mut [f64]) -> bool {
    // One-based, to keep the packed-triangle arithmetic readable against the
    // Fortran; every access subtracts one.
    let mut jj = (n * (n + 1)) / 2;

    // Move the nontrivial part of the last column of `s` into `w`. With
    // `m == n` that is the single diagonal entry.
    w[n - 1] = s[jj - 1];

    // Rotate `v` into a multiple of the n-th unit vector, introducing a spike
    // into `w` as it goes.
    for nmj in 1..n {
        let j = n - nmj;
        jj -= n - j + 1;
        w[j - 1] = 0.0;
        if v[j - 1] != 0.0 {
            let g = givens(v[n - 1], v[j - 1]);
            v[n - 1] = g.sin * v[j - 1] + g.cos * v[n - 1];
            v[j - 1] = g.tau;

            let mut l = jj;
            for i in j..=n {
                let temp = g.cos * s[l - 1] - g.sin * w[i - 1];
                w[i - 1] = g.sin * s[l - 1] + g.cos * w[i - 1];
                s[l - 1] = temp;
                l += 1;
            }
        }
    }

    // Add the spike from the rank-one update to `w`.
    for i in 0..n {
        w[i] += v[n - 1] * u[i];
    }

    // Eliminate the spike.
    let mut singular = false;
    for j in 1..n {
        if w[j - 1] != 0.0 {
            let g = givens(s[jj - 1], w[j - 1]);
            let mut l = jj;
            for i in j..=n {
                let temp = g.cos * s[l - 1] + g.sin * w[i - 1];
                w[i - 1] = -g.sin * s[l - 1] + g.cos * w[i - 1];
                s[l - 1] = temp;
                l += 1;
            }
            w[j - 1] = g.tau;
        }
        if s[jj - 1] == 0.0 {
            singular = true;
        }
        jj += n - j + 1;
    }

    // Move `w` back into the last column of `s`.
    s[jj - 1] = w[n - 1];
    if s[jj - 1] == 0.0 {
        singular = true;
    }
    singular
}

/// Apply the two chains of Givens rotations [`r1updt`] produced to the `rows`
/// -by-`n` column-major matrix `a`, in place.
///
/// `hybrd` calls this on the accumulated `Q` and again on the single row
/// `qtf`, which is why the row count is a parameter here and nowhere else in
/// this module.
pub(super) fn r1mpyq(rows: usize, n: usize, a: &mut [f64], v: &[f64], w: &[f64]) {
    /// Recover a rotation from the single number [`r1updt`] stored it as.
    fn recover(encoded: f64) -> (f64, f64) {
        if encoded.abs() > 1.0 {
            let cos = 1.0 / encoded;
            (cos, (1.0 - cos * cos).sqrt())
        } else {
            let sin = encoded;
            ((1.0 - sin * sin).sqrt(), sin)
        }
    }

    for nmj in 1..n {
        let j = n - nmj;
        let (cos, sin) = recover(v[j - 1]);
        for i in 0..rows {
            let temp = cos * a[i + (j - 1) * rows] - sin * a[i + (n - 1) * rows];
            a[i + (n - 1) * rows] = sin * a[i + (j - 1) * rows] + cos * a[i + (n - 1) * rows];
            a[i + (j - 1) * rows] = temp;
        }
    }

    for j in 1..n {
        let (cos, sin) = recover(w[j - 1]);
        for i in 0..rows {
            let temp = cos * a[i + (j - 1) * rows] + sin * a[i + (n - 1) * rows];
            a[i + (n - 1) * rows] = -sin * a[i + (j - 1) * rows] + cos * a[i + (n - 1) * rows];
            a[i + (j - 1) * rows] = temp;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rebuild the dense `n`-by-`n` `R` from `qrfac`'s output.
    fn dense_r(n: usize, a: &[f64], rdiag: &[f64]) -> Vec<f64> {
        let mut r = vec![0.0_f64; n * n];
        for j in 0..n {
            r[j + j * n] = rdiag[j];
            for i in 0..j {
                r[i + j * n] = a[i + j * n];
            }
        }
        r
    }

    fn multiply(n: usize, left: &[f64], right: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0_f64; n * n];
        for j in 0..n {
            for i in 0..n {
                let mut sum = 0.0;
                for k in 0..n {
                    sum += left[i + k * n] * right[k + j * n];
                }
                out[i + j * n] = sum;
            }
        }
        out
    }

    #[test]
    fn the_factorization_multiplies_back_to_the_matrix_it_factored() {
        let n = 4;
        let original: Vec<f64> = (0..n * n)
            .map(|k| ((k * 37 % 17) as f64) - 8.0 + 0.5 * (k as f64))
            .collect();

        let mut a = original.clone();
        let mut rdiag = vec![0.0; n];
        let mut acnorm = vec![0.0; n];
        qrfac(n, &mut a, &mut rdiag, &mut acnorm);

        let r = dense_r(n, &a, &rdiag);
        let mut q = a.clone();
        qform(n, &mut q);

        let product = multiply(n, &q, &r);
        for (got, want) in product.iter().zip(original.iter()) {
            assert!((got - want).abs() < 1e-12, "QR != A: {got} vs {want}");
        }
    }

    #[test]
    fn the_accumulated_factor_is_orthogonal() {
        let n = 5;
        let mut a: Vec<f64> = (0..n * n).map(|k| ((k % 7) as f64) - 3.0).collect();
        let mut rdiag = vec![0.0; n];
        let mut acnorm = vec![0.0; n];
        qrfac(n, &mut a, &mut rdiag, &mut acnorm);
        qform(n, &mut a);

        let mut transpose = vec![0.0; n * n];
        for j in 0..n {
            for i in 0..n {
                transpose[i + j * n] = a[j + i * n];
            }
        }
        let identity = multiply(n, &transpose, &a);
        for j in 0..n {
            for i in 0..n {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((identity[i + j * n] - want).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn the_recorded_column_norms_are_the_norms_of_the_original_columns() {
        let n = 3;
        let original = vec![3.0, 4.0, 0.0, 0.0, 5.0, 12.0, 1.0, 2.0, 2.0];
        let mut a = original.clone();
        let mut rdiag = vec![0.0; n];
        let mut acnorm = vec![0.0; n];
        qrfac(n, &mut a, &mut rdiag, &mut acnorm);
        assert!((acnorm[0] - 5.0).abs() < 1e-13);
        assert!((acnorm[1] - 13.0).abs() < 1e-13);
        assert!((acnorm[2] - 3.0).abs() < 1e-13);
    }

    #[test]
    fn a_rank_one_update_leaves_a_triangle_that_still_factors_the_updated_matrix() {
        // Given the `R` of `A`, `r1updt` produces the `R` of `A + col row^T`
        // -- where `col` is passed as its `v` argument, rotated into the `Q`
        // frame, and `row` is passed as its `u` argument. That order is the
        // transposed-storage subtlety the function's own doc comment records,
        // and getting it backwards still yields a triangular matrix, so this
        // checks the matrix it factors rather than that it is triangular.
        let n = 4;
        let base: Vec<f64> = (0..n * n).map(|k| ((k * 13 % 11) as f64) - 5.0).collect();
        // `A` gains `col * row^T`.
        let col: Vec<f64> = (0..n).map(|i| 0.5 + i as f64).collect();
        let row: Vec<f64> = (0..n).map(|i| 1.0 - 0.25 * i as f64).collect();

        let mut a = base.clone();
        let mut rdiag = vec![0.0; n];
        let mut acnorm = vec![0.0; n];
        qrfac(n, &mut a, &mut rdiag, &mut acnorm);
        let mut q = a.clone();
        qform(n, &mut q);

        // Pack `R` by rows, as `hybrd` stores it.
        let mut packed = Vec::with_capacity(n * (n + 1) / 2);
        for i in 0..n {
            for j in i..n {
                packed.push(if i == j { rdiag[i] } else { a[i + j * n] });
            }
        }

        // The update is expressed in the rotated frame, so the column vector
        // goes in as `Q^T col`.
        let mut qt_col = vec![0.0_f64; n];
        for j in 0..n {
            let mut sum = 0.0;
            for i in 0..n {
                sum += q[i + j * n] * col[i];
            }
            qt_col[j] = sum;
        }

        let mut w = vec![0.0_f64; n];
        r1updt(n, &mut packed, &row, &mut qt_col, &mut w);

        // Rebuild the dense updated triangle and compare `R^T R` against
        // `(A + u v^T)^T (A + u v^T)`, which is invariant to the sign
        // convention the rotations leave each row with.
        let mut updated_r = vec![0.0_f64; n * n];
        let mut k = 0;
        for i in 0..n {
            for j in i..n {
                updated_r[i + j * n] = packed[k];
                k += 1;
            }
        }
        let mut expected = base.clone();
        for j in 0..n {
            for i in 0..n {
                expected[i + j * n] += col[i] * row[j];
            }
        }

        let gram = |m: &[f64]| {
            let mut g = vec![0.0_f64; n * n];
            for j in 0..n {
                for i in 0..n {
                    let mut sum = 0.0;
                    for r in 0..n {
                        sum += m[r + i * n] * m[r + j * n];
                    }
                    g[i + j * n] = sum;
                }
            }
            g
        };
        for (got, want) in gram(&updated_r).iter().zip(gram(&expected).iter()) {
            assert!((got - want).abs() < 1e-9, "{got} vs {want}");
        }
    }

    #[test]
    fn a_rotation_round_trips_through_the_single_number_it_is_stored_as() {
        // `r1updt` compresses each rotation to one number and `r1mpyq`
        // expands it again; the two have to agree or the update corrupts `Q`.
        for (a, b) in [(3.0, 4.0), (4.0, 3.0), (-1.0, 7.0), (1.0, 0.0)] {
            let g = givens(a, b);
            let (cos, sin) = if g.tau.abs() > 1.0 {
                let c = 1.0 / g.tau;
                (c, (1.0 - c * c).sqrt())
            } else {
                let s = g.tau;
                ((1.0 - s * s).sqrt(), s)
            };
            assert!(
                (cos.abs() - g.cos.abs()).abs() < 1e-12,
                "cos for ({a}, {b})"
            );
            assert!(
                (sin.abs() - g.sin.abs()).abs() < 1e-12,
                "sin for ({a}, {b})"
            );
        }
    }
}
