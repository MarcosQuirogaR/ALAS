// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sequential quadratic programming driver.
//!
//! Line-search SQP with an l1 merit function and a damped BFGS Hessian
//! (Nocedal and Wright, Algorithms 18.3 and Procedure 18.2), in variables
//! normalised to the unit box. Each major iteration linearises the
//! inequality constraints and solves the elastic quadratic subproblem of
//! [`super::qp`]: slack variables with an l1 penalty keep the subproblem
//! feasible when the linearised constraints are inconsistent, as SNOPT's
//! elastic mode does, instead of aborting on an infeasible linearisation.
//! Derivatives come from forward differences evaluated as one batch per
//! iteration, so the disciplinary evaluations run in parallel.
//!
//! An evaluation that could not be built, trimmed or sized (`valid` false)
//! is treated as an infinite merit value: the line search backs off from
//! it and a difference through it is retried from the other side.

use super::differences::{differences, Best, DifferenceRequest, Scaling};
use super::step::{damped_bfgs, identity, subproblem};

/// The accepted step, the gradient and Jacobian it was taken from, and
/// the subproblem multipliers: the BFGS secant of the next iteration.
type Secant = (Vec<f64>, Vec<f64>, Vec<Vec<f64>>, Vec<f64>);

/// One evaluated design, as the driver sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstrainedPoint {
    /// Normalised objective to minimise.
    pub objective: f64,
    /// Inequality constraints, feasible when `<= 0`, normalised.
    pub constraints: Vec<f64>,
    /// Whether the analysis completed; an invalid point has no usable
    /// objective or constraints.
    pub valid: bool,
    /// The ranking scalar the rest of the optimizer reports.
    pub cost: f64,
}

impl ConstrainedPoint {
    /// A point whose analysis failed.
    pub fn invalid(cost: f64) -> Self {
        Self {
            objective: f64::INFINITY,
            constraints: Vec::new(),
            valid: false,
            cost,
        }
    }

    /// Largest constraint value, zero when there are none.
    pub fn max_violation(&self) -> f64 {
        self.constraints.iter().fold(0.0, |acc, c| acc.max(*c))
    }

    fn l1_violation(&self) -> f64 {
        self.constraints.iter().map(|c| c.max(0.0)).sum()
    }
}

/// Evaluates physical design vectors, several at a time.
pub trait ConstrainedEvaluator {
    /// Evaluate every design in `designs`, in order.
    fn evaluate_batch(&mut self, designs: &[Vec<f64>]) -> Vec<ConstrainedPoint>;
}

/// Driver settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SqpSettings {
    /// Major iterations.
    pub max_iterations: usize,
    /// Forward-difference step as a fraction of each bound range.
    pub finite_difference_step: f64,
    /// Constraint violation accepted as feasible.
    pub constraint_tolerance: f64,
    /// Relative objective change over two feasible iterations that counts
    /// as converged.
    pub objective_tolerance: f64,
    /// Normalised step below which the iteration has stopped moving.
    pub step_tolerance: f64,
}

/// What the driver returned and why it stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct SqpOutcome {
    /// Physical design vector of the best point found.
    pub best_values: Vec<f64>,
    /// The best point's evaluation.
    pub best: ConstrainedPoint,
    /// Major iterations completed.
    pub iterations: usize,
    /// Analyses performed.
    pub evaluations: usize,
    /// Whether a convergence test, rather than a budget or failure, ended
    /// the run.
    pub converged: bool,
    /// Stable termination label.
    pub termination: &'static str,
}

/// Armijo sufficient-decrease constant.
const ARMIJO: f64 = 1.0e-4;
/// Smallest line-search step tried, as a power of one half.
const LINE_SEARCH_HALVINGS: usize = 6;

fn merit(point: &ConstrainedPoint, penalty: f64) -> f64 {
    if !point.valid {
        return f64::INFINITY;
    }
    point.objective + penalty * point.l1_violation()
}

/// Run the driver from `initial` (physical units) inside `bounds`.
pub fn run_sqp(
    bounds: &[(f64, f64)],
    initial: &[f64],
    settings: &SqpSettings,
    evaluator: &mut dyn ConstrainedEvaluator,
    mut progress: Option<&mut dyn FnMut(&str)>,
) -> SqpOutcome {
    let scaling = Scaling::new(bounds);
    let tolerance = settings.constraint_tolerance;
    let mut z = scaling.to_normalized(initial);
    let mut evaluations = 0;
    let mut point = evaluator
        .evaluate_batch(&[scaling.to_physical(&z)])
        .pop()
        .unwrap_or_else(|| ConstrainedPoint::invalid(f64::INFINITY));
    evaluations += 1;
    let mut best = Best {
        z: z.clone(),
        point: point.clone(),
    };
    let finish = |best: Best, iterations, evaluations, converged, termination| SqpOutcome {
        best_values: scaling.to_physical(&best.z),
        best: best.point,
        iterations,
        evaluations,
        converged,
        termination,
    };
    if !point.valid {
        return finish(best, 0, evaluations, false, "initial_point_invalid");
    }
    let k = scaling.active.len();
    if k == 0 {
        return finish(best, 0, evaluations, true, "no_free_variables");
    }

    let mut hessian = identity(k);
    let mut penalty: f64 = 1.0;
    let mut previous: Option<Secant> = None;
    let mut feasible_history: Vec<f64> = Vec::new();
    let mut line_search_failures = 0;
    let mut iterations = 0;

    for iteration in 0..settings.max_iterations {
        iterations = iteration + 1;
        let request = DifferenceRequest {
            scaling: &scaling,
            z: &z,
            base: &point,
            step: settings.finite_difference_step,
            tolerance,
        };
        let (grad, jac) = differences(&request, evaluator, &mut evaluations, &mut best);
        let m = point.constraints.len();
        if let Some((s, grad_prev, jac_prev, lambda)) = previous.take() {
            let lagrangian = |g: &[f64], j: &[Vec<f64>]| -> Vec<f64> {
                (0..k)
                    .map(|i| g[i] + (0..m).map(|c| j[c][i] * lambda[c]).sum::<f64>())
                    .collect()
            };
            if jac_prev.len() == m {
                let y_new = lagrangian(&grad, &jac);
                let y_old = lagrangian(&grad_prev, &jac_prev);
                let y: Vec<f64> = y_new.iter().zip(&y_old).map(|(a, b)| a - b).collect();
                damped_bfgs(&mut hessian, &s, &y);
            }
        }

        let z_active: Vec<f64> = scaling.active.iter().map(|&i| z[i]).collect();
        let Some((d, multipliers)) =
            subproblem(&hessian, &grad, &jac, &point.constraints, &z_active)
        else {
            return finish(best, iterations, evaluations, false, "subproblem_failed");
        };
        let step_norm = d.iter().fold(0.0, |acc: f64, v| acc.max(v.abs()));
        let violation = point.max_violation();
        if step_norm <= settings.step_tolerance && violation <= tolerance {
            if let Some(progress) = progress.as_mut() {
                progress(&format!(
                    "sqp converged at iteration {iterations}: step {step_norm:.2e}, max violation {violation:.2e}, objective {:.6}",
                    point.objective
                ));
            }
            return finish(best, iterations, evaluations, true, "converged_step");
        }

        // Merit penalty large enough that d is a descent direction of the
        // l1 merit (Nocedal and Wright, eq. 18.36 with rho = 0.5).
        let g_d: f64 = grad.iter().zip(&d).map(|(g, d)| g * d).sum();
        let d_b_d: f64 = (0..k)
            .map(|i| d[i] * (0..k).map(|j| hessian[i][j] * d[j]).sum::<f64>())
            .sum();
        let linearised_violation: f64 = (0..m)
            .map(|j| {
                (point.constraints[j] + (0..k).map(|i| jac[j][i] * d[i]).sum::<f64>()).max(0.0)
            })
            .sum();
        let reduction = point.l1_violation() - linearised_violation;
        let lambda_norm = multipliers.iter().fold(0.0, |acc: f64, l| acc.max(l.abs()));
        penalty = penalty.max(lambda_norm + 1.0);
        if reduction > 1e-12 {
            penalty = penalty.max((g_d + 0.5 * d_b_d) / (0.5 * reduction));
        }
        let directional = g_d - penalty * reduction;
        let merit_here = merit(&point, penalty);

        let mut accepted: Option<(f64, Vec<f64>, ConstrainedPoint)> = None;
        let mut alpha = 1.0;
        for _ in 0..=LINE_SEARCH_HALVINGS {
            let mut trial = z.clone();
            for (column, &i) in scaling.active.iter().enumerate() {
                trial[i] = (z[i] + alpha * d[column]).clamp(0.0, 1.0);
            }
            let trial_point = evaluator
                .evaluate_batch(&[scaling.to_physical(&trial)])
                .pop()
                .unwrap_or_else(|| ConstrainedPoint::invalid(f64::INFINITY));
            evaluations += 1;
            best.offer(&trial, &trial_point, tolerance);
            let merit_trial = merit(&trial_point, penalty);
            if merit_trial <= merit_here + ARMIJO * alpha * directional.min(0.0)
                && merit_trial.is_finite()
            {
                accepted = Some((alpha, trial, trial_point));
                break;
            }
            alpha *= 0.5;
        }
        let Some((alpha, next_z, next_point)) = accepted else {
            line_search_failures += 1;
            if line_search_failures >= 2 {
                return finish(best, iterations, evaluations, false, "line_search_failed");
            }
            // Curvature information may be stale; restart from identity.
            hessian = identity(k);
            previous = None;
            continue;
        };
        line_search_failures = 0;
        let s: Vec<f64> = (0..k).map(|i| alpha * d[i]).collect();
        previous = Some((s, grad, jac, multipliers));

        if let Some(progress) = progress.as_mut() {
            progress(&format!(
                "sqp iteration {iterations}/{} | objective {:.6} -> {:.6} | max violation {:.2e} | step {:.3e} x {alpha} | evaluations {evaluations}",
                settings.max_iterations,
                point.objective,
                next_point.objective,
                next_point.max_violation(),
                step_norm
            ));
        }
        z = next_z;
        point = next_point;

        if point.max_violation() <= tolerance {
            feasible_history.push(point.objective);
            if feasible_history.len() >= 3 {
                let n = feasible_history.len();
                let scale = feasible_history[n - 1].abs().max(1.0);
                let recent = (feasible_history[n - 1] - feasible_history[n - 2]).abs();
                let older = (feasible_history[n - 2] - feasible_history[n - 3]).abs();
                if recent <= settings.objective_tolerance * scale
                    && older <= settings.objective_tolerance * scale
                {
                    return finish(best, iterations, evaluations, true, "converged_objective");
                }
            }
        } else {
            feasible_history.clear();
        }
    }
    finish(best, iterations, evaluations, false, "iteration_limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Analytic<F: FnMut(&[f64]) -> ConstrainedPoint>(F);

    impl<F: FnMut(&[f64]) -> ConstrainedPoint> ConstrainedEvaluator for Analytic<F> {
        fn evaluate_batch(&mut self, designs: &[Vec<f64>]) -> Vec<ConstrainedPoint> {
            designs.iter().map(|x| (self.0)(x)).collect()
        }
    }

    fn settings() -> SqpSettings {
        SqpSettings {
            max_iterations: 60,
            finite_difference_step: 1e-6,
            constraint_tolerance: 1e-6,
            objective_tolerance: 1e-10,
            step_tolerance: 1e-7,
        }
    }

    #[test]
    fn a_bound_constrained_quadratic_converges_to_its_minimiser() {
        let mut evaluator = Analytic(|x: &[f64]| ConstrainedPoint {
            objective: (x[0] - 0.3).powi(2) + 2.0 * (x[1] + 0.2).powi(2),
            constraints: Vec::new(),
            valid: true,
            cost: 0.0,
        });
        let outcome = run_sqp(
            &[(-1.0, 1.0), (-1.0, 1.0)],
            &[0.9, 0.9],
            &settings(),
            &mut evaluator,
            None,
        );
        assert!(outcome.converged, "{outcome:?}");
        assert!(
            (outcome.best_values[0] - 0.3).abs() < 1e-4,
            "{:?}",
            outcome.best_values
        );
        assert!(
            (outcome.best_values[1] + 0.2).abs() < 1e-4,
            "{:?}",
            outcome.best_values
        );
    }

    #[test]
    fn an_active_nonlinear_constraint_is_found_and_satisfied() {
        // min (x-2)^2 + (y-1)^2  s.t.  x^2 - y <= 0,  x + y <= 2.
        // Solution (1, 1): both constraints active.
        let mut evaluator = Analytic(|x: &[f64]| ConstrainedPoint {
            objective: (x[0] - 2.0).powi(2) + (x[1] - 1.0).powi(2),
            constraints: vec![x[0] * x[0] - x[1], x[0] + x[1] - 2.0],
            valid: true,
            cost: 0.0,
        });
        let outcome = run_sqp(
            &[(-3.0, 3.0), (-3.0, 3.0)],
            &[0.0, 0.0],
            &settings(),
            &mut evaluator,
            None,
        );
        assert!(outcome.best.max_violation() <= 1e-5, "{outcome:?}");
        assert!(
            (outcome.best_values[0] - 1.0).abs() < 1e-3,
            "{:?}",
            outcome.best_values
        );
        assert!(
            (outcome.best_values[1] - 1.0).abs() < 1e-3,
            "{:?}",
            outcome.best_values
        );
    }

    #[test]
    fn an_infeasible_start_is_driven_feasible_before_the_objective_is_polished() {
        // min x + y  s.t.  1 - x*y <= 0, x, y in [0.1, 5]; solution (1, 1).
        let mut evaluator = Analytic(|x: &[f64]| ConstrainedPoint {
            objective: x[0] + x[1],
            constraints: vec![1.0 - x[0] * x[1]],
            valid: true,
            cost: 0.0,
        });
        let outcome = run_sqp(
            &[(0.1, 5.0), (0.1, 5.0)],
            &[0.2, 0.3],
            &settings(),
            &mut evaluator,
            None,
        );
        assert!(outcome.best.max_violation() <= 1e-5, "{outcome:?}");
        assert!((outcome.best.objective - 2.0).abs() < 2e-3, "{outcome:?}");
    }

    #[test]
    fn a_fixed_variable_is_left_alone_and_an_invalid_start_is_reported() {
        let mut evaluator = Analytic(|x: &[f64]| ConstrainedPoint {
            objective: (x[0] - 0.5).powi(2) + x[1].powi(2),
            constraints: Vec::new(),
            valid: true,
            cost: 0.0,
        });
        let outcome = run_sqp(
            &[(0.0, 1.0), (2.0, 2.0)],
            &[0.0, 2.0],
            &settings(),
            &mut evaluator,
            None,
        );
        assert!(outcome.converged);
        assert!((outcome.best_values[0] - 0.5).abs() < 1e-4);
        assert_eq!(outcome.best_values[1], 2.0);

        let mut failing = Analytic(|_: &[f64]| ConstrainedPoint::invalid(1.0e3));
        let outcome = run_sqp(&[(0.0, 1.0)], &[0.5], &settings(), &mut failing, None);
        assert_eq!(outcome.termination, "initial_point_invalid");
        assert!(!outcome.best.valid);
    }
}
