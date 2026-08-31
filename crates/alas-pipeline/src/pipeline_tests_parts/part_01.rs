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
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        ..AlasConfig::default()
    };
    if let Some(mass_model) = preset.mass_model.clone() {
        config.mass_model = mass_model;
    }
    if let Some(performance) = preset.performance.clone() {
        config.performance = performance;
    }
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
    assert!(result.finalist_evaluations.is_empty());
    assert!(result.finalist_audit.is_none());
}

#[test]
fn optimized_pipeline_delivers_only_a_native_reviewed_finalist() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.requirements.max_wing_area_m2 = 2_000.0;
    config.requirements.cg_range_pct_mac = 100.0;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.display_progress = false;
    config.optimizer.solver.finalist_count = 1;
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
    assert_eq!(
        result.feasibility.verdict(),
        crate::FeasibilityVerdict::Feasible
    );
    assert_eq!(result.finalist_evaluations.len(), 1);
    assert_eq!(
        result.finalist_evaluations[0].outcome,
        crate::FinalistOutcome::Selected
    );
    assert_eq!(
        result.finalist_evaluations[0].verdict,
        Some(result.feasibility.verdict())
    );
    assert_eq!(
        result.finalist_evaluations[0].finding_codes,
        result
            .feasibility
            .findings
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>()
    );
    assert!(result
        .finalist_audit
        .as_ref()
        .is_some_and(|path| path.is_file()));
}

#[test]
fn optimized_pipeline_never_falls_back_to_a_native_infeasible_screening_winner() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.requirements.max_wing_area_m2 = 2_000.0;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.display_progress = false;
    config.optimizer.solver.finalist_count = 1;
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

    assert!(error.contains("no native finalist passed"), "{error}");
    assert!(error.contains("#1 infeasible"), "{error}");
    assert!(error.contains("audit retained at"), "{error}");
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
        .filter(|event| event.kind == crate::RunEventKind::StageStarted)
        .count();
    let finishes = events
        .iter()
        .filter(|event| event.kind == crate::RunEventKind::StageCompleted)
        .count();
    assert_eq!(starts, 7);
    assert_eq!(finishes, 7);
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
