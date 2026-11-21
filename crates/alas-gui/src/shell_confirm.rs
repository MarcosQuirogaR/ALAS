// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Confirmation before an action replaces a case with unsaved edits, and
//! before the application exits with them.

use std::path::PathBuf;

use egui::{Align2, Area, Context, Id, Order, RichText};

use crate::config_edit::CleanSheetBrief;
use crate::file_library::LibraryEntry;
use crate::state::AppState;
use crate::views::{tr, tr_label};

/// What waits for the user's answer.
#[derive(Debug, Clone)]
pub enum Pending {
    /// Choose a registered aircraft.
    Preset(String),
    /// Open a saved aircraft.
    UserAircraft(PathBuf),
    /// Open a library file.
    Library(LibraryEntry),
    /// Open a file from any path.
    Path(PathBuf),
    /// Create a clean sheet.
    CleanSheet(CleanSheetBrief),
    /// Load the configuration file at `config_path`.
    ConfigFile,
    /// Replace the sandbox aircraft with the AVE reference.
    NewSandbox,
    /// Leave the Sandbox keeping its aircraft as the guided case.
    Promote,
    /// Restore the Sandbox's own crash-autosave slot.
    RestoreSandbox,
    /// Restore the autosaved object of this kind.
    RestoreAutosave(crate::file_library::ObjectKind),
    /// Quit the application.
    Exit,
}

impl Pending {
    /// Whether the action replaces a whole aircraft (and so settles its unsaved
    /// edits when it succeeds), as opposed to a part of it, which is an edit.
    fn replaces_case(&self) -> bool {
        use crate::file_library::ObjectKind::{Aircraft, Results};
        match self {
            Pending::Library(entry) => matches!(entry.kind, Aircraft | Results),
            Pending::RestoreAutosave(kind) => matches!(kind, Aircraft | Results),
            Pending::Path(path) => crate::file_library::file_kind(path)
                .is_none_or(|kind| matches!(kind, Aircraft | Results)),
            // Promote keeps the sandbox aircraft as a new, edited guided case.
            // A restored Sandbox copy stays unsaved (see `library_restore_sandbox_autosave`).
            Pending::Promote | Pending::Exit | Pending::RestoreSandbox => false,
            _ => true,
        }
    }
}

impl AppState {
    /// Whether the application holds edits that were neither saved nor
    /// autosaved to a file: the guided case's (also while the Sandbox holds
    /// it) or the sandbox aircraft's.
    pub fn has_unsaved_edits(&self) -> bool {
        self.shell.undo.is_dirty() || self.sandbox.dirty()
    }

    /// The aircraft the user is working on was saved or replaced as it stands:
    /// the sandbox aircraft in the Sandbox, the guided case otherwise. The
    /// guided case the Sandbox holds is never settled from there.
    pub(crate) fn settle_case(&mut self) {
        if self.sandbox.active() {
            self.sandbox.mark_saved();
        } else {
            self.shell.undo.mark_saved();
        }
    }

    /// Whether `action` would lose unsaved edits: Exit loses either aircraft's,
    /// any other action replaces the aircraft being worked on.
    pub(crate) fn needs_confirmation(&self, action: &Pending) -> bool {
        match action {
            Pending::Exit => self.has_unsaved_edits(),
            // Promoting replaces the guided case the Sandbox holds.
            Pending::Promote => self.shell.undo.is_dirty(),
            // Restoring the Sandbox's autosave replaces the sandbox aircraft only; from
            // the guided workspace it enters the Sandbox and holds the guided case.
            Pending::RestoreSandbox => self.sandbox.dirty(),
            // A partial open is an undoable edit, not a replacement.
            _ if !action.replaces_case() => false,
            _ if self.sandbox.active() => self.sandbox.dirty(),
            _ => self.shell.undo.is_dirty(),
        }
    }

    /// Run `action` now, or after the user confirms when edits would be lost.
    /// Leaving the application asks for either aircraft's edits; a load in the
    /// Sandbox replaces the sandbox aircraft only, so it asks for those.
    pub fn guarded(&mut self, action: Pending) {
        if self.needs_confirmation(&action) {
            self.shell.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    pub(crate) fn perform(&mut self, action: Pending) {
        // Replacing the whole case settles the unsaved edits; opening a part of it
        // (a mission plan, an engine) is itself an edit. A file that fails to open
        // replaces nothing, so it settles nothing.
        let replaces_case = action.replaces_case();
        let restored_copy = matches!(
            action,
            Pending::RestoreAutosave(crate::file_library::ObjectKind::Aircraft)
        );
        let errors = self.shell.errors_logged;
        let opened = match action {
            Pending::Preset(name) => {
                self.load_preset(&name);
                true
            }
            Pending::UserAircraft(path) => {
                self.load_user_aircraft(&path);
                true
            }
            Pending::Library(entry) => self.library_open(&entry),
            Pending::Path(path) => self.library_open_path(&path),
            Pending::CleanSheet(brief) => self.start_clean_sheet(&brief),
            Pending::ConfigFile => {
                self.load_config();
                true
            }
            Pending::NewSandbox => self.new_sandbox_from_reference(),
            Pending::Promote => self.resolve_leave_sandbox(crate::sandbox::ExitChoice::Promote),
            Pending::RestoreSandbox => self.library_restore_sandbox_autosave(),
            Pending::RestoreAutosave(kind) => self.library_restore_autosave(kind),
            Pending::Exit => std::process::exit(0),
        };
        // A failure shows as a result where the open reports one, and as an error
        // line where it only logs.
        if opened {
            // A different file or aircraft is a new question for the live dispatch fetch.
            self.shell.live_dispatch.reset();
        }
        let failed = !opened || self.shell.errors_logged != errors;
        if replaces_case && !failed {
            self.settle_case();
            // A recovered guided crash copy is not on disk as a saved file.
            if restored_copy {
                if self.sandbox.active() {
                    self.sandbox.saved_revision = self.sandbox.revision.wrapping_sub(1);
                } else {
                    self.shell.undo.mark_unsaved();
                }
            }
        }
    }
}

/// A dimmed layer above the in-app windows that swallows every click.
pub(crate) fn modal_blocker(ctx: &Context, id: &'static str) {
    let screen = ctx.screen_rect();
    Area::new(Id::new(id))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(110));
        });
}

/// How a button of a [`confirm_dialog`] is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmStyle {
    /// The highlighted main action.
    Primary,
    /// An action that loses something: the subtle red button.
    Destructive,
    /// Any other action.
    Plain,
}

/// One action of a [`confirm_dialog`].
#[derive(Debug, Clone)]
pub struct ConfirmAction {
    /// The button text, already translated.
    pub label: String,
    /// How the button is drawn.
    pub style: ConfirmStyle,
}

impl ConfirmAction {
    /// The highlighted main action.
    pub fn primary(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: ConfirmStyle::Primary,
        }
    }

    /// A destructive action, drawn in red.
    pub fn destructive(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: ConfirmStyle::Destructive,
        }
    }

    /// An ordinary action.
    pub fn plain(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: ConfirmStyle::Plain,
        }
    }
}

/// The user's answer to a [`confirm_dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAnswer {
    /// The action at this index of the list was chosen.
    Action(usize),
    /// Cancel, or Escape.
    Cancel,
}

/// The one blocking dialog of the application: a dimmed layer that swallows
/// every click, a card with a bold title and a body, the actions in the order
/// given (the first is the highlighted main action; red only for a
/// destructive one) and Cancel last. Escape answers Cancel.
///
/// `id` keeps two dialogs apart. Returns the answer in the frame it is given,
/// `None` while the dialog waits. Text is passed already translated.
pub fn confirm_dialog(
    ctx: &Context,
    id: &'static str,
    title: &str,
    body: Option<&str>,
    actions: &[ConfirmAction],
) -> Option<ConfirmAnswer> {
    let mut answer = None;
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        answer = Some(ConfirmAnswer::Cancel);
    }
    // Modal: a dimmed layer above the in-app windows swallows every click; the
    // dialog sits one order higher, so a click on the layer never raises it.
    modal_blocker(ctx, id);
    Area::new(Id::new((id, "card")))
        .order(Order::Tooltip)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, -40.0))
        .show(ctx, |ui| {
            crate::theme::card_frame(ui)
                .fill(ui.visuals().panel_fill)
                .show(ui, |ui| {
                    ui.set_max_width(CONFIRM_MAX_WIDTH);
                    ui.label(RichText::new(title).strong().size(16.0));
                    if let Some(body) = body {
                        ui.add_space(6.0);
                        ui.label(body);
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        for (index, action) in actions.iter().enumerate() {
                            let response = match action.style {
                                ConfirmStyle::Primary => {
                                    ui.add(crate::theme::primary_button(action.label.as_str()))
                                }
                                ConfirmStyle::Destructive => {
                                    crate::theme::danger_button(ui, action.label.as_str())
                                }
                                ConfirmStyle::Plain => ui.button(action.label.as_str()),
                            };
                            if response.clicked() {
                                answer = Some(ConfirmAnswer::Action(index));
                            }
                        }
                        if ui.button(tr_label("Cancel")).clicked() {
                            answer = Some(ConfirmAnswer::Cancel);
                        }
                    });
                });
        });
    answer
}

/// Widest the card of a [`confirm_dialog`] grows, in points.
const CONFIRM_MAX_WIDTH: f32 = 420.0;

/// Draw the confirmation, when an action waits.
pub fn show(state: &mut AppState, ctx: &Context) {
    let Some(action) = state.shell.pending.clone() else {
        crate::native_viewport::set_modal_pending(ctx, false);
        return;
    };
    // The prompt lives in the root window; a native window in front of it
    // would hide it, so the root takes the focus when the prompt appears and
    // the detached windows stop accepting input until it is answered.
    if !crate::native_viewport::set_modal_pending(ctx, true) {
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }
    let exiting = matches!(action, Pending::Exit);
    let (title, go) = if exiting {
        ("Exit With Unsaved Changes", "Exit")
    } else {
        ("Replace Unsaved Changes", "Replace")
    };
    let body = tr("Edits that were not saved will be lost.");
    let answer = confirm_dialog(
        ctx,
        "alas_confirm_block",
        &tr_label(title),
        Some(&body),
        &[ConfirmAction::primary(tr_label(go))],
    );
    match answer {
        Some(ConfirmAnswer::Action(_)) => {
            state.shell.pending = None;
            state.perform(action);
        }
        Some(ConfirmAnswer::Cancel) => state.shell.pending = None,
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LogKind;
    use serde_json::json;

    #[test]
    fn a_file_that_fails_to_open_leaves_the_unsaved_edits_standing() {
        let root = std::env::temp_dir().join(format!("alas-confirm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("dir");
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(805.0);
        state.on_config_modified();
        assert!(state.has_unsaved_edits());
        let broken = root.join("broken.json");
        std::fs::write(&broken, "{ not json").expect("write");
        state.guarded(Pending::Path(broken));
        assert!(state.shell.pending.is_some(), "the prompt is raised");
        let waiting = state.shell.pending.take().expect("pending");
        state.perform(waiting);
        assert!(
            state.has_unsaved_edits(),
            "a failed open must not settle the edits"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn edited_guided_state(tag: &str) -> (AppState, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("alas-confirm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.load_preset("A320-200");
        state.reset_undo_baseline();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(805.0);
        state.on_config_modified();
        assert!(state.has_unsaved_edits());
        (state, root)
    }

    /// A Save As in the Sandbox saves the sandbox aircraft, and a save that
    /// writes nothing saves nothing: neither settles the guided case.
    #[test]
    fn a_sandbox_save_as_or_a_failed_save_leaves_the_guided_case_unsaved() {
        use crate::file_dialog::DialogTarget;
        let (mut state, root) = edited_guided_state("save-as");
        std::fs::create_dir_all(&root).expect("dir");
        // A failed save, in the guided workspace: the path is a directory.
        state.apply_dialog_result(DialogTarget::ConfigSave, root.clone());
        assert!(state.has_unsaved_edits(), "nothing was written");
        // A successful one settles the guided case.
        state.apply_dialog_result(DialogTarget::ConfigSave, root.join("case.json"));
        assert!(!state.has_unsaved_edits());
        // Edit again, enter the Sandbox, Save As there.
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(810.0);
        state.on_config_modified();
        assert!(state.enter_sandbox(true));
        state.apply_dialog_result(DialogTarget::ConfigSave, root.join("sandbox.json"));
        assert!(!state.sandbox.dirty(), "the sandbox aircraft was saved");
        assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
        assert!(state.has_unsaved_edits(), "the held guided case was not");
        let _ = std::fs::remove_dir_all(root);
    }

    /// Sandbox edits are tracked: leaving the application or loading over them
    /// asks, and a saved or loaded sandbox aircraft is clean.
    #[test]
    fn sandbox_edits_are_tracked_and_asked_about() {
        let (mut state, root) = edited_guided_state("tracked");
        assert!(state.save_current_as_user_aircraft("Reload Me"));
        let saved = state.user_aircraft().first().expect("saved").path.clone();
        assert!(!state.has_unsaved_edits());
        assert!(state.enter_sandbox(true));
        assert!(!state.has_unsaved_edits(), "entering edits nothing");
        state.guarded(Pending::UserAircraft(saved.clone()));
        assert!(
            state.shell.pending.is_none(),
            "a clean sandbox loads at once"
        );

        // An edit through the sandbox's own path.
        state.on_sandbox_model_changed();
        assert!(state.sandbox.dirty());
        assert!(state.has_unsaved_edits(), "closing or Exit would lose it");
        state.guarded(Pending::Exit);
        assert!(matches!(state.shell.pending, Some(Pending::Exit)));
        state.shell.pending = None;
        // Loading over it asks first, and answering Replace loads it clean.
        state.guarded(Pending::UserAircraft(saved));
        assert!(matches!(
            state.shell.pending,
            Some(Pending::UserAircraft(_))
        ));
        let waiting = state.shell.pending.take().expect("pending");
        state.perform(waiting);
        assert!(!state.sandbox.dirty());
        // Load configuration asks the same way.
        state.on_sandbox_model_changed();
        state.guarded(Pending::ConfigFile);
        assert!(matches!(state.shell.pending, Some(Pending::ConfigFile)));
        let _ = std::fs::remove_dir_all(root);
    }

    /// A dirty guided case held by a clean Sandbox: Exit asks, a load does not.
    #[test]
    fn the_sandbox_exit_asks_for_the_held_guided_case() {
        let (mut state, root) = edited_guided_state("held-exit");
        std::fs::create_dir_all(&root).expect("dir");
        // A file to load, saved before the edit that stays unsaved.
        state.apply_dialog_result(
            crate::file_dialog::DialogTarget::ConfigSave,
            root.join("case.json"),
        );
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(900.0);
        state.on_config_modified();
        assert!(state.enter_sandbox(true));
        assert!(!state.sandbox.dirty() && state.has_unsaved_edits());
        state.guarded(Pending::Exit);
        assert!(matches!(state.shell.pending, Some(Pending::Exit)));
        state.shell.pending = None;
        state.guarded(Pending::ConfigFile);
        assert!(state.shell.pending.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Loading or saving in the Sandbox works on the sandbox aircraft; the
    /// guided case it holds must still ask before it is replaced or closed.
    #[test]
    fn a_sandbox_load_or_save_leaves_the_held_guided_cases_edits_unsaved() {
        let root =
            std::env::temp_dir().join(format!("alas-confirm-sandbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.load_preset("A320-200");
        assert!(state.save_current_as_user_aircraft("Held Case"));
        let saved = state.user_aircraft().first().expect("saved").path.clone();
        state.reset_undo_baseline();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(805.0);
        state.on_config_modified();
        assert!(state.has_unsaved_edits());
        assert!(state.enter_sandbox(true));
        assert!(state.sandbox.active());

        // A load in the Sandbox runs at once (no prompt) and settles nothing.
        state.guarded(Pending::UserAircraft(saved));
        assert!(state.shell.pending.is_none());
        // Nor does a save from it.
        assert!(state.save_current_as_user_aircraft("Sandbox Copy"));
        assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
        assert!(
            state.has_unsaved_edits(),
            "the guided case is back with its edits, and closing must still prompt"
        );
        state.guarded(Pending::Exit);
        assert!(matches!(state.shell.pending, Some(Pending::Exit)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_corrupt_library_aircraft_keeps_the_edits_even_with_the_log_at_its_cap() {
        let root = std::env::temp_dir().join(format!("alas-confirm-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.load_preset("A320-200");
        assert!(state.library_save(crate::file_library::ObjectKind::Aircraft, "Soon Broken", ""));
        let entry =
            crate::file_library::list(&state.data_dir(), crate::file_library::ObjectKind::Aircraft)
                .into_iter()
                .next()
                .expect("saved");
        std::fs::write(&entry.path, "{ \"alas_object\": ").expect("corrupt");
        state.reset_undo_baseline();
        state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(805.0);
        state.on_config_modified();
        for index in 0..1_600 {
            state.log(format!("line {index}"), LogKind::Info);
        }
        assert!(state.has_unsaved_edits());
        state.guarded(Pending::Library(entry));
        let waiting = state.shell.pending.take().expect("prompt raised");
        state.perform(waiting);
        assert!(
            state.has_unsaved_edits(),
            "a failed library open must not settle the edits"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod prompt_focus_tests {
    use super::*;
    use crate::native_viewport::capture;

    #[test]
    fn the_prompt_pulls_the_root_forward_once_and_blocks_the_detached_windows() {
        let ctx = Context::default();
        let mut state = AppState::default();
        state.shell.pending = Some(Pending::Exit);
        let focus_commands = |output: &egui::FullOutput| {
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .filter(|command| matches!(command, egui::ViewportCommand::Focus))
                .count()
        };
        let first = capture::frame(&ctx, |ctx| show(&mut state, ctx));
        assert_eq!(focus_commands(&first), 1, "the root takes the focus");
        assert!(crate::native_viewport::modal_pending(&ctx));
        let second = capture::frame(&ctx, |ctx| show(&mut state, ctx));
        assert_eq!(focus_commands(&second), 0, "and does not keep stealing it");
        state.shell.pending = None;
        capture::frame(&ctx, |ctx| show(&mut state, ctx));
        assert!(!crate::native_viewport::modal_pending(&ctx));
    }
}
