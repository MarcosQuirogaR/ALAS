// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
// (allow inherited from the `#[allow(...)] mod tests` attribute in session.rs)

use super::*;
use crate::state::AppState;
use serde_json::Value;

fn state() -> AppState {
    let mut state = AppState::default();
    state.finish_walkthrough();
    state
}

fn root_z(state: &AppState) -> f64 {
    state.config_values["geometry"]["wing"]["root_z_m"]
        .as_f64()
        .expect("root z")
}

#[test]
fn first_entry_starts_from_ave_and_preserves_the_prior_case() {
    let mut state = state();
    state.load_preset("A380-800");
    let prior_preset = state.active_preset.clone();
    let prior_config = state.config_values.clone();
    let prior_log_len = state.logs.len();

    assert!(state.enter_sandbox(false));
    assert!(state.sandbox.active());
    assert!(state.active_preset.is_empty());
    assert_eq!(state.design_values["span_m"], 71.75, "AVE span");
    assert_eq!(
        state.config_values["optimizer"]["design_space"]["mode"],
        Value::String("baseline_sandbox".to_owned())
    );
    assert!(state.sandbox.prior_case.is_some());
    assert!(
        state.logs.len() < prior_log_len + 3,
        "sandbox log is its own"
    );
    assert!(state.sandbox.scene.is_some());

    assert!(state.resolve_leave_sandbox(ExitChoice::Discard));
    assert!(!state.sandbox.active());
    assert_eq!(state.active_preset, prior_preset);
    assert_eq!(state.config_values, prior_config);
    assert!(state.logs.len() >= prior_log_len);
}

#[test]
fn cancel_keeps_the_sandbox_open_and_unchanged() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    state.config_values["geometry"]["wing"]["root_z_m"] = Value::from(-1.0);
    state.on_config_modified();
    state.request_leave_sandbox();
    assert!(state.sandbox.exit_prompt);
    assert!(state.resolve_leave_sandbox(ExitChoice::Cancel));
    assert!(state.sandbox.active());
    assert!(!state.sandbox.exit_prompt);
    assert_eq!(root_z(&state), -1.0);
}

#[test]
fn promotion_returns_to_the_guided_workspace_with_a_custom_baseline() {
    let mut state = state();
    state.load_preset("A380-800");
    assert!(state.enter_sandbox(false));
    state.design_values.insert("span_m".to_owned(), 66.0);
    state.on_sandbox_model_changed();
    let sandbox_revision = state.sandbox.revision;

    assert!(state.resolve_leave_sandbox(ExitChoice::Promote));
    assert!(!state.sandbox.active());
    assert!(state.active_preset.is_empty());
    assert_eq!(state.starting_design(), StartingDesign::CleanSheet);
    assert_eq!(state.design_values["span_m"], 66.0);
    let config = state.typed_config().expect("promoted config");
    assert!(config.preset.is_empty());
    assert_eq!(config.optimizer.design_space.mode, DesignMode::CleanSheet);
    assert!(!config.optimizer.design_space.fuselage_sized_by_cabin);
    assert!(
        state.pipeline_result.is_none(),
        "mismatched results are dropped"
    );
    assert!(state.sandbox.prior_case.is_none());
    assert!(state.sandbox.last_design.is_some());
    assert!(!state.manual_geometry_locked());
    assert!(state.sandbox.revision >= sandbox_revision);
}

#[test]
fn re_entry_resumes_the_promoted_design_and_new_from_ave_is_explicit() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    state.design_values.insert("span_m".to_owned(), 66.0);
    state.on_sandbox_model_changed();
    assert!(state.resolve_leave_sandbox(ExitChoice::Promote));
    // A guided-workspace edit of the custom baseline carries into the resumed
    // sandbox.
    state.config_values["requirements"]["cruise_mach"] = Value::from(0.80);
    state.on_config_modified();

    assert!(state.enter_sandbox(false));
    assert_eq!(state.design_values["span_m"], 66.0);
    assert_eq!(
        state.config_values["requirements"]["cruise_mach"],
        Value::from(0.80)
    );

    assert!(state.new_sandbox_from_reference());
    assert_eq!(state.design_values["span_m"], 71.75);
    assert!(
        !state.sandbox.undo.can_undo(),
        "a new sandbox starts a new history"
    );
}

#[test]
fn discard_after_promotion_forgets_the_custom_design() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    assert!(state.resolve_leave_sandbox(ExitChoice::Promote));
    assert!(state.enter_sandbox(false));
    assert!(state.resolve_leave_sandbox(ExitChoice::Discard));
    assert!(state.sandbox.last_design.is_none());
    assert!(state.enter_sandbox(false));
    assert_eq!(state.design_values["span_m"], 71.75, "back to AVE");
}

#[test]
fn undo_and_redo_cover_form_edits_resets_and_drags_as_one_history() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    let original = root_z(&state);

    // A form edit.
    let before = state.edit_snapshot();
    state.config_values["geometry"]["wing"]["root_z_m"] = Value::from(-1.0);
    state.sandbox.undo.record(before);
    state.on_config_modified();

    // A drag: one transaction with several intermediate frames.
    let handle = state
        .sandbox_handles()
        .into_iter()
        .find(|h| h.kind == super::super::drag::HandleKind::Span)
        .expect("span handle");
    let framing = state.sandbox.scene.as_ref().expect("scene").1;
    assert!(state.begin_handle_drag(&handle, &framing, 1.0));
    for _ in 0..5 {
        state.update_handle_drag(egui::vec2(4.0, 0.0));
    }
    state.end_handle_drag();
    let dragged_span = state.design_values["span_m"];
    assert_ne!(dragged_span, 71.75);
    assert_eq!(state.sandbox.undo.undo_depth(), 2);

    assert!(state.sandbox_undo());
    assert_eq!(
        state.design_values["span_m"], 71.75,
        "one undo reverts the whole drag"
    );
    assert_eq!(root_z(&state), -1.0);
    assert!(state.sandbox_undo());
    assert_eq!(root_z(&state), original);
    assert!(state.sandbox_redo());
    assert_eq!(root_z(&state), -1.0);
    assert!(state.sandbox_redo());
    assert_eq!(state.design_values["span_m"], dragged_span);
    assert!(!state.sandbox_redo());
}

#[test]
fn a_rejected_geometry_keeps_the_last_valid_scene_and_reports_it() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    let valid_revision = state.sandbox.scene_revision;
    state.design_values.insert("tip_chord_m".to_owned(), -2.0);
    state.on_sandbox_model_changed();
    assert_eq!(state.sandbox.scene_revision, valid_revision);
    assert!(state.sandbox.scene.is_some());
    assert!(state.sandbox.rejected_edit.is_some());
    state.design_values.insert("tip_chord_m".to_owned(), 2.0);
    state.on_sandbox_model_changed();
    assert!(state.sandbox.rejected_edit.is_none());
    assert!(state.sandbox.scene_revision > valid_revision);
}

#[test]
fn every_model_edit_bumps_the_revision_and_invalidates_estimates() {
    let mut state = state();
    assert!(state.enter_sandbox(false));
    let revision = state.sandbox.revision;
    state.sandbox.estimates.computed_revision = Some(revision);
    // An edit through a detached settings path uses the shared hook.
    state.config_values["requirements"]["cruise_mach"] = Value::from(0.80);
    state.on_config_modified();
    assert_eq!(state.sandbox.revision, revision + 1);
    assert!(state.sandbox.estimates.stale);
}

#[test]
fn a_run_in_flight_blocks_entering_the_sandbox() {
    let mut state = state();
    state.is_running = true;
    assert!(!state.enter_sandbox(false));
    assert!(!state.sandbox.active());
}

#[test]
fn the_estimates_strip_stays_closed_until_quick_analysis_opens_it() {
    let mut state = state();
    assert!(state.enter_sandbox(true));
    assert!(
        !state.sandbox.layout.estimates_open,
        "closed on first entry"
    );
    assert!(
        !state.sandbox.layout.summary_open,
        "the Summary card is closed too"
    );
    // A strip left open (View menu or an older layout) does not survive a
    // re-entry: only Quick Analysis opens it.
    state.sandbox.layout.estimates_open = true;
    state.request_leave_sandbox();
    assert!(state.resolve_leave_sandbox(ExitChoice::Discard));
    assert!(state.enter_sandbox(false));
    assert!(!state.sandbox.layout.estimates_open, "closed on re-entry");
    crate::sandbox::workspace::start_quick_analysis(&mut state);
    assert!(
        state.sandbox.layout.estimates_open,
        "Quick Analysis opens it"
    );
}
