// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit tests for the product pipeline's private configuration boundaries.

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
}

#[test]
fn an_explicit_design_and_bounds_reach_the_desktop_pipeline() {
    let progress = std::sync::Mutex::new(Vec::new());
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
        .run_with_design_space_and_progress(
            &options,
            &RunEnvironment::default(),
            &design,
            &bounds,
            &|message| progress.lock().unwrap().push(message.to_owned()),
        )
        .unwrap_or_else(|error| panic!("desktop pipeline run: {error}"));

    let progress = progress.lock().unwrap();
    assert!(progress.starts_with(&[
        "Validating run configuration".to_owned(),
        "Analysis workspace ready".to_owned(),
    ]));
    assert_eq!(
        progress.last().map(String::as_str),
        Some("Stage 7/7: finalizing artifacts and run manifest")
    );

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
fn enabled_native_mission_is_present_in_a_normal_pipeline_result() {
    let config = AlasConfig::default();
    let pipeline = DesignPipeline::new(config);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(1),
        quiet: true,
    };
    let injected_route = Route::new(
        vec![
            alas_route::route::Waypoint::named(40.47, -3.56, "LEMD"),
            alas_route::route::Waypoint::named(45.0, -2.0, "FIXTURE"),
            alas_route::route::Waypoint::named(51.15, -0.19, "EGKK"),
        ],
        alas_route::route::RouteSource::SimbriefApi,
    );
    let result = pipeline
        .run_with_environment_and_route(
            &options,
            &RunEnvironment::default(),
            Some(injected_route.clone()),
        )
        .unwrap_or_else(|error| panic!("normal native pipeline run: {error}"));
    assert_eq!(result.route.as_ref(), Some(&injected_route));
    let mission = result
        .mission_result
        .as_ref()
        .unwrap_or_else(|| panic!("enabled mission must produce telemetry"));
    assert!(!mission.segments.is_empty());
    assert!(
        mission.solutions.iter().all(|solution| solution.converged),
        "enabled public-path mission must converge: {:?}",
        mission.solutions
    );
    assert!(mission.initial_mass_kg() > mission.final_mass_kg());
    assert!(mission.fuel_burned_kg() > 0.0);
    assert_eq!(result.execution.seed_requested, Some(1));
    assert!(!result.execution.seed_applied);
    assert!(!result.execution.parallel_effective);
}

#[test]
fn enabled_mission_does_not_silently_succeed_without_a_route() {
    let config = AlasConfig {
        departure_airport: "not an airport".to_owned(),
        ..AlasConfig::default()
    };
    let pipeline = DesignPipeline::new(config);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(1),
        quiet: true,
    };
    let error = pipeline
        .run(&options, &RunEnvironment::default())
        .err()
        .unwrap_or_else(|| panic!("enabled mission must not return a result without a route"));
    assert!(
        error.contains("route planning"),
        "unexpected error: {error}"
    );
}

#[test]
fn disabled_mission_does_not_publish_a_route() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    let pipeline = DesignPipeline::new(config);
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
    let result = pipeline
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| {
            panic!("disabled mission does not affect the aerodynamic run: {error}")
        });
    assert!(result.route.is_none());
    assert!(result.mission_result.is_none());
}

#[test]
fn parallel_downstream_stages_preserve_the_serial_analysis_result() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(17),
        quiet: true,
    };
    let serial = DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| panic!("serial pipeline run: {error}"));
    let parallel = DesignPipeline::new(config)
        .run(
            &PipelineOptions {
                parallel: true,
                ..options
            },
            &RunEnvironment::default(),
        )
        .unwrap_or_else(|error| panic!("parallel pipeline run: {error}"));

    assert_eq!(parallel.optimized_design, serial.optimized_design);
    assert_eq!(parallel.optimized_report, serial.optimized_report);
    assert_eq!(parallel.baseline_analysis, serial.baseline_analysis);
    assert_eq!(
        parallel.baseline_analysis_error,
        serial.baseline_analysis_error
    );
    assert!(
        serial.baseline_analysis_error.is_none(),
        "a completed baseline comparison must not carry a hidden layout error"
    );
    assert!(
        serial
            .baseline_analysis
            .as_ref()
            .is_some_and(|report| report.payload_layout.is_some()),
        "a completed full baseline report must retain its detailed payload layout"
    );
    assert!(
        serial
            .baseline_report
            .as_ref()
            .is_some_and(|report| report.status == "ok" && report.payload_layout.is_some()),
        "the fast baseline must also expose a resolved detailed payload layout"
    );
    assert!(!serial.execution.parallel_effective);
    assert!(parallel.execution.parallel_effective);
}

#[test]
fn enabled_mses_without_a_tool_retains_the_corrected_section_condition() {
    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.mses.enabled = true;
    let freestream_mach = config.requirements.cruise_mach;
    let inboard_twist_deg = config
        .geometry
        .wing
        .inboard_aerodynamic_station(&DesignVector::default())
        .unwrap_or_else(|error| panic!("the default inboard station is valid: {error}"))
        .twist_deg;
    let pipeline = DesignPipeline::new(config);
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
    let result = pipeline
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| {
            panic!("missing optional MSES must not abort the pipeline: {error}")
        });
    let mses = result
        .mses_result
        .unwrap_or_else(|| panic!("MSES status is retained"));
    let pressure = result
        .mses_pressure
        .unwrap_or_else(|| panic!("MSES pressure status is retained"));
    let report = result
        .optimized_report
        .unwrap_or_else(|| panic!("MSES condition uses the analyzed report"));
    let expected_mach = freestream_mach * report.design.sweep_deg.to_radians().cos();
    let body_alpha_deg = report
        .trimmed_design_point
        .map_or(report.design_point.alpha_deg, |trim| {
            trim.geometric_body_alpha_deg
        });
    let expected_induced_angle_deg = mean_induced_angle_deg(
        report
            .trimmed_design_point
            .map_or(report.design_point.cl, |trim| trim.cl),
        report.polar_fit.aspect_ratio,
        report.polar_fit.oswald_e,
    );
    let expected_alpha = body_alpha_deg + inboard_twist_deg - expected_induced_angle_deg;

    assert_eq!(mses.status, alas_aero::mses::MsesStatus::Absent);
    assert!(mses
        .error
        .as_deref()
        .is_some_and(|error| error.contains("not configured")));
    assert!((mses.mach - expected_mach).abs() < 1e-12);
    assert!((pressure.alpha_deg - expected_alpha).abs() < 1e-12);
}

#[test]
fn the_section_incidence_proxy_removes_finite_wing_downwash() {
    let angle = mean_induced_angle_deg(0.5, 9.0, 0.85);

    assert!(angle > 0.0);
    assert!(angle < 2.0, "angle={angle}");
    assert_eq!(mean_induced_angle_deg(0.5, 0.0, 0.85), 0.0);
}
