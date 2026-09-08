// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    assert!(!mission
        .solutions
        .last()
        .is_some_and(|solution| solution.throttle_limited));
    assert!(mission
        .segments
        .iter()
        .flat_map(|segment| segment.conditions.throttle.iter())
        .all(|throttle| (0.0..=1.0).contains(throttle)));
    assert!(!result
        .feasibility
        .contains(crate::FindingCode::MissionThrottleLimitViolation));
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

