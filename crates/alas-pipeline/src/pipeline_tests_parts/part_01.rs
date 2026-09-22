// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn successful_mses_exports_retain_the_verbatim_mplot_tables() {
    let output =
        std::env::temp_dir().join(format!("alas-mses-raw-retention-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output);
    let result = MsesPressureResult {
        raw_bl_dump: "BL source\n".to_owned(),
        raw_flowfield_dump: "flowfield source\n".to_owned(),
        ..MsesPressureResult::default()
    };

    persist_mses_raw_exports(&result, &output)
        .unwrap_or_else(|error| panic!("persist MSES raw exports: {error}"));
    assert_eq!(
        std::fs::read_to_string(output.join("mses/bl_dump.txt"))
            .unwrap_or_else(|error| panic!("read retained BL dump: {error}")),
        result.raw_bl_dump
    );
    assert_eq!(
        std::fs::read_to_string(output.join("mses/flowfield.txt"))
            .unwrap_or_else(|error| panic!("read retained flowfield: {error}")),
        result.raw_flowfield_dump
    );
    let diagnostics = std::fs::read_to_string(output.join("mses/pressure_diagnostics.json"))
        .unwrap_or_else(|error| panic!("read retained pressure diagnostics: {error}"));
    assert!(
        diagnostics.contains("\"status\": \"not_run\""),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("\"convergence_verified\": false"),
        "{diagnostics}"
    );
    std::fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}

#[test]
fn partial_mses_polar_exports_each_requested_point_transcript() {
    let output = std::env::temp_dir().join(format!(
        "alas-mses-polar-diagnostic-retention-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output);
    let result = MsesPolarResult {
        status: alas_aero::mses::MsesStatus::PartialConvergence,
        requested_alpha_count: 2,
        converged_alpha_count: 1,
        point_diagnostics: vec![
            alas_aero::mses::MsesPolarPointDiagnostic {
                requested_alpha_deg: 1.0,
                status: alas_aero::mses::MsesPolarPointStatus::Converged,
                solver_output: "Converged on tolerance".to_owned(),
            },
            alas_aero::mses::MsesPolarPointDiagnostic {
                requested_alpha_deg: 2.0,
                status: alas_aero::mses::MsesPolarPointStatus::NotConverged,
                solver_output: "Iteration limit reached".to_owned(),
            },
        ],
        ..MsesPolarResult::default()
    };

    persist_mses_polar_diagnostics(&result, &output)
        .unwrap_or_else(|error| panic!("persist MSES polar diagnostics: {error}"));
    let text = std::fs::read_to_string(output.join("mses/polar_diagnostics.json"))
        .unwrap_or_else(|error| panic!("read retained polar diagnostics: {error}"));
    assert!(text.contains("\"requested_alpha_deg\": 2.0"), "{text}");
    assert!(text.contains("\"status\": \"not_converged\""), "{text}");
    assert!(text.contains("Iteration limit reached"), "{text}");
    std::fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}

#[test]
fn a_pipeline_seed_reaches_the_optimizer_configuration() {
    let config = AlasConfig::default();
    let effective = optimizer_config(&config, Some(42))
        .unwrap_or_else(|error| panic!("seed is representable: {error}"));
    assert_eq!(effective.optimizer.solver.seed, Some(42));
    assert_eq!(config.optimizer.solver.seed, None);
}

#[test]
fn an_unrepresentable_pipeline_seed_is_rejected_before_optimization() {
    let error = optimizer_config(&AlasConfig::default(), Some(u64::MAX))
        .err()
        .unwrap_or_else(|| panic!("the optimizer configuration stores signed seeds"));
    assert!(error.contains("seed"));
}

#[test]
fn a_named_preset_is_the_public_nominal_design() {
    let preset =
        presets::get("A220-300").unwrap_or_else(|error| panic!("registered preset: {error}"));
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset.name
    }))
    .unwrap_or_else(|error| panic!("load preset configuration: {error}"));
    config.mission.enabled = false;
    config.structures.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| panic!("preset pipeline run: {error}"));

    assert_eq!(result.optimized_design, Some(preset.design_vector));
    assert!(
        result
            .optimized_report
            .as_ref()
            .is_some_and(|report| report.component_masses["Fuel"] > 0.0),
        "the A220 carries positive fuel after its selected engine reaches mass analysis"
    );
    assert_eq!(
        result.feasibility.fuel_loading.usable_capacity,
        crate::feasibility::FuelCapacityAssessment {
            capacity_kg: preset.reference.usable_fuel_mass_kg,
            evidence: crate::feasibility::FuelCapacityEvidence::PublishedPreset,
        }
    );
}

/// A fixed-design run with the native physical review enabled.
fn reviewed_fixed_design_config() -> AlasConfig {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.requirements.max_wing_area_m2 = 2_000.0;
    config.requirements.cg_range_pct_mac = 100.0;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.display_progress = false;
    config
}

/// A pinned design that the native physical review can still report on must
/// not be promoted as a finalist, and the solver branch must name why.
///
/// **The binding constraint moved, and the move is the evidence, not a
/// regression.** This test used to name `wing_loading`: the default design
/// sat below `min_wing_loading_kg_m2`. It no longer does, because the wingbox
/// is now solved at the structural design gross mass and the sizing mission
/// flies the route's own cruise altitude, which together changed the sized
/// mass the wing loading is formed from. What binds instead is
/// `geometric_body_alpha`, the clean-sheet transport's 2-4 degree cruise body
/// attitude window: a design target on the trimmed aircraft, applied only in
/// clean-sheet mode, and the sole violated hard residual on this design
/// (measured: `violated_hard_ids() == ["geometric_body_alpha"]`). The test
/// therefore pins the structure it was written for (reviewable, not
/// promotable, with a named reason) and the residual that is actually
/// binding today, rather than a reason that no longer applies.
#[test]
fn fixed_design_review_exposes_its_binding_constraint_without_promoting_a_finalist() {
    let mut config = reviewed_fixed_design_config();
    // The corrected pair-rated exit layout seats the complete default
    // 350-passenger brief at the pinned shell. Keep this review load case at
    // that exact target so the test isolates the independent mass-feasibility
    // finding below.
    config.requirements.num_passengers = 350;
    let design = DesignVector::default();
    let bounds = design
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect::<Vec<_>>();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: crate::AerodynamicSolverMode::Vlm,
        optimization_solver: crate::OptimizationSolverMode::Vlm,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .unwrap_or_else(|error| panic!("fixed-design finalist run: {error}"));

    assert_eq!(result.optimized_design, Some(design));
    assert!(
        result.feasibility.is_feasible(),
        "the physical review remains usable; the sizing constraint is retained on the solver branch"
    );
    assert!(result.solver_optimizations.as_ref().is_some_and(|set| {
        set.vlm.status == crate::SolverOptimizationStatus::Failed
            && set
                .vlm
                .error
                .as_deref()
                .is_some_and(|error| error.contains("geometric_body_alpha"))
    }));
    assert!(result.optimization_result.is_none());
}

/// A percentage-mode passenger brief does not constrain the cabin capacity.
///
/// This test used to assert the opposite: that a brief the pinned shell could
/// not seat produced a `PassengerCapacityShortfall`. It does not, and the
/// reason is a deliberate architecture rather than a regression in the
/// finding. `alas_payload::build::build_payload_layout` documents that "every
/// study ... is sized from its class shares and fills the usable floor; there
/// is no explicit passenger target for the solver to hit", and it enforces
/// that by rewriting `requirements.num_passengers` to whatever the layout
/// seated, up to four times, until no passenger is unseated.
///
/// The consequence is measured, not inferred:
/// `cargo run -p alas-pipeline --example seating_capacity_probe` asks the
/// default shell for 350, 400, 500, 700, 900 and 1500 passengers and gets
/// `total 525 / seated 525 / unseated 0` every time. So the brief is absorbed
/// silently at any magnitude, and `PassengerCapacityShortfall` is reachable
/// only through the residual row-packing gap the four passes cannot close.
///
/// An explicit count-mode cabin is a different input: its installed seats
/// bound occupancy, and a shortfall remains visible. This test covers the
/// percentage-mode capacity policy, not that installed-cabin contract.
///
/// Whether a percentage-mode brief should be a requirement the product can fail is a
/// product decision, not one this lane may take on its own, so this test pins
/// the contract that actually ships. If that decision is ever made, this test
/// is the one to invert.
#[test]
fn a_passenger_brief_is_reconciled_to_what_the_cabin_seats_rather_than_constraining_it() {
    let mut config = reviewed_fixed_design_config();
    const OVERSUBSCRIBED_BRIEF: usize = 900;
    let requested = i64::try_from(OVERSUBSCRIBED_BRIEF)
        .expect("a three-digit passenger brief is representable");
    config.requirements.num_passengers = requested;
    config.mass_model.flops_transport.first_class_passenger_count = Some(0);
    config.mass_model.flops_transport.business_class_passenger_count = Some(0);
    config.mass_model.flops_transport.tourist_class_passenger_count = Some(OVERSUBSCRIBED_BRIEF);
    let design = DesignVector::default();
    let bounds = design
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect::<Vec<_>>();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: crate::AerodynamicSolverMode::Vlm,
        optimization_solver: crate::OptimizationSolverMode::Vlm,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .unwrap_or_else(|error| panic!("fixed-design finalist run: {error}"));

    // No shortfall is raised, however far the brief exceeds the cabin.
    assert!(
        !result
            .feasibility
            .findings
            .iter()
            .any(|finding| finding.code == crate::FindingCode::PassengerCapacityShortfall),
        "a shortfall is now reachable for an oversubscribed brief; \
         this test and the layout reconciliation both need revisiting: {:?}",
        result.feasibility.findings
    );

    // The layout seats a full cabin and reports no unseated passenger, and the
    // seated count is strictly below the brief that was asked for - which is
    // exactly the silent absorption described above.
    let Some(alas_payload::layout::LayoutSummary::Passenger(summary)) = result
        .optimized_report
        .as_ref()
        .and_then(|report| report.payload_layout.as_ref())
        .map(|layout| &layout.summary)
    else {
        panic!("the finalist analysis must publish a passenger layout");
    };
    assert_eq!(summary.unseated_pax, 0);
    assert_eq!(summary.total_pax, summary.seated_pax);
    assert!(summary.seated_pax > 0 && summary.seated_pax < requested);

    // The independent half of the original test, which does still hold: a
    // rejected optimizer branch never becomes a finalist or an apparently
    // valid result.
    assert_eq!(result.optimized_design, Some(design));
}

#[test]
fn diagnostic_policies_deliver_a_bounded_baseline_when_requirements_are_missed() {
    // Every requirement family is diagnostic, so the wing-area miss is
    // reported on the finalist rather than making the search infeasible.
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.requirements.max_wing_area_m2 = 1.0;
    let diagnostic = alas_config::ConstraintPolicy::Diagnostic;
    config.optimizer.objective.mass_constraints = diagnostic;
    config.optimizer.objective.balance_constraints = diagnostic;
    config.optimizer.objective.performance_constraints = diagnostic;
    config.optimizer.objective.geometry_constraints = diagnostic;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.display_progress = false;

    let design = DesignVector::default();
    let initial_span = design.to_array()[0];
    let mut bounds = design
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect::<Vec<_>>();
    bounds[0] = (initial_span - 1.0, initial_span + 1.0);
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: crate::AerodynamicSolverMode::Vlm,
        optimization_solver: crate::OptimizationSolverMode::Vlm,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .unwrap_or_else(|error| panic!("diagnostic bounded finalist run: {error}"));

    let optimized = result
        .optimized_design
        .expect("a diagnostic-policy optimization publishes the baseline finalist");
    assert!(optimized
        .to_array()
        .iter()
        .zip(&bounds)
        .all(|(value, &(lower, upper))| *value >= lower && *value <= upper));
    assert!(result
        .optimization_result
        .as_ref()
        .is_some_and(|optimization| optimization.best_valid));
}

#[test]
fn optimized_pipeline_never_falls_back_to_a_native_infeasible_screening_winner() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.requirements.max_wing_area_m2 = 2_000.0;
    config.requirements.max_cruise_cl = 0.0;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.display_progress = false;
    let design = DesignVector::default();
    let bounds = design
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect::<Vec<_>>();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: crate::AerodynamicSolverMode::Vlm,
        optimization_solver: crate::OptimizationSolverMode::Vlm,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    let error = DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect_err("native-infeasible finalist must not be delivered");

    // The mission-sized objective reports the cruise stall guard as its own
    // physical-infeasibility reason, distinct from a numerical trim failure.
    assert!(error.contains("VLM optimization failed"), "{error}");
    assert!(error.contains("no feasible design"), "{error}");
    assert!(error.contains("trim_cruise_cl_exceeds_max"), "{error}");
}

#[test]
fn an_explicit_design_and_bounds_reach_the_desktop_pipeline() {
    let events = std::sync::Mutex::new(Vec::new());
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    let design = DesignVector {
        span_m: 42.0,
        ..DesignVector::default()
    };
    let mut bounds = DesignVector::bounds();
    bounds[0] = (41.0, 43.0);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };

    let result = DesignPipeline::new(config)
        .run_with_design_space_events(
            &options,
            &RunEnvironment::default(),
            &design,
            &bounds,
            &|event| events.lock().unwrap().push(event),
            &cancel,
        )
        .unwrap_or_else(|error| panic!("desktop pipeline run: {error}"));

    let events = events.lock().unwrap();
    let starts = events
        .iter()
        .filter(|event| {
            event.kind == crate::RunEventKind::StageStarted && event.stage_index.is_some()
        })
        .count();
    let finishes = events
        .iter()
        .filter(|event| {
            event.kind == crate::RunEventKind::StageCompleted && event.stage_index.is_some()
        })
        .count();
    assert_eq!(starts, 7);
    assert_eq!(finishes, 7);
    assert!(events.iter().any(|event| {
        event.stage == "downstream/mses" && event.kind == crate::RunEventKind::StageStarted
    }));
    assert!(events.iter().any(|event| {
        event.stage == "downstream/structural" && event.kind == crate::RunEventKind::StageCompleted
    }));
    assert_eq!(
        events
            .last()
            .map(|event| (event.stage.as_str(), event.fraction)),
        Some(("finalization", Some(1.0)))
    );
    assert!(events
        .iter()
        .all(|event| event.fraction.is_none_or(|f| (0.0..=1.0).contains(&f))));

    assert_eq!(result.optimized_design, Some(design));
    let fuselage = result
        .optimized_report
        .as_ref()
        .and_then(|report| report.airplane.fuselages.first())
        .unwrap_or_else(|| panic!("explicit design report has a fuselage"));
    let rebuilt_length_m = fuselage
        .xsecs
        .last()
        .unwrap_or_else(|| panic!("explicit design fuselage has an end section"))
        .xyz_c[0]
        - fuselage
            .xsecs
            .first()
            .unwrap_or_else(|| panic!("explicit design fuselage has a start section"))
            .xyz_c[0];
    assert!(
        (rebuilt_length_m - design.fuselage_length_m).abs() < 1.0e-9,
        "reported geometry must rebuild the literal explicit fuselage: {rebuilt_length_m} vs {}",
        design.fuselage_length_m
    );
    assert_eq!(
        result.feasibility.fuel_loading.usable_capacity.evidence,
        crate::feasibility::FuelCapacityEvidence::GeometryEstimate,
        "an edited preset geometry must not inherit the registered aircraft's published tanks"
    );
    let loading = result.feasibility.fuel_loading;
    let expected_carried_kg = loading.usable_capacity.capacity_kg.map_or(
        loading.mtow_closure_fuel_kg.max(0.0),
        |capacity_kg| {
            capacity_kg
                .max(0.0)
                .min(loading.mtow_closure_fuel_kg.max(0.0))
        },
    );
    assert_eq!(loading.analyzed_carried_fuel_kg, expected_carried_kg);
    assert_eq!(
        loading.analyzed_takeoff_mass_kg,
        loading.zero_fuel_mass_kg + expected_carried_kg
    );
}

#[test]
fn a_desktop_cancellation_stops_at_the_first_safe_boundary() {
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let design = DesignVector::default();
    let bounds = DesignVector::bounds();
    let result = DesignPipeline::new(AlasConfig::default()).run_with_design_space_events(
        &PipelineOptions::default(),
        &RunEnvironment::default(),
        &design,
        &bounds,
        &|_| {},
        &cancel,
    );

    assert_eq!(
        result.unwrap_err(),
        "Cancelled safely at a pipeline stage boundary"
    );
}

#[test]
fn malformed_desktop_bounds_are_rejected_before_optimization() {
    let error = DesignPipeline::new(AlasConfig::default())
        .run_with_design_space(
            &PipelineOptions::default(),
            &RunEnvironment::default(),
            &DesignVector::default(),
            &[(0.0, 1.0)],
        )
        .err()
        .unwrap_or_else(|| panic!("a partial design space must be rejected"));
    assert!(error.contains("bound pairs"), "{error}");
}

#[test]
fn invalid_cross_field_configuration_is_rejected_before_any_stage_runs() {
    let mut config = AlasConfig::default();
    config.requirements.dive_speed_m_s = 50.0;
    config.geometry.empennage.hstab_tip_chord_m = 9.0;
    let error = match DesignPipeline::new(config).run(
        &PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: false,
            output_dir: None,
            save_plots: false,
            quiet: true,
            ..PipelineOptions::default()
        },
        &RunEnvironment::default(),
    ) {
        Err(error) => error,
        Ok(_) => panic!("blocking configuration validation must stop the public run path"),
    };
    assert!(error.contains("configuration validation failed"), "{error}");
    assert!(error.contains("requirements.dive_speed_m_s"), "{error}");
    assert!(
        error.contains("geometry.empennage.hstab_tip_chord_m"),
        "{error}"
    );
}

#[test]
fn an_unregistered_preset_identity_is_rejected_at_the_public_run_boundary() {
    let config = AlasConfig {
        preset: "Concorde".to_owned(),
        ..AlasConfig::default()
    };
    let error = match DesignPipeline::new(config).run(
        &PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: false,
            output_dir: None,
            save_plots: false,
            quiet: true,
            ..PipelineOptions::default()
        },
        &RunEnvironment::default(),
    ) {
        Err(error) => error,
        Ok(_) => panic!("an unsupported preset must not run generic defaults"),
    };
    assert!(
        error.contains("preset identity is not registered"),
        "{error}"
    );
}

#[test]
fn drifted_preset_geometry_is_rejected_at_dispatch_in_preset_mode_only() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .expect("registered preset loads");
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    config.geometry.wing.root_z_m += 0.5;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        output_dir: None,
        save_plots: false,
        quiet: true,
        ..PipelineOptions::default()
    };
    let error = match DesignPipeline::new(config.clone()).run(&options, &RunEnvironment::default())
    {
        Err(error) => error,
        Ok(_) => panic!("a drifted locked geometry must not run in preset mode"),
    };
    assert!(error.contains("protected in preset mode"), "{error}");
    assert!(error.contains("geometry/wing/root_z_m"), "{error}");
    // The barrier reads the design point and bounds the desktop supplies.
    let preset = presets::get("A320-200").expect("A320");
    let mut clean = config.clone();
    clean.geometry = preset.geometry.clone();
    clean.geometry.engine.apply_engine_spec();
    let mut drifted_design = preset.design_vector;
    drifted_design.span_m += 1.0;
    let error = check_preset_policy(&clean, Some(&drifted_design), None)
        .expect_err("a drifted initial point is rejected");
    assert!(error.contains("span_m"), "{error}");
    assert!(check_preset_policy(&clean, Some(&preset.design_vector), None).is_ok());
    // A clean-sheet study starting from the same shape is not preset mode.
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::CleanSheet;
    assert!(check_preset_policy(&config, None, None).is_ok());
}
