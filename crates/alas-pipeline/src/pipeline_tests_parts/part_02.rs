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
    let snapshots = std::sync::Mutex::new(Vec::new());
    let parallel = DesignPipeline::new(config)
        .run_inner(
            &PipelineOptions {
                parallel: true,
                ..options
            },
            &RunEnvironment::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&|snapshot| snapshots.lock().unwrap().push(snapshot)),
        )
        .unwrap_or_else(|error| panic!("parallel pipeline run: {error}"));

    let snapshots = snapshots.into_inner().unwrap();
    assert!(
        snapshots.len() > 2,
        "publish component completions, not only joins"
    );
    let first = snapshots.first().unwrap();
    assert!(first.optimized_report.is_some());
    assert!(first.baseline_analysis.is_none());
    assert!(first.flowunsteady_result.is_none());
    let last = snapshots.last().unwrap();
    assert_eq!(last.baseline_analysis, parallel.baseline_analysis);
    assert_eq!(last.vspaero_result, parallel.vspaero_result);
    assert_eq!(last.avl_result, parallel.avl_result);
    assert_eq!(last.flowunsteady_result, parallel.flowunsteady_result);
    assert_eq!(last.mses_result, parallel.mses_result);
    assert_eq!(last.structural_result, parallel.structural_result);
    for pair in snapshots.windows(2) {
        assert!(pair[0].baseline_analysis.is_none() || pair[1].baseline_analysis.is_some());
        assert!(pair[0].flowunsteady_result.is_none() || pair[1].flowunsteady_result.is_some());
    }

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

// User-path regression for the storage ownership sentinel: the real
// output-root producer (`prepare_analysis_workspace`, called from
// `run_inner`) must claim a retained output directory at creation, so
// Manage Storage can inventory and clear it. A directory nobody ran the
// producer on must stay unrecognized end to end, even if `clear_storage`
// is invoked on it directly (the forced-negative control), and the
// temporary scratch fallback used when the caller retains nothing must not
// receive the same claim.
#[test]
fn a_run_created_output_root_is_recognized_end_to_end_and_an_arbitrary_root_is_not() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let base = std::env::temp_dir().join(format!(
        "alas-storage-sentinel-e2e-{}-{stamp}",
        std::process::id()
    ));
    let owned_root = base.join("owned-run-outputs");
    let arbitrary_root = base.join("unrelated-user-folder");
    std::fs::create_dir_all(&arbitrary_root)
        .unwrap_or_else(|error| panic!("create arbitrary root: {error}"));
    std::fs::write(arbitrary_root.join("vacation.txt"), b"not ours")
        .unwrap_or_else(|error| panic!("seed arbitrary root: {error}"));

    // The real user path: create the output root the same way `run_inner`
    // does, not a fixture that pre-seeds an ownership sentinel.
    let created = prepare_analysis_workspace(Some(owned_root.clone()))
        .unwrap_or_else(|error| panic!("analysis workspace creation: {error}"));
    assert_eq!(created, owned_root);
    std::fs::write(owned_root.join("design_database.json"), b"{}")
        .unwrap_or_else(|error| panic!("seed a run artefact: {error}"));

    let locator = ToolLocator::new(base.join("app"), base.join("user"));
    let unused_cfd = base.join("unused-cfd");
    let unused_navdata = base.join("unused-navdata");
    let unused_texture = base.join("unused-texture");
    let temp = base.join("unused-temp");
    let locations = alas_exec::storage::StorageLocations {
        output_dir: &owned_root,
        cfd_case_root: &unused_cfd,
        navdata_dir: &unused_navdata,
        texture_path: &unused_texture,
    };

    let entries = alas_exec::storage::storage_inventory_with_temp(&locator, &locations, &temp);
    let outputs = entries
        .iter()
        .find(|entry| entry.id == StorageCategoryId::GeneratedOutputs)
        .unwrap_or_else(|| panic!("generated-outputs category is always reported"));
    assert!(
        outputs.exists,
        "a root created by the real output-root producer must be inventoried"
    );
    let outcome = alas_exec::storage::clear_storage(outputs);
    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    assert!(!owned_root.join("design_database.json").exists());
    assert!(
        owned_root
            .join(alas_exec::storage::OWNERSHIP_SENTINEL)
            .is_file(),
        "the claim made at creation must survive its own cleanup"
    );

    // Forced-negative control: a directory nobody ran the producer on, both
    // through the normal inventory path and through a direct clear attempt.
    let unowned_locations = alas_exec::storage::StorageLocations {
        output_dir: &arbitrary_root,
        ..locations
    };
    let unowned_entries =
        alas_exec::storage::storage_inventory_with_temp(&locator, &unowned_locations, &temp);
    let unowned_outputs = unowned_entries
        .iter()
        .find(|entry| entry.id == StorageCategoryId::GeneratedOutputs)
        .unwrap_or_else(|| panic!("generated-outputs category is always reported"));
    assert!(
        !unowned_outputs.exists,
        "an arbitrary directory must not be offered for cleanup"
    );
    assert!(unowned_outputs.removable.is_empty());

    let forced_entry = alas_exec::storage::StorageEntry {
        id: StorageCategoryId::GeneratedOutputs,
        label: StorageCategoryId::GeneratedOutputs.label(),
        description: StorageCategoryId::GeneratedOutputs.description(),
        root: arbitrary_root.clone(),
        removable: vec![arbitrary_root.join("vacation.txt")],
        exists: true,
        bytes: 0,
        files: 0,
    };
    let forced_outcome = alas_exec::storage::clear_storage(&forced_entry);
    assert!(
        forced_outcome.removed.is_empty(),
        "an arbitrary directory must not be subjected to cleanup"
    );
    assert!(arbitrary_root.join("vacation.txt").is_file());

    // The temp scratch fallback (caller retains nothing) must not receive
    // the generated-outputs claim; it is already recognized separately by
    // its `alas-analysis-` prefix.
    let scratch = prepare_analysis_workspace(None)
        .unwrap_or_else(|error| panic!("scratch workspace creation: {error}"));
    assert!(!scratch
        .join(alas_exec::storage::OWNERSHIP_SENTINEL)
        .exists());
    let _ = std::fs::remove_dir_all(&scratch);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_headless_run_can_be_cancelled_and_joined_instead_of_being_abandoned() {
    // This is the harness case: a supervisor with a wall-clock guard has no
    // way to stop `DesignPipeline::run`, so it could only abandon its worker.
    // `run_cancellable` is that missing seam. The flag is set before the run
    // starts, so the test is deterministic and times nothing: the run must
    // return at its first cancellation boundary, and the worker must be
    // joinable rather than left running.
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let mut config = AlasConfig::default();
    // A budget large enough that a run which ignored the flag would not
    // finish inside this test, so a pass cannot be an accident of speed.
    config.optimizer.solver.max_iterations = 200;
    config.optimizer.solver.population_size = 6;

    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(7),
        quiet: true,
    };

    let cancel = Arc::new(AtomicBool::new(true));
    let worker_cancel = Arc::clone(&cancel);
    let worker = std::thread::spawn(move || {
        DesignPipeline::new(config).run_cancellable(
            &options,
            &RunEnvironment::default(),
            &worker_cancel,
        )
    });

    let outcome = worker.join().expect("the cancelled worker must be joinable");
    let error = outcome.expect_err("a cancelled run reports cancellation rather than a result");
    assert!(
        error.starts_with("Cancelled safely"),
        "a cancelled run must be distinguishable from a failed one: {error}"
    );
}
