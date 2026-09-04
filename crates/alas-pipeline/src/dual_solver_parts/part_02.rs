// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl AvlObjective {
    fn evaluate_uncached(&self, design: &DesignVector, key: &str) -> ObjectiveEvaluation {
        let enforce_physical_constraints =
            self.config.optimizer.solver.enforce_physical_constraints;
        let report = match FullAnalysis::new(self.config.clone()).run(design, true) {
            Ok(report) => report,
            Err(_) => return ObjectiveEvaluation::rejected(self.failure_cost(), "full_analysis"),
        };
        let projected_wing_area_m2 = report
            .airplane
            .wings
            .first()
            .map_or(f64::NAN, alas_geom::aircraft::wing::Wing::projected_area);
        let area_penalty = if enforce_physical_constraints {
            let Some(penalty) = wing_area_excess_penalty(
                projected_wing_area_m2,
                self.config.requirements.max_wing_area_m2,
                self.config.optimizer.weights.area_penalty_scale,
            ) else {
                return ObjectiveEvaluation::rejected(self.failure_cost(), "wing_area_limit");
            };
            penalty
        } else {
            0.0
        };
        if enforce_physical_constraints && report.cg_envelope_ok != Some(true) {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cg_envelope");
        }
        if enforce_physical_constraints
            && report.static_margin < self.config.requirements.min_physical_static_margin
        {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "static_margin");
        }
        let evaluation_dir = self.output_root.join(key);
        let avl = run_avl_analysis(
            &report,
            &self.config,
            &evaluation_dir,
            Some(&self.executable),
            300.0,
        );
        let Some(polar) = avl.comparable_polar() else {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_unavailable");
        };
        let required_cl = FullAnalysis::new(self.config.clone()).cruise_cl(&report.airplane);
        let Some(point) = interpolate_avl_at_lift(polar, required_cl) else {
            return ObjectiveEvaluation::rejected(
                self.failure_cost(),
                "avl_required_lift_out_of_range",
            );
        };
        if !point.induced_drag_coefficient.is_finite() || point.induced_drag_coefficient <= 0.0 {
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_induced_drag");
        }
        let induced_l_over_d = required_cl / point.induced_drag_coefficient;
        let moment_penalty = point.pitching_moment_coefficient.abs();
        let cost = -induced_l_over_d + 10.0 * moment_penalty + area_penalty;
        ObjectiveEvaluation {
            cost,
            valid: true,
            l_over_d: induced_l_over_d,
            span_m: report.airplane.b_ref,
            alpha_deg: point.alpha_deg,
            area_m2: report.airplane.s_ref,
            trim_ih_deg: report
                .trimmed_design_point
                .map_or(0.0, |trim| trim.trim_ih_deg),
            reject_reason: String::new(),
        }
    }

    fn failure_cost(&self) -> f64 {
        self.config.optimizer.weights.failure_cost
    }
}

/// Interpolate the AVL polar at the required cruise lift coefficient.
///
/// The AVL branch is an induced-drag objective at a prescribed lift state; a
/// nearest-alpha lookup changes that state whenever the alpha grid or design
/// lift curve moves. Only a bracketed finite pair is admitted, so an AVL run
/// that does not cover the required lift is rejected instead of extrapolated.
fn interpolate_avl_at_lift(polar: &AvlPolar, target_cl: f64) -> Option<AvlPolarPoint> {
    if !target_cl.is_finite() {
        return None;
    }
    for point in &polar.points {
        if point.lift_coefficient == target_cl && finite_avl_objective_point(point) {
            return Some(*point);
        }
    }
    for pair in polar.points.windows(2) {
        let [left, right] = pair else {
            continue;
        };
        let delta_cl = right.lift_coefficient - left.lift_coefficient;
        if !finite_avl_objective_point(left)
            || !finite_avl_objective_point(right)
            || !delta_cl.is_finite()
            || delta_cl == 0.0
            || (target_cl - left.lift_coefficient) * (target_cl - right.lift_coefficient) > 0.0
        {
            continue;
        }
        let fraction = (target_cl - left.lift_coefficient) / delta_cl;
        let lerp = |a: f64, b: f64| a + fraction * (b - a);
        let point = AvlPolarPoint {
            alpha_deg: lerp(left.alpha_deg, right.alpha_deg),
            beta_deg: lerp(left.beta_deg, right.beta_deg),
            mach: lerp(left.mach, right.mach),
            lift_coefficient: target_cl,
            total_drag_coefficient: lerp(left.total_drag_coefficient, right.total_drag_coefficient),
            induced_drag_coefficient: lerp(
                left.induced_drag_coefficient,
                right.induced_drag_coefficient,
            ),
            pitching_moment_coefficient: lerp(
                left.pitching_moment_coefficient,
                right.pitching_moment_coefficient,
            ),
            span_efficiency: match (left.span_efficiency, right.span_efficiency) {
                (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Some(lerp(a, b)),
                _ => None,
            },
        };
        return finite_avl_objective_point(&point).then_some(point);
    }
    None
}

fn finite_avl_objective_point(point: &AvlPolarPoint) -> bool {
    [
        point.alpha_deg,
        point.beta_deg,
        point.mach,
        point.lift_coefficient,
        point.total_drag_coefficient,
        point.induced_drag_coefficient,
        point.pitching_moment_coefficient,
    ]
    .iter()
    .all(|value| value.is_finite())
}

fn wing_area_excess_penalty(
    projected_area_m2: f64,
    maximum_area_m2: f64,
    penalty_scale: f64,
) -> Option<f64> {
    if !projected_area_m2.is_finite()
        || !maximum_area_m2.is_finite()
        || maximum_area_m2 <= 0.0
        || !penalty_scale.is_finite()
    {
        return None;
    }
    let excess_fraction = ((projected_area_m2 - maximum_area_m2) / maximum_area_m2).max(0.0);
    Some(excess_fraction.powi(2) * penalty_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polar() -> AvlPolar {
        AvlPolar {
            reference: alas_aero::avl::AvlReference {
                area_m2: 100.0,
                chord_m: 5.0,
                span_m: 30.0,
                moment_reference_m: [0.0; 3],
            },
            model: alas_aero::avl::AvlModel::ALAS_LIFTING_SURFACES,
            points: vec![
                AvlPolarPoint {
                    alpha_deg: -2.0,
                    beta_deg: 0.0,
                    mach: 0.7,
                    lift_coefficient: 0.2,
                    total_drag_coefficient: 0.03,
                    induced_drag_coefficient: 0.02,
                    pitching_moment_coefficient: -0.04,
                    span_efficiency: Some(0.8),
                },
                AvlPolarPoint {
                    alpha_deg: 2.0,
                    beta_deg: 0.0,
                    mach: 0.7,
                    lift_coefficient: 0.6,
                    total_drag_coefficient: 0.04,
                    induced_drag_coefficient: 0.04,
                    pitching_moment_coefficient: -0.08,
                    span_efficiency: Some(0.9),
                },
            ],
        }
    }

    #[test]
    fn avl_optimization_preserves_a_gradient_above_the_wing_area_limit() {
        assert_eq!(wing_area_excess_penalty(90.0, 100.0, 0.5), Some(0.0));
        assert_eq!(wing_area_excess_penalty(100.0, 100.0, 0.5), Some(0.0));
        let Some(slight_excess) = wing_area_excess_penalty(101.0, 100.0, 0.5) else {
            panic!("finite dimensions should produce a penalty");
        };
        let Some(larger_excess) = wing_area_excess_penalty(110.0, 100.0, 0.5) else {
            panic!("finite dimensions should produce a penalty");
        };
        assert!(slight_excess > 0.0);
        assert!(larger_excess > slight_excess);
        assert_eq!(wing_area_excess_penalty(f64::NAN, 100.0, 0.5), None);
    }

    #[test]
    fn avl_objective_interpolates_induced_drag_at_required_lift() {
        let point = match interpolate_avl_at_lift(&polar(), 0.4) {
            Some(point) => point,
            None => panic!("target is bracketed"),
        };
        assert!((point.alpha_deg - 0.0).abs() < 1.0e-12);
        assert!((point.induced_drag_coefficient - 0.03).abs() < 1.0e-12);
        assert!((point.pitching_moment_coefficient + 0.06).abs() < 1.0e-12);
        assert!((point.lift_coefficient - 0.4).abs() < 1.0e-12);
    }

    #[test]
    fn avl_objective_rejects_required_lift_outside_the_native_polar() {
        assert!(interpolate_avl_at_lift(&polar(), 0.1).is_none());
        assert!(interpolate_avl_at_lift(&polar(), 0.7).is_none());
    }

    #[test]
    fn avl_objective_rejects_non_finite_bracket_coefficients() {
        let mut malformed = polar();
        malformed.points[1].pitching_moment_coefficient = f64::NAN;
        assert!(interpolate_avl_at_lift(&malformed, 0.4).is_none());
    }

    #[test]
    fn both_mode_prefers_a_completed_vlm_branch_when_avl_is_unavailable() {
        let vlm = SolverOptimizationResult {
            solver: SolverKind::Vlm,
            status: SolverOptimizationStatus::Completed,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir: None,
            error: None,
        };
        let avl = SolverOptimizationResult::failed(
            SolverKind::Avl,
            None,
            "AVL executable was not configured",
        );
        let set = SolverOptimizationSet { vlm, avl };

        assert_eq!(
            set.selected(OptimizationSolverMode::Both).map(|r| r.solver),
            Ok(SolverKind::Vlm)
        );
        assert!(set.selected(OptimizationSolverMode::Avl).is_err());
    }

    #[test]
    fn avl_only_mode_never_selects_a_failed_branch_as_a_vlm_fallback() {
        let set = SolverOptimizationSet {
            vlm: SolverOptimizationResult::not_requested(SolverKind::Vlm),
            avl: SolverOptimizationResult::failed(SolverKind::Avl, None, "missing executable"),
        };

        assert!(set.selected(OptimizationSolverMode::Avl).is_err());
    }

    #[test]
    fn an_all_invalid_default_de_branch_is_reported_as_a_typed_pipeline_failure() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.max_iterations = 0;
        config.optimizer.solver.population_size = 1;
        config.optimizer.solver.enforce_physical_constraints = true;
        config.requirements.max_cruise_cl = 0.01;

        let result = run_solver_optimizations(
            &config,
            OptimizationSolverMode::Vlm,
            false,
            Some(42),
            &RunEnvironment::default(),
            &DesignVector::default(),
            None,
            None,
        );

        assert_eq!(result.vlm.status, SolverOptimizationStatus::Failed);
        assert!(result.vlm.design.is_none());
        assert!(result.vlm.optimization.is_none());
        assert!(result
            .vlm
            .error
            .as_deref()
            .is_some_and(|error| error.contains("no feasible design")));
    }
}
