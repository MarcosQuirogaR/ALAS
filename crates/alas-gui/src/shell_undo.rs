// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Undo and redo for edits made in the guided shell.
//!
//! Every committed change of the case (a form field, an Advanced section, the
//! seat-class toggle, a field-length override, Create Clean Sheet, opening a
//! library file) goes through `on_config_modified`; this module keeps the
//! state before each of them. The Sandbox has its own history and is not
//! touched here.

use crate::sandbox::undo::{EditSnapshot, UndoStack};
use crate::state::AppState;

/// Undo steps retained.
const CAPACITY: usize = 200;

/// The shell's undo history.
#[derive(Debug)]
pub struct ShellUndo {
    stack: UndoStack,
    baseline: Option<EditSnapshot>,
    replaying: bool,
    dirty: bool,
    /// The pointer is held this frame: a drag in progress.
    held: bool,
    /// Edits were committed while the pointer was held; `baseline` is still the
    /// state from before the gesture and the one step is recorded on release.
    gesture_open: bool,
}

impl Default for ShellUndo {
    fn default() -> Self {
        Self {
            stack: UndoStack::new(CAPACITY),
            baseline: None,
            replaying: false,
            dirty: false,
            held: false,
            gesture_open: false,
        }
    }
}

impl ShellUndo {
    /// Whether the case changed since it was last saved or opened.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The case now matches what is on disk (or was just replaced).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// The case holds a recovered copy that is not on disk as a saved file.
    pub(crate) fn mark_unsaved(&mut self) {
        self.dirty = true;
    }
}

impl AppState {
    /// Start the history from the current case.
    pub fn reset_undo_baseline(&mut self) {
        self.shell.undo.stack.clear();
        self.shell.undo.gesture_open = false;
        self.shell.undo.baseline = Some(self.edit_snapshot());
    }

    /// Begin a fresh history after the sandbox handed its case back, so an undo
    /// never returns to a case from before the sandbox. `edited` marks the
    /// case as changed since it was last saved.
    pub(crate) fn settle_after_sandbox(&mut self, edited: bool) {
        // The sandbox's run log must not dock under the guided pages for an
        // informational line; an error line reopens it.
        self.run_log_open = false;
        self.on_config_modified();
        self.reset_undo_baseline();
        if edited {
            self.shell.undo.dirty = true;
        }
        // Leaving on purpose retires the Sandbox's crash copy, only where
        // autosave is on (the writes are gated the same way, so a state that
        // never autosaves never deletes). Promote first writes the guided
        // slots: the promoted aircraft exists nowhere else. Discard writes
        // nothing: the held case was flushed when the Sandbox opened, and a
        // previous session's copy awaiting the user's decision is never touched.
        if self.files.autosave_enabled {
            self.files.autosave_since = None;
            let guided_safe = !edited || self.library_autosave();
            if guided_safe && !self.slot_protected("sandbox") {
                let _ = std::fs::remove_file(crate::file_library::sandbox_autosave_path(
                    &self.data_dir(),
                ));
            }
        }
    }

    /// Called once at the start of each frame with whether a pointer button is
    /// held. A slider or drag value commits on every frame of its drag; those
    /// edits are merged into the one step from the drag's start to its end
    /// (the same gesture rule as `views::edit_draft`), recorded on release.
    pub(crate) fn sync_undo_gesture(&mut self, pointer_held: bool) {
        self.shell.undo.held = pointer_held;
        if !pointer_held && self.shell.undo.gesture_open {
            self.shell.undo.gesture_open = false;
            self.close_undo_step();
        }
    }

    /// Record the state before the edit that has just been committed.
    pub(crate) fn note_edit_for_undo(&mut self) {
        if self.sandbox.active() || self.shell.undo.replaying {
            return;
        }
        if self.shell.undo.held {
            // Mid-gesture: keep the pre-gesture baseline, record on release.
            if self.shell.undo.baseline.is_some() {
                self.shell.undo.gesture_open = true;
                self.shell.undo.dirty = true;
                return;
            }
        }
        self.close_undo_step();
    }

    /// Record `baseline -> now` as one undo step and make `now` the baseline.
    fn close_undo_step(&mut self) {
        let now = self.edit_snapshot();
        if let Some(before) = self.shell.undo.baseline.take() {
            if before != now {
                self.shell.undo.stack.record(before);
                self.shell.undo.dirty = true;
            }
        }
        self.shell.undo.baseline = Some(now);
    }

    /// Run a change the program makes by itself (a mode switch that a button
    /// implies): it is part of the case but not a user edit, so it neither
    /// marks the case unsaved nor adds an undo step.
    pub(crate) fn without_marking_edit(&mut self, change: impl FnOnce(&mut Self)) {
        let was_replaying = self.shell.undo.replaying;
        self.shell.undo.replaying = true;
        change(self);
        self.shell.undo.replaying = was_replaying;
        if !was_replaying {
            self.shell.undo.baseline = Some(self.edit_snapshot());
        }
    }

    /// Whether an undo step exists.
    pub fn can_shell_undo(&self) -> bool {
        !self.sandbox.active() && self.shell.undo.stack.can_undo()
    }

    /// Whether a redo step exists.
    pub fn can_shell_redo(&self) -> bool {
        !self.sandbox.active() && self.shell.undo.stack.can_redo()
    }

    fn replay(&mut self, snapshot: EditSnapshot) {
        self.shell.undo.replaying = true;
        self.config_values = snapshot.config_values;
        self.bump_config_revision();
        self.design_values = snapshot.design_values;
        // The registered aircraft of a case is the `preset` key it carries.
        self.active_preset = self
            .config_values
            .get("preset")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
        self.on_config_modified();
        self.shell.undo.replaying = false;
        self.shell.undo.baseline = Some(self.edit_snapshot());
    }

    /// Undo the last edit of the case.
    pub fn shell_undo(&mut self) -> bool {
        if self.sandbox.active() {
            return false;
        }
        let current = self.edit_snapshot();
        match self.shell.undo.stack.undo(current) {
            Some(previous) => {
                self.replay(previous);
                true
            }
            None => false,
        }
    }

    /// Redo the last undone edit.
    pub fn shell_redo(&mut self) -> bool {
        if self.sandbox.active() {
            return false;
        }
        let current = self.edit_snapshot();
        match self.shell.undo.stack.redo(current) {
            Some(next) => {
                self.replay(next);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;

    /// Load AVE, press Analyze reference with no edit, let the run finish:
    /// switching preset afterwards must not ask to replace unsaved changes.
    #[test]
    fn analyzing_the_reference_without_an_edit_leaves_the_case_saved() {
        let mut state = AppState::default();
        state.perform(crate::shell_confirm::Pending::Preset("AVE".to_owned()));
        assert!(!state.has_unsaved_edits(), "a loaded preset is saved");
        state.start_pipeline(true);
        let started = std::time::Instant::now();
        while state.is_running && started.elapsed() < std::time::Duration::from_secs(300) {
            state.poll_worker();
            state.note_autosave_due();
            assert!(!state.has_unsaved_edits(), "marked unsaved while running");
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        state.poll_worker();
        state.note_autosave_due();
        assert!(state.pipeline_result.is_some(), "the run delivered a result");
        assert!(!state.has_unsaved_edits(), "marked unsaved by the run");
        assert!(!state.needs_confirmation(&crate::shell_confirm::Pending::Preset(
            "A320-200".to_owned()
        )));
    }

    /// Switching to Advanced and opening its pages fills defaults and bounds
    /// by itself; none of that is an edit of the case.
    #[test]
    fn the_advanced_pages_opened_without_an_edit_leave_the_case_saved() {
        let mut state = AppState::default();
        state.perform(crate::shell_confirm::Pending::Preset("B747-400".to_owned()));
        state.without_marking_edit(|s| s.set_design_mode(alas_config::DesignMode::BaselineSandbox));
        assert!(!state.has_unsaved_edits());
        state.shell.detail = crate::detail::Detail::Advanced;
        let ctx = egui::Context::default();
        for page in ["inputs", "design_space", "setup_analyses", "mission", "payload"] {
            crate::app::go_to_page(&mut state, page);
            let nav_page = crate::nav::page(page).unwrap().clone();
            for _ in 0..3 {
                crate::detail::publish(&ctx, crate::detail::Detail::Advanced);
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1400.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            if page == "design_space" {
                                crate::views::design_space_view::show_design_space_view(
                                    &mut state, ui,
                                );
                            } else {
                                crate::views::flow_pages::show_flow_page(&mut state, ui, &nav_page);
                            }
                        });
                    },
                );
                state.note_autosave_due();
                assert!(!state.has_unsaved_edits(), "{page} marked the case unsaved");
            }
        }
    }

    #[test]
    fn an_edit_in_an_advanced_section_undoes_and_redoes() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        let before = state.config_values["mass_model"]["fuel_density_kg_m3"].clone();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(800.0);
        state.on_config_modified();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(790.0);
        state.on_config_modified();
        assert!(state.shell_undo());
        assert_eq!(
            state.config_values["mass_model"]["fuel_density_kg_m3"],
            serde_json::json!(800.0)
        );
        assert!(state.shell_undo());
        assert_eq!(
            state.config_values["mass_model"]["fuel_density_kg_m3"],
            before
        );
        assert!(!state.can_shell_undo());
        assert!(state.shell_redo());
        assert_eq!(
            state.config_values["mass_model"]["fuel_density_kg_m3"],
            serde_json::json!(800.0)
        );
        // A new edit after an undo drops the redo history.
        state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(810.0);
        state.on_config_modified();
        assert!(!state.can_shell_redo());
    }

    #[test]
    fn a_drag_of_many_commits_is_one_undo_step() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        let before = state.config_values["mass_model"]["fuel_density_kg_m3"].clone();
        state.sync_undo_gesture(true);
        for value in [780.0, 790.0, 800.0] {
            state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(value);
            state.on_config_modified();
            state.sync_undo_gesture(true);
        }
        assert!(!state.can_shell_undo(), "nothing is recorded mid-drag");
        state.sync_undo_gesture(false);
        assert!(state.can_shell_undo());
        assert!(state.has_unsaved_edits());
        assert!(state.shell_undo());
        assert_eq!(
            state.config_values["mass_model"]["fuel_density_kg_m3"],
            before
        );
        assert!(!state.can_shell_undo(), "the whole drag was one step");
        assert!(state.shell_redo());
        assert_eq!(
            state.config_values["mass_model"]["fuel_density_kg_m3"],
            serde_json::json!(800.0)
        );
    }

    #[test]
    fn a_sandbox_hand_back_starts_a_fresh_history_and_marks_the_edit() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(800.0);
        state.on_config_modified();
        assert!(state.can_shell_undo());
        state.shell.undo.mark_saved();
        state.settle_after_sandbox(true);
        assert!(!state.can_shell_undo(), "no undo into the pre-sandbox case");
        assert!(state.has_unsaved_edits());
        state.settle_after_sandbox(false);
        assert!(
            state.has_unsaved_edits(),
            "handing back does not settle edits"
        );
    }

    #[test]
    fn replacing_the_case_is_one_undoable_step() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        let wing = state.config_values["geometry"]["wing"].clone();
        state.load_preset("A220-300");
        assert!(state.shell_undo());
        assert_eq!(state.config_values["geometry"]["wing"], wing);
    }
}
