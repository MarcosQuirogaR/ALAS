// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
fn both_mode_prefers_a_feasible_design_over_an_infeasible_one() {
    use SolverOptimizationStatus::{Completed, Infeasible};
    let branch = |solver, status| {
        let mut result = SolverOptimizationResult::not_requested(solver);
        result.status = status;
        result
    };
    for (vlm, avl, expected) in [
        (Infeasible, Completed, SolverKind::Avl),
        (Completed, Infeasible, SolverKind::Vlm),
        (Completed, Completed, SolverKind::Vlm),
        (Infeasible, Infeasible, SolverKind::Vlm),
    ] {
        let set = SolverOptimizationSet {
            vlm: branch(SolverKind::Vlm, vlm),
            avl: branch(SolverKind::Avl, avl),
        };
        assert_eq!(
            set.selected(OptimizationSolverMode::Both).map(|r| r.solver),
            Ok(expected),
            "{vlm:?} / {avl:?}"
        );
    }
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
    config.optimizer.solver.refinement.max_evaluations = 24;
    config.optimizer.solver.screening.max_evaluations = 8;
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
    config.optimizer.solver.refinement.max_evaluations = 496;
    config.optimizer.solver.screening.max_evaluations = 8;
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
    config.optimizer.solver.refinement.max_evaluations = 496;
    config.optimizer.solver.screening.max_evaluations = 8;
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
    config.optimizer.solver.refinement.max_evaluations = 80;
    config.optimizer.solver.screening.max_evaluations = 8;
    // The longest stage limit, so a slow serial initial population on a
    // loaded machine cannot end the refinement before its first generation.
    config.optimizer.solver.refinement.time_limit_s = 300.0;
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
    // `--no-parallel` decides both whether the VLM and AVL
    // branches run side by side and the batch width. A user who asks for a serial run gets
    // one candidate evaluated at a time as well, whatever the automatic
    // worker count would have resolved to on this machine.
    let mut config = AlasConfig::default();
    config.optimizer.solver.workers = 0;
    assert!(
        config.optimizer.solver.resolved_workers() >= 1,
        "the automatic setting (0) resolves against the machine"
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

fn delivered_fixture(best_valid: bool, verified: Option<bool>) -> alas_opt::OptimizationResult {
    let mut result = alas_opt::OptimizationResult {
        best_design: DesignVector::default(),
        best_cost: 1.0,
        best_valid,
        history: alas_opt::OptimizationHistory::new(),
        wall_time_s: 1.0,
        method: "differential_evolution".to_owned(),
        strategy: String::new(),
        termination: "evaluation_budget".to_owned(),
        pareto_front: Vec::new(),
        search_diagnostics: None,
        delivered_acceptance: None,
    };
    if let Some(verified) = verified {
        result.record_delivered_acceptance(alas_opt::DeliveredAcceptance {
            verified,
            finalist_rejected_by: Vec::new(),
            delivered_rejected_by: Vec::new(),
            rejection_messages: Vec::new(),
            candidates_evaluated: 1,
            delivered_is_search_finalist: true,
            wall_time_s: 0.0,
            analyses: 1,
            baseline: None,
        });
    }
    result
}

#[test]
fn an_infeasible_delivered_design_never_reads_as_completed() {
    for (valid, verified) in [(false, None), (true, Some(false)), (false, Some(true))] {
        let result = delivered_fixture(valid, verified);
        assert!(!result.is_delivered_feasible());
        let status = SolverOptimizationStatus::for_delivered(&result);
        assert_ne!(status, SolverOptimizationStatus::Completed);
        assert!(status.has_design(), "the design stays readable");
        assert!(!status.label().eq_ignore_ascii_case("completed"));
        let (stage_word, _) = crate::pipeline::optimization_stage_status(Some(&result));
        assert!(!stage_word.eq_ignore_ascii_case("completed"));
    }
    let feasible = delivered_fixture(true, Some(true));
    assert_eq!(
        SolverOptimizationStatus::for_delivered(&feasible),
        SolverOptimizationStatus::Completed
    );
}

#[test]
fn an_infeasible_branch_is_still_selectable_but_not_completed() {
    let mut vlm = SolverOptimizationResult::not_requested(SolverKind::Vlm);
    vlm.status = SolverOptimizationStatus::Infeasible;
    let set = SolverOptimizationSet {
        vlm,
        avl: SolverOptimizationResult::not_requested(SolverKind::Avl),
    };
    let selected = set.selected(OptimizationSolverMode::Vlm);
    assert_eq!(
        selected.map(|branch| branch.status),
        Ok(SolverOptimizationStatus::Infeasible)
    );
}

#[test]
fn an_avl_candidate_is_drawn_with_the_tail_the_assessment_sizes() {
    let preset = "A320-200";
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .expect("valid configuration");
    let mut moved = alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector;
    moved.span_m *= 0.95;
    moved.root_chord_m *= 1.07;
    moved.break_chord_m *= 1.07;
    moved.tip_chord_m *= 1.07;

    let (avl_config, avl_design) =
        avl::sized_candidate(&config, &moved, false).expect("resolvable candidate");
    let assessment =
        alas_opt::assess_product_candidate(&config, &moved).expect("assessable candidate");
    let sizing = assessment.resolved.tail_sizing;

    assert!(
        (avl_design.tail_scale - sizing.tail_scale).abs() <= 1e-12 * sizing.tail_scale.abs(),
        "tail scale {} vs {}",
        avl_design.tail_scale,
        sizing.tail_scale
    );
    let ratio = avl_config.geometry.empennage.vstab_scale_ratio;
    assert!(
        (ratio - sizing.vstab_scale_ratio).abs() <= 1e-12 * sizing.vstab_scale_ratio.abs(),
        "fin ratio {ratio} vs {}",
        sizing.vstab_scale_ratio
    );
    assert!(
        (avl_design.tail_scale - moved.tail_scale).abs() > 1e-6,
        "the moved wing must resize the tail for this check to mean anything"
    );
}
