// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Variable scaling, the running best point, and the finite-difference
//! gradient and Jacobian the SQP driver linearises with.

use super::sqp::{ConstrainedEvaluator, ConstrainedPoint};

/// The map between physical design variables and the unit box.
pub(super) struct Scaling {
    lower: Vec<f64>,
    width: Vec<f64>,
    /// Indices of the variables whose bounds have positive width.
    pub(super) active: Vec<usize>,
}

impl Scaling {
    pub(super) fn new(bounds: &[(f64, f64)]) -> Self {
        let lower = bounds.iter().map(|b| b.0).collect();
        let width: Vec<f64> = bounds.iter().map(|b| b.1 - b.0).collect();
        let active = width
            .iter()
            .enumerate()
            .filter(|(_, w)| **w > 0.0)
            .map(|(i, _)| i)
            .collect();
        Self {
            lower,
            width,
            active,
        }
    }

    pub(super) fn to_physical(&self, z: &[f64]) -> Vec<f64> {
        (0..self.lower.len())
            .map(|i| self.lower[i] + z[i].clamp(0.0, 1.0) * self.width[i])
            .collect()
    }

    pub(super) fn to_normalized(&self, x: &[f64]) -> Vec<f64> {
        (0..self.lower.len())
            .map(|i| {
                if self.width[i] > 0.0 {
                    ((x[i] - self.lower[i]) / self.width[i]).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect()
    }
}

/// The best point seen so far: feasible beats infeasible, then lower
/// objective among feasible points, then lower maximum violation.
pub(super) struct Best {
    pub(super) z: Vec<f64>,
    pub(super) point: ConstrainedPoint,
}

impl Best {
    pub(super) fn offer(&mut self, z: &[f64], point: &ConstrainedPoint, tolerance: f64) {
        if !point.valid {
            return;
        }
        let candidate_feasible = point.max_violation() <= tolerance;
        let best_feasible = self.point.valid && self.point.max_violation() <= tolerance;
        let better = match (candidate_feasible, best_feasible) {
            (true, true) => point.objective < self.point.objective,
            (true, false) => true,
            (false, true) => false,
            (false, false) => {
                !self.point.valid || point.max_violation() < self.point.max_violation()
            }
        };
        if better {
            self.z = z.to_vec();
            self.point = point.clone();
        }
    }
}

/// What one finite-difference pass needs beyond the evaluator.
pub(super) struct DifferenceRequest<'a> {
    pub(super) scaling: &'a Scaling,
    pub(super) z: &'a [f64],
    pub(super) base: &'a ConstrainedPoint,
    /// Step in normalised coordinates.
    pub(super) step: f64,
    pub(super) tolerance: f64,
}

/// Forward differences of the objective and constraints over the active
/// variables, one batch per side: a probe that fails is retried from the
/// other side, and a variable that fails both ways gets a zero column.
/// Returns the gradient and the Jacobian (one row per constraint, one
/// column per active variable).
pub(super) fn differences(
    request: &DifferenceRequest<'_>,
    evaluator: &mut dyn ConstrainedEvaluator,
    evaluations: &mut usize,
    best: &mut Best,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let DifferenceRequest {
        scaling,
        z,
        base,
        step,
        tolerance,
    } = *request;
    let m = base.constraints.len();
    let k = scaling.active.len();
    let mut grad = vec![0.0; k];
    let mut jac = vec![vec![0.0; k]; m];
    let side = |z_i: f64, forward: bool| {
        if forward {
            if z_i + step <= 1.0 {
                step
            } else {
                -step
            }
        } else if z_i - step >= 0.0 {
            -step
        } else {
            step
        }
    };
    let mut pending: Vec<usize> = (0..k).collect();
    for forward in [true, false] {
        if pending.is_empty() {
            break;
        }
        let mut probes = Vec::with_capacity(pending.len());
        for &column in &pending {
            let i = scaling.active[column];
            let mut probe = z.to_vec();
            probe[i] = (z[i] + side(z[i], forward)).clamp(0.0, 1.0);
            probes.push(probe);
        }
        let designs: Vec<Vec<f64>> = probes.iter().map(|p| scaling.to_physical(p)).collect();
        let points = evaluator.evaluate_batch(&designs);
        *evaluations += points.len();
        let mut still_pending = Vec::new();
        for ((&column, point), probe) in pending.iter().zip(&points).zip(&probes) {
            best.offer(probe, point, tolerance);
            let h = probe[scaling.active[column]] - z[scaling.active[column]];
            if point.valid && point.constraints.len() == m && h.abs() > 0.0 {
                grad[column] = (point.objective - base.objective) / h;
                for (row, (&probe_c, &base_c)) in jac
                    .iter_mut()
                    .zip(point.constraints.iter().zip(&base.constraints))
                {
                    row[column] = (probe_c - base_c) / h;
                }
            } else {
                still_pending.push(column);
            }
        }
        pending = still_pending;
    }
    (grad, jac)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Linear;

    impl ConstrainedEvaluator for Linear {
        fn evaluate_batch(&mut self, designs: &[Vec<f64>]) -> Vec<ConstrainedPoint> {
            designs
                .iter()
                .map(|x| ConstrainedPoint {
                    objective: 3.0 * x[0] - 2.0 * x[1],
                    constraints: vec![x[0] + x[1] - 1.0],
                    valid: x[0] <= 9.0,
                    cost: 0.0,
                })
                .collect()
        }
    }

    #[test]
    fn a_linear_function_is_differentiated_exactly_in_physical_units_and_retried_at_a_bound() {
        // Variable 0 spans [0, 10], variable 1 spans [0, 1]; the base sits
        // at x0 = 9, the edge of validity, so the forward probe of variable
        // 0 is invalid and the backward one is used on the second batch.
        let scaling = Scaling::new(&[(0.0, 10.0), (0.0, 1.0)]);
        let z = vec![0.9, 0.5];
        let mut evaluator = Linear;
        let base = evaluator
            .evaluate_batch(&[scaling.to_physical(&z)])
            .pop()
            .unwrap_or_else(|| panic!("evaluates"));
        let mut evaluations = 0;
        let mut best = Best {
            z: z.clone(),
            point: base.clone(),
        };
        let request = DifferenceRequest {
            scaling: &scaling,
            z: &z,
            base: &base,
            step: 1e-3,
            tolerance: 1e-6,
        };
        let (grad, jac) = differences(&request, &mut evaluator, &mut evaluations, &mut best);
        // Normalised gradient = physical gradient times the bound width.
        assert!((grad[0] - 30.0).abs() < 1e-6, "{grad:?}");
        assert!((grad[1] + 2.0).abs() < 1e-6, "{grad:?}");
        assert!((jac[0][0] - 10.0).abs() < 1e-6 && (jac[0][1] - 1.0).abs() < 1e-6);
        assert_eq!(evaluations, 3);
    }
}
