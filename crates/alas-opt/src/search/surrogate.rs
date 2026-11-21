// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Radial-basis surrogates of the objective and of the signed constraint
//! margins, fitted to the candidates a search stage has already analysed.
//!
//! The model of one quantity is the cubic radial-basis interpolant with a
//! linear polynomial tail,
//! `s(x) = sum_i w_i |x - x_i|^3 + c_0 + c . x`, whose weights solve
//!
//! ```text
//! [ Phi + rho I   P ] [ w ]   [ y ]
//! [ P^T           0 ] [ c ] = [ 0 ]
//! ```
//!
//! with `Phi_ij = |x_i - x_j|^3` and `P = [1, x_i]` (M. J. D. Powell, "The
//! Theory of Radial Basis Function Approximation in 1990," in *Advances in
//! Numerical Analysis II*, Oxford University Press, 1992; the form R. G.
//! Regis uses for expensive constrained problems in "Constrained
//! Optimization by Radial Basis Function Interpolation for High-Dimensional
//! Expensive Black-Box Problems with Infeasible Initial Points," Engineering
//! Optimization 46(2), 2014, DOI 10.1080/0305215X.2013.765000). One
//! factorization serves every modelled quantity. `rho` is a small ridge that
//! keeps the system solvable when two analysed points nearly coincide; with
//! it the model is a smoothing fit that misses each node by about
//! `rho * w_i`.
//!
//! The surrogate only orders candidates before they are analysed. It never
//! scores, accepts or delivers a design: every number a run reports comes
//! from the coupled analysis.
//!
//! Coordinates are the free design variables scaled to `[0, 1]` over the
//! search box, so the model is independent of each variable's unit.

use alas_math::linalg::DenseMatrix;

/// Ridge added to the kernel diagonal (dimensionless, on unit coordinates).
const RIDGE: f64 = 1.0e-8;

/// Points closer than this in unit coordinates are one node: the later one
/// is left out of the fit.
const COINCIDENT: f64 = 1.0e-7;

/// A fitted set of interpolants sharing their nodes.
#[derive(Debug, Clone)]
pub(crate) struct Surrogate {
    nodes: Vec<Vec<f64>>,
    /// Per modelled quantity: `n` kernel weights, then the constant and the
    /// linear coefficients.
    coefficients: Vec<Vec<f64>>,
}

impl Surrogate {
    /// Fit `outputs[q][i]`, quantity `q` at `points[i]` (unit coordinates).
    ///
    /// `None` when there are fewer distinct points than the linear tail
    /// needs (`dimension + 1`), a value is not finite, or the system is
    /// singular (points confined to a hyperplane).
    pub(crate) fn fit(points: &[Vec<f64>], outputs: &[Vec<f64>]) -> Option<Self> {
        let dimension = points.first()?.len();
        let mut kept: Vec<usize> = Vec::with_capacity(points.len());
        for (index, point) in points.iter().enumerate() {
            if point.len() != dimension || point.iter().any(|value| !value.is_finite()) {
                return None;
            }
            if kept
                .iter()
                .all(|&other| distance(point, &points[other]) > COINCIDENT)
            {
                kept.push(index);
            }
        }
        let n = kept.len();
        if n < dimension + 2 || outputs.iter().any(|values| values.len() != points.len()) {
            return None;
        }
        let size = n + dimension + 1;
        let mut matrix = vec![0.0_f64; size * size];
        for (row, &i) in kept.iter().enumerate() {
            for (column, &j) in kept.iter().enumerate().skip(row + 1) {
                let value = distance(&points[i], &points[j]).powi(3);
                matrix[row * size + column] = value;
                matrix[column * size + row] = value;
            }
            matrix[row * size + row] = RIDGE;
            matrix[row * size + n] = 1.0;
            matrix[n * size + row] = 1.0;
            for (axis, &value) in points[i].iter().enumerate() {
                matrix[row * size + n + 1 + axis] = value;
                matrix[(n + 1 + axis) * size + row] = value;
            }
        }
        let factorization = DenseMatrix::from_row_major(size, &matrix).factor().ok()?;
        let rhs: Vec<Vec<f64>> = (0..size)
            .map(|row| {
                outputs
                    .iter()
                    .map(|values| kept.get(row).map_or(0.0, |&index| values[index]))
                    .collect()
            })
            .collect();
        if rhs.iter().flatten().any(|value| !value.is_finite()) {
            return None;
        }
        let (solution, _) = factorization.solve_columns(&rhs);
        if solution.iter().flatten().any(|value| !value.is_finite()) {
            return None;
        }
        let coefficients = (0..outputs.len())
            .map(|quantity| solution.iter().map(|row| row[quantity]).collect())
            .collect();
        Some(Self {
            nodes: kept
                .into_iter()
                .map(|index| points[index].clone())
                .collect(),
            coefficients,
        })
    }

    /// Every modelled quantity at `point` (unit coordinates), in fit order.
    pub(crate) fn predict(&self, point: &[f64]) -> Vec<f64> {
        let n = self.nodes.len();
        let kernel: Vec<f64> = self
            .nodes
            .iter()
            .map(|node| distance(point, node).powi(3))
            .collect();
        self.coefficients
            .iter()
            .map(|coefficients| {
                let radial: f64 = kernel
                    .iter()
                    .zip(coefficients)
                    .map(|(phi, weight)| phi * weight)
                    .sum();
                let linear: f64 = point
                    .iter()
                    .zip(&coefficients[n + 1..])
                    .map(|(value, slope)| value * slope)
                    .sum();
                radial + coefficients[n] + linear
            })
            .collect()
    }
}

fn distance(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_methods::rng::SearchRng;

    fn sample(count: usize, dimension: usize, seed: u64) -> Vec<Vec<f64>> {
        let mut rng = SearchRng::seed(seed);
        (0..count)
            .map(|_| (0..dimension).map(|_| rng.unit()).collect())
            .collect()
    }

    #[test]
    fn a_linear_quantity_is_reproduced_everywhere_and_the_nodes_are_met() {
        let points = sample(40, 5, 3);
        let linear = |x: &[f64]| 2.0 - x[0] + 0.5 * x[3];
        let curved = |x: &[f64]| (x[1] - 0.4).powi(2) + x[2] * x[4];
        let outputs = vec![
            points.iter().map(|p| linear(p)).collect::<Vec<_>>(),
            points.iter().map(|p| curved(p)).collect::<Vec<_>>(),
        ];
        let model = Surrogate::fit(&points, &outputs).expect("fits");
        // The ridge makes the fit a smoothing one: each node is missed by
        // about `RIDGE * w_i`, a few parts in 1e5 here, not by round-off.
        for point in &points {
            let predicted = model.predict(point);
            assert!((predicted[0] - linear(point)).abs() < 1e-4, "{predicted:?}");
            assert!((predicted[1] - curved(point)).abs() < 1e-4, "{predicted:?}");
        }
        let mut curved_error = 0.0;
        for point in sample(50, 5, 9) {
            let predicted = model.predict(&point);
            assert!(
                (predicted[0] - linear(&point)).abs() < 1e-3,
                "{predicted:?}"
            );
            curved_error += (predicted[1] - curved(&point)).abs() / 50.0;
        }
        // A smooth curved quantity is ranked, not reproduced: its mean error
        // on forty points in five dimensions is a few percent of its range
        // (about 1.3 here).
        assert!(curved_error < 0.1, "{curved_error}");
    }

    #[test]
    fn too_few_or_degenerate_points_give_no_model() {
        let points = sample(5, 5, 1);
        let outputs = vec![vec![1.0; 5]];
        assert!(Surrogate::fit(&points, &outputs).is_none());
        // Repeats of one point are one node.
        let repeated = vec![vec![0.5; 3]; 12];
        assert!(Surrogate::fit(&repeated, &[vec![1.0; 12]]).is_none());
        let mut bad = sample(20, 3, 2);
        bad[4][1] = f64::NAN;
        assert!(Surrogate::fit(&bad, &[vec![1.0; 20]]).is_none());
        assert!(Surrogate::fit(&sample(20, 3, 2), &[vec![f64::INFINITY; 20]]).is_none());
    }
}
