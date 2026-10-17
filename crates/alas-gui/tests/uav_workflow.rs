// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless contracts for the fixed-wing UAV competition workflow.

// Navigation and catalogue fixtures are required by the page contract under test.
#![allow(clippy::expect_used)]

use alas_gui::nav::{self, PageKind};
use alas_gui::uav::{UavExecutionStatus, UavSection, UavWorkflowOutcome, UavWorkflowState};
use alas_uav::{TopologyAvailability, UavAnalysisPath, UavTopology};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn fixed_wing_uav_is_a_first_class_navigation_page() {
    use_synthetic_tables();
    let page = nav::page("uav").expect("UAV page is registered");
    assert_eq!(page.kind, PageKind::Uav);
    assert_eq!(page.title, "Fixed-Wing UAV");
}

#[test]
fn every_design_convention_has_a_typed_sizing_boundary() {
    use_synthetic_tables();
    assert_eq!(UavTopology::ALL.len(), 4);
    assert_eq!(UavTopology::ALL[0], UavTopology::ConventionalTail);
    for topology in UavTopology::ALL {
        assert!(topology
            .availability(UavAnalysisPath::GeometryExport)
            .is_available());
        assert!(topology
            .availability(UavAnalysisPath::SharedCoreLift)
            .is_available());
    }
    assert!(matches!(
        UavTopology::ConventionalTail.availability(UavAnalysisPath::PreliminaryOptimization),
        TopologyAvailability::Available
    ));
}

#[test]
fn competition_search_with_a_small_budget_completes_and_fills_the_result() {
    use_synthetic_tables();
    let mut state = runnable_state(8);
    assert!(state.start());
    wait_for_worker(&mut state);

    assert_eq!(state.execution, UavExecutionStatus::Completed);
    assert_eq!(state.active_section, UavSection::Results);
    let UavWorkflowOutcome::Competition(result) = &state.outcome else {
        panic!("a completed competition search fills the competition result");
    };
    assert_eq!(result.preset_id, "acc-2026");
    assert!(!result.preset_name.is_empty());
    assert_eq!(result.optimum.evaluations, 8);
    assert!(result.optimum.compatible_bundles > 0);
    if let Some(best) = &result.optimum.best {
        assert!(!best.components.motor_id.is_empty());
        assert!(!best.components.propeller_id.is_empty());
        assert!(best.evaluation.feasible || !best.evaluation.failed_gates.is_empty());
    }
}

#[test]
fn an_unknown_preset_or_a_tiny_budget_is_refused_before_the_search() {
    use_synthetic_tables();
    let mut state = runnable_state(8);
    state.competition.preset_id = "no-such-preset".to_owned();
    assert!(!state.start());
    assert!(matches!(
        &state.execution,
        UavExecutionStatus::Failed(message) if message.contains("no-such-preset")
    ));

    let mut state = runnable_state(2);
    assert!(!state.start());
    assert!(matches!(
        &state.execution,
        UavExecutionStatus::Failed(message) if message.contains("at least 4")
    ));
    assert!(matches!(state.outcome, UavWorkflowOutcome::NotRun));
}

#[test]
fn active_search_cannot_be_started_twice_and_cancels_cooperatively() {
    use_synthetic_tables();
    let mut state = runnable_state(1_000_000);
    assert!(state.start());
    assert!(
        !state.start(),
        "one input gesture starts at most one worker"
    );
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Running(progress)
            if progress.total_candidates == 1_000_000
    ));

    state.cancel();
    assert!(matches!(
        state.execution,
        UavExecutionStatus::CancelRequested(_)
    ));
    wait_for_worker(&mut state);
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Cancelled { .. }
    ));
    assert!(matches!(state.outcome, UavWorkflowOutcome::NotRun));
}

#[test]
fn failed_or_cancelled_restart_retains_the_last_completed_result() {
    use_synthetic_tables();
    let mut state = runnable_state(8);
    assert!(state.start());
    wait_for_worker(&mut state);
    let completed_preset = match &state.outcome {
        UavWorkflowOutcome::Competition(result) => result.preset_id.clone(),
        UavWorkflowOutcome::NotRun => panic!("the first search must complete"),
    };

    state.competition.preset_id = "no-such-preset".to_owned();
    assert!(!state.start());
    assert!(matches!(state.execution, UavExecutionStatus::Failed(_)));
    assert!(matches!(
        &state.outcome,
        UavWorkflowOutcome::Competition(result) if result.preset_id == completed_preset
    ));

    state.competition.preset_id = completed_preset.clone();
    state.evaluations = 1_000_000;
    assert!(state.start());
    state.cancel();
    wait_for_worker(&mut state);
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Cancelled { .. }
    ));
    assert!(matches!(
        &state.outcome,
        UavWorkflowOutcome::Competition(result) if result.preset_id == completed_preset
    ));
}

#[test]
fn competition_labels_have_spanish_catalog_entries() {
    use_synthetic_tables();
    let catalog = alas_i18n::es::desktop_catalog();
    for key in UavSection::ALL
        .into_iter()
        .flat_map(|section| [section.short_title(), section.title(), section.description()])
        .chain([
            "Competition preset",
            "Announced take-off distance",
            "Payload count (ACC cans gate)",
            "Payload volume (XC gate)",
            "Evaluation budget",
            "Random seed",
            "Run competition search",
            "Chosen components",
            "Hard gates",
            "Gates that bind (failed)",
            "No hard gate fails.",
            "Competition search result",
            "Feasible design found: every evaluated hard gate passes.",
            "Differential evolution over compatible propulsion bundles",
            "Search time and evaluation rate depend on the machine and are not reproducible; the seed and budget are.",
            "UAV search completed.",
            "UAV result",
            "Preliminary: not validated against measured flight data",
        ])
    {
        assert!(catalog.contains_key(key), "missing Spanish UAV key: {key}");
    }
}

fn runnable_state(evaluations: usize) -> UavWorkflowState {
    let mut state = UavWorkflowState::default();
    state.evaluations = evaluations;
    state
}

fn wait_for_worker(state: &mut UavWorkflowState) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while state.is_running() && Instant::now() < deadline {
        state.poll();
        thread::sleep(Duration::from_millis(5));
    }
    state.poll();
    assert!(
        !state.is_running(),
        "UAV worker did not reach a terminal state: {:#?}",
        state.execution
    );
}

/// ALAS ships no propeller data; these SYNTHETIC tables stand in for it.
fn use_synthetic_tables() {
    alas_uav::propulsion_electric::prop_data::set_prop_data_dir(Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../alas-uav/tests/fixtures/prop_data"),
    ));
}
