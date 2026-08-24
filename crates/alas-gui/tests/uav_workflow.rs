// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless contracts for the fixed-wing UAV desktop workflow.

// Navigation/catalogue fixtures are required by the page contract under test.
#![allow(clippy::expect_used)]

use alas_gui::nav::{self, PageKind};
use alas_gui::uav::{
    ComponentRole, MissionPlanMode, PropulsionInputMode, UavExecutionStatus, UavSection,
    UavWorkflowOutcome, UavWorkflowState,
};
use alas_uav::{TopologyAvailability, UavAnalysisPath, UavTopology};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn fixed_wing_uav_is_a_first_class_navigation_page() {
    let page = nav::page("uav").expect("UAV page is registered");
    assert_eq!(page.kind, PageKind::Uav);
    assert_eq!(page.title, "Fixed-Wing UAV");
}

#[test]
fn every_design_convention_has_a_typed_sizing_boundary() {
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
    for topology in [
        UavTopology::TTail,
        UavTopology::VTail,
        UavTopology::FlyingWing,
    ] {
        assert!(matches!(
            topology.availability(UavAnalysisPath::PreliminaryOptimization),
            TopologyAvailability::Unavailable(_)
        ));
    }
}

#[test]
#[ignore = "the complete-catalogue gate intentionally removes incomplete source records"]
fn every_required_role_defaults_to_a_reviewed_provenanced_record() {
    let state = UavWorkflowState::default();
    for role in ComponentRole::ALL {
        let record = state
            .selected_record(role)
            .expect("expanded reviewed catalogue covers every UAV role");
        assert!(record.provenance.source_url.starts_with("https://"));
        assert!(!record.provenance.publisher.trim().is_empty());
    }
    assert_eq!(state.propulsion_input_mode, PropulsionInputMode::Automatic);
    assert_eq!(state.mission_plan_mode, MissionPlanMode::Standard);
    assert_eq!(state.selections.motor, "tmotor-at2814-900kv");
    assert_eq!(state.selections.propeller, "apc-12x6e");
    assert!(!state.selected_evidence_gaps().is_empty());
    assert!(state
        .selected_evidence_gaps()
        .iter()
        .any(|gap| gap.contains("allowable stress")));
}

#[test]
#[ignore = "the complete-catalogue gate intentionally removes incomplete source records"]
fn component_selection_exposes_source_expanded_records_without_filling_missing_data() {
    let mut state = UavWorkflowState::default();
    let motors = state.records_for(ComponentRole::Motor);
    let motor = motors
        .iter()
        .find(|record| record.id == "tmotor-at1050-kv90")
        .expect("source-expanded T-MOTOR motor is selectable in the GUI");
    assert_eq!(motor.provenance.publisher, "T-MOTOR");
    assert_eq!(
        motor.provenance.source_url,
        "https://store.tmotor.com/product/fixed-wing-motor-at1050.html"
    );
    match &motor.kind {
        alas_uav::ComponentKind::Motor(spec) => {
            assert_eq!(spec.max_static_thrust_n, None);
            assert_eq!(spec.recommended_propeller_diameter_m, None);
        }
        other => panic!("expected motor record, got {other:?}"),
    }

    state.selections.set(ComponentRole::Motor, motor.id.clone());
    let selected = state
        .selected_record(ComponentRole::Motor)
        .expect("source-expanded selection remains addressable");
    assert_eq!(selected.id, motor.id);
}

#[test]
fn automatic_sizing_never_substitutes_missing_material_or_landing_gear_evidence() {
    let mut state = UavWorkflowState::default();
    assert!(state.records_for(ComponentRole::Material).is_empty());
    assert!(state.records_for(ComponentRole::LandingGear).is_empty());
    assert!(!state.start());
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Failed(message) if message.contains("priced, analysis-complete")
    ));
    assert_eq!(state.outcome, UavWorkflowOutcome::NotRun);
}

#[test]
#[ignore = "the complete-catalogue gate intentionally blocks before the manual map path"]
fn advanced_manual_map_without_evidence_is_rejected_before_optimization() {
    let mut state = UavWorkflowState::default();
    state.propulsion_input_mode = PropulsionInputMode::AdvancedManual;
    state.propulsion_evidence.clear();
    assert!(!state.start());
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Failed(message) if message.contains("evidence is required")
    ));
    assert_eq!(state.outcome, UavWorkflowOutcome::NotRun);
}

#[test]
#[ignore = "a complete material and landing-gear record has not been sourced"]
fn automatic_solver_needs_no_manual_points_and_retains_a_multi_phase_result() {
    let mut state = runnable_state(8);
    state.propulsion_evidence.clear();
    state.propulsion_motor_count = 2;
    assert!(state.start());
    wait_for_worker(&mut state);

    let mission = state
        .last_electrical_mission
        .as_ref()
        .expect("automatic source-backed calculation is retained with any optimizer verdict");
    assert_eq!(mission.phases.len(), 4);
    assert!(mission.total_duration_s >= state.objectives.endurance_s);
    assert!(mission.total_distance_m >= state.objectives.range_m);
    assert!(
        mission.phases[0].propulsion.total_thrust_n
            > mission.phases[0].propulsion.per_motor.thrust_n
    );
    assert!(mission.maximum_battery_current_a > 0.0);
}

#[test]
#[ignore = "a complete material and landing-gear record has not been sourced"]
fn background_search_completes_and_reports_retail_evidence_gaps() {
    let mut state = runnable_state(8);
    assert!(state.start());
    wait_for_worker(&mut state);

    assert_eq!(state.execution, UavExecutionStatus::Completed);
    assert_eq!(state.active_section, UavSection::Results);
    assert_eq!(
        state.last_completed_topology,
        Some(UavTopology::ConventionalTail)
    );
    assert!(
        matches!(
            state.outcome,
            UavWorkflowOutcome::NoFeasibleDesign(_)
                | UavWorkflowOutcome::PreliminaryFeasible { .. }
        ),
        "unexpected outcome: {:#?}",
        state.outcome
    );
    if let UavWorkflowOutcome::NoFeasibleDesign(summary) = state.outcome {
        assert!(summary
            .rejections
            .iter()
            .any(|row| { row.kind == alas_uav::FindingKind::MissingData && row.candidates > 0 }));
        assert!(summary
            .examples
            .iter()
            .any(|example| example.kind == alas_uav::FindingKind::MissingData));
        assert!(summary.best_evaluated.is_none());
    }
}

#[test]
fn unsupported_design_conventions_explain_why_preliminary_sizing_cannot_start() {
    for topology in [
        UavTopology::TTail,
        UavTopology::VTail,
        UavTopology::FlyingWing,
    ] {
        let mut state = runnable_state(8);
        state.topology = topology;
        assert!(!state.start());
        assert!(matches!(
            state.execution,
            UavExecutionStatus::Failed(message) if message.contains(topology.label())
        ));
    }
}

#[test]
#[ignore = "a complete material and landing-gear record has not been sourced"]
fn active_search_cannot_be_started_twice_and_cancels_cooperatively() {
    let mut state = runnable_state(1_000_000);
    assert!(state.start());
    assert!(
        !state.start(),
        "one input gesture starts at most one worker"
    );
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Running(progress)
            if progress.evaluated_candidates == 0
                && progress.total_candidates == 1_000_000
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
    assert_eq!(state.outcome, UavWorkflowOutcome::NotRun);
}

#[test]
#[ignore = "a complete material and landing-gear record has not been sourced"]
fn failed_or_cancelled_restart_retains_the_last_completed_result() {
    let mut state = runnable_state(8);
    assert!(state.start());
    wait_for_worker(&mut state);
    let completed = state.outcome.clone();
    assert_ne!(completed, UavWorkflowOutcome::NotRun);

    state.propulsion_input_mode = PropulsionInputMode::AdvancedManual;
    state.propulsion_evidence.clear();
    assert!(!state.start());
    assert!(matches!(state.execution, UavExecutionStatus::Failed(_)));
    assert_eq!(state.outcome, completed);

    state.propulsion_evidence = "user-provided dynamometer record".to_owned();
    state.evaluations = 1_000_000;
    assert!(state.start());
    state.cancel();
    wait_for_worker(&mut state);
    assert!(matches!(
        state.execution,
        UavExecutionStatus::Cancelled { .. }
    ));
    assert_eq!(state.outcome, completed);
}

#[test]
fn dynamic_uav_labels_have_spanish_catalog_entries() {
    let catalog = alas_i18n::es::desktop_catalog();
    for key in ComponentRole::ALL
        .into_iter()
        .map(ComponentRole::label)
        .chain(UavSection::ALL.into_iter().map(UavSection::short_title))
        .chain(
            [
                PropulsionInputMode::Automatic,
                PropulsionInputMode::AdvancedManual,
            ]
            .into_iter()
            .map(PropulsionInputMode::label),
        )
        .chain(
            [MissionPlanMode::Standard, MissionPlanMode::Advanced]
                .into_iter()
                .map(MissionPlanMode::label),
        )
        .chain(
            UavTopology::ALL
                .into_iter()
                .flat_map(|topology| [topology.label(), topology.description()]),
        )
        .chain([
            "UAV workflow",
            "Preliminary design metrics",
            "Design convention",
            "Propulsion data source or test ID",
            "Preliminary sizing is available for this design convention.",
            "Preliminary sizing unavailable: {reason}",
            "Choose a supported convention before starting preliminary sizing: {reason}",
            "Endurance",
            "Range",
            "Cruise speed",
            "Maximum stall speed",
            "Missing evidence",
            "Insufficient lift",
            "Insufficient thrust",
            "Energy shortfall",
            "Structural overload",
            "Landing-gear overload",
            "VLM CL",
            "Induced-CD discrepancy",
            "Cancel UAV search",
            "Cancellation requested; finishing the current candidate.",
            "UAV search completed.",
            "The last completed UAV result is retained.",
            "Shared-core lift verdict: passed",
            "Shared-core lift verdict: insufficient lift",
            "Design convention used by this result: {topology}",
            "The selected convention differs from this result; run again to update it.",
        ])
    {
        assert!(catalog.contains_key(key), "missing Spanish UAV key: {key}");
    }
}

fn runnable_state(evaluations: usize) -> UavWorkflowState {
    let mut state = UavWorkflowState::default();
    state.propulsion_evidence = "user-provided dynamometer record".to_owned();
    state.evaluations = evaluations;
    state
}

fn wait_for_worker(state: &mut UavWorkflowState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.is_running() && Instant::now() < deadline {
        state.poll();
        thread::sleep(Duration::from_millis(1));
    }
    state.poll();
    assert!(
        !state.is_running(),
        "UAV worker did not reach a terminal state: {:#?}",
        state.execution
    );
}
