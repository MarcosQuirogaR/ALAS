// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless state and persistence regression tests.

use super::super::*;
use super::persistence::{
    allocate_case_directory, environment_preferences_path, CfdEnvironmentPreferences,
};
use alas_cfd::{AirfoilSnapshot, CfdOutcome, CfdResults, CfdStudyConfig, OperatingInput};
use alas_exec::openfoam::OpenFoamPreferences;
use alas_exec::ToolLocator;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
#[test]
fn selecting_airfoil_only_changes_the_study_and_invalidates_results() {
    let mut state = AirfoilCfdState::default();
    state.result = Some(test_result_fixture());
    let revision = state.input_revision;
    assert!(state.select_airfoil("rae2822"));
    assert_eq!(state.selected_airfoil(), "rae2822");
    assert!(state.result.is_none());
    assert!(state.input_revision > revision);
    assert!(state.preview_coordinates.is_some());
}

#[test]
fn unknown_airfoil_never_substitutes_preview_geometry() {
    let mut state = AirfoilCfdState::default();
    state.select_airfoil("rae2822");
    let before = state.preview_coordinates.clone();
    assert!(!state.select_airfoil("missing-cfd-section"));
    assert_eq!(state.selected_airfoil(), "rae2822");
    assert!(state.preview_coordinates.is_none());
    assert_ne!(state.preview_coordinates, before);
}

#[test]
fn default_filter_contains_the_database_and_is_searchable() {
    let mut state = AirfoilCfdState::default();
    assert!(state.filtered_airfoils.len() > 100);
    state.set_airfoil_filter("rae2822");
    assert_eq!(state.filtered_airfoils, vec!["rae2822".to_owned()]);
}

#[test]
fn validation_rejects_a_run_without_spawning_a_worker() {
    let mut state = AirfoilCfdState::default();
    state.config.chord_m = 0.0;
    let result = state.start_run();
    assert!(result.is_err());
    assert!(!state.running);
    assert!(state.error.is_some());
}

#[test]
fn cancellation_is_safe_when_idle_and_sets_the_owned_flag_when_running() {
    let mut state = AirfoilCfdState::default();
    state.cancel_run();
    assert!(!state.cancel_flag.load(Ordering::Relaxed));
    state.running = true;
    state.cancel_run();
    assert!(state.cancel_flag.load(Ordering::Relaxed));
}

#[test]
fn late_finished_run_clears_busy_state_without_installing_stale_result() {
    let mut state = AirfoilCfdState::default();
    state.running = true;
    state.run_id = 9;
    state.input_revision = 4;
    let (tx, rx) = channel();
    state.rx = Some(rx);
    tx.send(CfdWorkerMessage::Finished {
        run_id: 9,
        input_revision: 3,
        result: Ok(test_result_fixture()),
    })
    .expect("test channel receiver is alive");

    state.poll();

    assert!(!state.running);
    assert!(state.result.is_none());
    assert!(state
        .status
        .contains("discarded because its inputs changed"));
}

#[test]
fn sweep_values_include_endpoints_and_support_descending_ranges() {
    let settings = CfdSweepSettings::default();
    assert_eq!(
        settings.values().unwrap_or_default(),
        vec![-4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0]
    );
    let descending = CfdSweepSettings {
        start: 12.0,
        end: -4.0,
        step: 2.0,
        ..settings
    };
    let values = descending.values().unwrap_or_default();
    assert_eq!(values.first().copied(), Some(12.0));
    assert_eq!(values.last().copied(), Some(-4.0));
    assert_eq!(values.len(), 9);
}

#[test]
fn sweep_reynolds_point_uses_the_declared_independent_input() {
    let settings = CfdSweepSettings {
        variable: CfdSweepVariable::Reynolds,
        start: 2.0e5,
        end: 2.0e5,
        step: 1.0e4,
    };
    let point = settings.config_for_value(&CfdStudyConfig::default(), 2.0e5);
    assert_eq!(point.operating_input, OperatingInput::Reynolds);
    assert_eq!(point.reynolds, 2.0e5);
    assert!((point.effective_reynolds() - 2.0e5).abs() < 1.0e-9);
}

#[test]
fn sweep_poll_retains_point_evidence_and_marks_unstarted_points_cancelled() {
    let mut state = AirfoilCfdState::default();
    let config = CfdStudyConfig::default();
    state.run_id = 17;
    state.input_revision = 3;
    state.running = true;
    state.sweep_running = true;
    state.cancel_flag.store(true, Ordering::Relaxed);
    state.sweep_results = vec![
        CfdSweepPointResult::pending(0, -2.0, config.clone()),
        CfdSweepPointResult::pending(1, 0.0, config.clone()),
    ];
    let (tx, rx) = channel();
    state.rx = Some(rx);
    tx.send(CfdWorkerMessage::SweepPointStarted {
        run_id: 17,
        input_revision: 3,
        index: 0,
        value: -2.0,
        config,
    })
    .expect("sweep start message receiver is alive");
    tx.send(CfdWorkerMessage::SweepPointFinished {
        run_id: 17,
        input_revision: 3,
        index: 0,
        result: Ok(test_result_fixture()),
    })
    .expect("sweep result message receiver is alive");
    tx.send(CfdWorkerMessage::SweepFinished {
        run_id: 17,
        input_revision: 3,
    })
    .expect("sweep finish message receiver is alive");

    state.poll();

    assert!(!state.running);
    assert!(!state.sweep_running);
    assert_eq!(
        state.sweep_results[0].status,
        CfdSweepPointStatus::Finished(CfdOutcome::Unconverged)
    );
    assert!(state.sweep_results[0].result.is_some());
    assert_eq!(
        state.sweep_results[1].status,
        CfdSweepPointStatus::Cancelled
    );
}

#[test]
fn startup_restores_cfd_environment_paths_for_the_worker() {
    let root = std::env::temp_dir().join(format!("alas-cfd-gui-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let locator = ToolLocator::new(root.join("app"), root.join("prefs"));
    let path = environment_preferences_path(&locator);
    std::fs::create_dir_all(path.parent().unwrap_or(root.as_path())).expect("create test prefs");
    let mut openfoam = OpenFoamPreferences::default();
    openfoam.native_bin_dir = Some("C:/OpenFOAM/bin".to_owned());
    openfoam.native_project_dir = Some("C:/OpenFOAM/project".to_owned());
    openfoam.gmsh_executable = Some("C:/tools/gmsh.exe".to_owned());
    let preferences = CfdEnvironmentPreferences {
        openfoam,
        gmsh_executable: None,
        paraview_executable: Some("C:/tools/paraview.exe".to_owned()),
    };
    let text = serde_json::to_string(&preferences).expect("encode test prefs");
    std::fs::write(&path, text).expect("write test prefs");

    let state = AirfoilCfdState::new(&locator);

    assert_eq!(
        state.openfoam_preferences.native_bin_dir.as_deref(),
        Some("C:/OpenFOAM/bin")
    );
    assert_eq!(
        state.openfoam_preferences.gmsh_executable.as_deref(),
        Some("C:/tools/gmsh.exe")
    );
    assert_eq!(state.gmsh_executable.as_deref(), Some("C:/tools/gmsh.exe"));
    assert_eq!(
        state.paraview_executable.as_deref(),
        Some("C:/tools/paraview.exe")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn changing_environment_inputs_discards_the_previous_probe_result() {
    let mut state = AirfoilCfdState::default();
    state.capabilities = Some(alas_exec::openfoam::OpenFoamCapabilities {
        backend: alas_exec::openfoam::OpenFoamBackend::Native,
        version: Some("OpenFOAM-test".to_owned()),
        commands: std::collections::BTreeMap::new(),
        available: true,
        detail: "test probe".to_owned(),
    });
    state.status = "Native Windows (OpenFOAM-test)".to_owned();
    let (tx, rx) = channel();
    state.probing = true;
    state.probe_rx = Some(rx);
    tx.send(ProbeMessage::Finished(
        state.capabilities.clone().expect("test capability"),
    ))
    .expect("probe receiver is alive");

    state.mark_environment_changed();
    state.poll();

    assert!(state.capabilities.is_none());
    assert!(!state.probing);
    assert_eq!(state.status, "OpenFOAM connection has not been checked.");
}

#[test]
fn default_state_can_save_relative_study_and_sweep_paths() {
    let suffix = format!(
        "alas-cfd-gui-save-{}-{}",
        std::process::id(),
        NEXT_CASE_COUNTER.load(Ordering::Relaxed)
    );
    let study_path = PathBuf::from(format!(".{suffix}-study.json"));
    let sweep_path = PathBuf::from(format!(".{suffix}-sweep.json"));
    let _ = std::fs::remove_file(&study_path);
    let _ = std::fs::remove_file(&sweep_path);

    let mut state = AirfoilCfdState::default();
    state.saved_study_path = study_path.clone();
    state.saved_sweep_path = sweep_path.clone();
    let saved = state.save_study().expect("save study settings");

    assert_eq!(saved, study_path);
    assert!(study_path.is_file());
    assert!(sweep_path.is_file());
    let _ = std::fs::remove_file(study_path);
    let _ = std::fs::remove_file(sweep_path);
}

#[test]
fn independent_states_allocate_distinct_case_directories_and_retain_old_cases() {
    let root = std::env::temp_dir().join(format!(
        "alas-cfd-gui-case-allocation-{}-{}",
        std::process::id(),
        NEXT_CASE_COUNTER.load(Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);

    // Each state starts its run counter from zero, as a freshly launched
    // application would.  The durable directory identity must therefore
    // come from allocation, not from `run_id`.
    let first_state = AirfoilCfdState::default();
    let first_case = allocate_case_directory(&root, first_state.run_id + 1)
        .expect("first isolated case directory");
    let evidence = first_case.join("results.json");
    std::fs::write(&evidence, b"first run evidence").expect("write first case evidence");

    let second_state = AirfoilCfdState::default();
    let second_case = allocate_case_directory(&root, second_state.run_id + 1)
        .expect("second isolated case directory");

    assert_ne!(first_case, second_case);
    assert!(evidence.is_file());
    let case_count = std::fs::read_dir(&root)
        .expect("read case root")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    assert_eq!(case_count, 2);
    let _ = std::fs::remove_dir_all(root);
}

// The result is only used to exercise invalidation.  Constructing a full
// provenance record here would couple the UI state tests to solver output.
fn test_result_fixture() -> CfdResults {
    let config = CfdStudyConfig::default();
    let airfoil =
        alas_cfd::resolve_airfoil(&config.airfoil_name).unwrap_or_else(|_| AirfoilSnapshot {
            name: config.airfoil_name.clone(),
            coordinates: Vec::new(),
            coordinate_hash: String::new(),
        });
    CfdResults {
        outcome: CfdOutcome::Unconverged,
        case_dir: PathBuf::from("test-case"),
        provenance: alas_cfd::StudyProvenance {
            template_version: alas_cfd::TEMPLATE_VERSION.to_owned(),
            config: config.clone(),
            airfoil,
            effective_speed_m_s: config.effective_speed_m_s(),
            effective_reynolds: config.effective_reynolds(),
            frame: alas_cfd::FrameConvention::default(),
            backend: None,
            openfoam_version: None,
            file_hashes: std::collections::BTreeMap::new(),
        },
        residuals: Vec::new(),
        forces: Vec::new(),
        mass_balance: Vec::new(),
        mesh_quality: alas_cfd::MeshQuality::default(),
        fields: Vec::new(),
        surface: None,
        surface_error: None,
        command_logs: std::collections::BTreeMap::new(),
        status_detail: String::new(),
    }
}
