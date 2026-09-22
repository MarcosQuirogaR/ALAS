// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

impl AvlObjective<'_> {
    /// Score one candidate with the mission-sized objective around AVL's
    /// aerodynamics: AVL supplies the induced drag at the required cruise
    /// lift, while the parasite build-up, trim incidence and neutral point
    /// stay the native report's, and the sizing loop closes mass, fuel and
    /// takeoff mass around that fixed polar.
    ///
    /// Cancellation is read twice here: once before the native analysis and
    /// once immediately before the external sweep is launched. A candidate
    /// refused at either point is rejected with a reason that names
    /// cancellation, so a stopped search cannot be read as a design space
    /// where AVL failed.
    fn evaluate_uncached(&self, design: &DesignVector, key: &str) -> ObjectiveEvaluation {
        if self.scope.requested() {
            self.scope
                .work_skipped("AVL candidate not started on the cancellation request");
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cancelled");
        }
        let report = match FullAnalysis::new(self.config.clone()).run(design, true) {
            Ok(report) => report,
            Err(_) => return ObjectiveEvaluation::rejected(self.failure_cost(), "full_analysis"),
        };
        if self.scope.requested() {
            // The deck is not written and no process is spawned: the drain
            // stops here instead of waiting out a sweep nobody will read.
            self.scope
                .external_skipped("AVL sweep not launched on the cancellation request");
            return ObjectiveEvaluation::rejected(self.failure_cost(), "cancelled");
        }
        let evaluation_dir = self.output_root.join(key);
        self.scope
            .enter(alas_opt::CancelPhase::ExternalSolverCall, 0);
        self.scope
            .external_started(format!("AVL sweep for candidate {key}"));
        let avl = run_avl_analysis(
            &report,
            &self.config,
            &evaluation_dir,
            Some(&self.executable),
            AVL_EVALUATION_TIMEOUT_S,
        );
        if avl.status == AvlAnalysisStatus::TimedOut {
            self.scope.external_terminated(format!(
                "AVL sweep for candidate {key} exceeded {AVL_EVALUATION_TIMEOUT_S:.0} s and its \
                 process tree was killed"
            ));
            return ObjectiveEvaluation::rejected(self.failure_cost(), "avl_timed_out");
        }
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
        let cd0 = report.polar_fit.cd0;
        let wave_drag_cd = report
            .polar
            .cl
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (**left - required_cl)
                    .abs()
                    .total_cmp(&(**right - required_cl).abs())
            })
            .and_then(|(index, _)| report.polar.cd_wave.get(index).copied())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let polar = ExternalPolar {
            cd0,
            induced_factor_k: point.induced_drag_coefficient / (required_cl * required_cl),
            wave_drag_cd,
            lift_to_drag: required_cl / (cd0 + wave_drag_cd + point.induced_drag_coefficient),
            alpha_deg: point.alpha_deg,
            incidence_deg: report
                .trimmed_design_point
                .map_or(0.0, |trim| trim.trim_ih_deg),
            x_np: report.x_neutral_point,
            // Evaluation identity, so this polar cannot be flown at another
            // state: AVL's own run Mach (a mis-commanded run is caught, not
            // relabelled), the requirement altitude (AVL has no atmosphere),
            // and the area the coefficients are referred to.
            mach: point.mach,
            altitude_m: self.config.requirements.cruise_altitude_m,
            reference_area_m2: report.airplane.s_ref,
            target_cl: required_cl,
            source: "avl",
            bracketed: true,
        };
        match assess_candidate_with_polar(&self.objective, &design.to_array(), &polar) {
            Ok(assessment) => ObjectiveEvaluation {
                cost: assessment.cost,
                valid: assessment.hard_feasible,
                l_over_d: polar.lift_to_drag,
                span_m: report.airplane.b_ref,
                alpha_deg: point.alpha_deg,
                area_m2: report.airplane.s_ref,
                trim_ih_deg: polar.incidence_deg,
                reject_reason: assessment.violated_hard_ids().join("+"),
            },
            Err(reason) => ObjectiveEvaluation::rejected(self.failure_cost(), reason),
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

    #[test]
    fn a_cancelled_flag_stops_the_de_branch_short_of_its_generation_budget() {
        // The flag is set before the branch ever starts, so this is
        // deterministic: cancellation is observed at the first generation
        // boundary, well short of the 30-generation budget below, with
        // nothing timed.
        let mut config = AlasConfig::default();
        config.optimizer.solver.max_iterations = 30;
        config.optimizer.solver.population_size = 1;
        let cancel = AtomicBool::new(true);

        let result = run_solver_optimizations(
            &config,
            OptimizationSolverMode::Vlm,
            false,
            Some(3),
            &RunEnvironment::default(),
            &DesignVector::default(),
            None,
            None,
            None,
            Some(&cancel),
        );

        assert_eq!(result.vlm.status, SolverOptimizationStatus::Failed);
        assert!(
            result.vlm.design.is_none(),
            "a cancelled branch must never deliver a design as optimized"
        );
        assert!(
            result
                .vlm
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("Cancelled safely")),
            "{:?} must carry the same prefix the GUI already checks to report a cancelled run",
            result.vlm.error
        );
    }

    /// A cancelled branch must leave the analyses it already paid for behind,
    /// labelled as effort and not as a result.
    #[test]
    fn a_cancelled_branch_persists_a_record_that_claims_nothing() {
        let root = std::env::temp_dir().join(format!(
            "alas-cancelled-search-record-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut config = AlasConfig::default();
        config.optimizer.solver.max_iterations = 30;
        config.optimizer.solver.population_size = 1;
        let cancel = AtomicBool::new(true);

        let result = run_solver_optimizations(
            &config,
            OptimizationSolverMode::Vlm,
            false,
            Some(3),
            &RunEnvironment::default(),
            &DesignVector::default(),
            None,
            Some(root.as_path()),
            None,
            Some(&cancel),
        );

        assert_eq!(result.vlm.status, SolverOptimizationStatus::Failed);
        let record_path = root.join("solvers/vlm").join(CANCELLED_SEARCH_RECORD);
        let bytes = std::fs::read(&record_path)
            .unwrap_or_else(|error| panic!("{} must exist: {error}", record_path.display()));
        let record: serde_json::Value =
            serde_json::from_slice(&bytes).expect("the record is valid JSON");
        assert_eq!(record["record_kind"], "cancelled_search");
        assert_eq!(record["termination"], "cancelled");
        assert_eq!(record["stop_reason"], "cancelled");
        for verdict in ["converged", "delivered_feasible", "best_valid"] {
            assert_eq!(
                record[verdict],
                serde_json::Value::Bool(false),
                "a cancelled run may never claim {verdict}"
            );
        }
        assert!(
            record.get("best_design").is_none() && record.get("design").is_none(),
            "the record must carry no design: {record}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The headline claim, asserted on counts rather than on a clock so it is
    /// deterministic: once the search is inside a Differential Evolution
    /// generation, a cancellation request costs at most the one coupled
    /// analysis already in flight.
    #[test]
    fn a_request_inside_a_de_generation_costs_at_most_one_more_analysis() {
        let mut config = AlasConfig::default();
        // Enough generations that the run cannot finish on its own before the
        // request, small enough that a failure of this test wastes a bounded
        // amount of time rather than an unbounded one.
        config.optimizer.solver.max_iterations = 4;
        config.optimizer.solver.population_size = 1;
        config.optimizer.solver.workers = 1;

        let watch = alas_opt::CancelWatch::new();
        let worker_watch = std::sync::Arc::clone(&watch);
        let worker_config = config.clone();
        let handle = std::thread::spawn(move || {
            run_solver_optimizations(
                &worker_config,
                OptimizationSolverMode::Vlm,
                false,
                Some(7),
                &RunEnvironment::default(),
                &DesignVector::default(),
                None,
                None,
                None,
                Some(worker_watch.flag()),
            )
        });

        // Wait for the search to actually be inside a generation. The wait is
        // on the telemetry's own phase, not on elapsed time, so what the
        // assertion below measures is exactly the boundary it names.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        loop {
            let snapshot = watch.snapshot();
            if snapshot.phase == alas_opt::CancelPhase::DeGeneration
                && snapshot.evaluations_completed > 0
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the default configuration never reached a DE generation; phase {} after {} \
                 analyses",
                snapshot.phase.as_str(),
                snapshot.evaluations_completed
            );
            std::thread::yield_now();
        }
        watch.request_cancellation();
        let result = handle.join().expect("the optimizer worker must not panic");
        watch.mark_worker_joined();

        let snapshot = watch.snapshot();
        assert_eq!(
            snapshot.requested_during,
            alas_opt::CancelPhase::DeGeneration,
            "the request must have landed inside a generation for this bound to mean anything"
        );
        assert!(
            snapshot.evaluations_after_request <= 1,
            "cancellation inside a generation must cost at most the analysis in flight, not the \
             rest of the generation: {} analyses ran after the request",
            snapshot.evaluations_after_request
        );
        assert_eq!(
            snapshot.external_processes_started, 0,
            "the VLM branch spawns no external solver, so nothing can survive it"
        );
        assert_eq!(snapshot.stop_reason, Some(alas_opt::StopReason::Cancelled));
        assert_eq!(result.vlm.status, SolverOptimizationStatus::Failed);
        assert!(
            result.vlm.design.is_none(),
            "a cancelled branch must never deliver a design as optimized"
        );
        assert!(result
            .vlm
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("Cancelled safely")));
    }

    #[test]
    fn a_serial_request_reaches_the_candidate_batch_and_not_only_the_branches() {
        // `--no-parallel` used to decide only whether the VLM and AVL
        // branches ran side by side. A user who asks for a serial run gets
        // one candidate evaluated at a time as well, whatever the automatic
        // worker count would have resolved to on this machine.
        let mut config = AlasConfig::default();
        config.optimizer.solver.workers = 0;
        assert!(
            alas_config::SolverSettings::default().resolved_workers() >= 1,
            "the automatic default resolves against the machine"
        );

        let serial = serial_solver_config(&config, false);
        assert_eq!(serial.optimizer.solver.workers, 1);
        assert_eq!(serial.optimizer.solver.resolved_workers(), 1);

        let parallel = serial_solver_config(&config, true);
        assert_eq!(
            parallel.optimizer.solver.workers, 0,
            "a parallel run keeps the configured automatic setting"
        );
    }

    #[test]
    fn a_serial_request_does_not_overwrite_an_explicit_worker_count_upwards() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.workers = 4;
        assert_eq!(
            serial_solver_config(&config, false)
                .optimizer
                .solver
                .workers,
            1
        );
    }
}
