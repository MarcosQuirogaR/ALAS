// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every way to replace the guided case or the sandbox aircraft asks first.
//!
//! The inventory is two tables. `REPLACING_CALLS` lists, per file, the
//! production call sites of the functions that replace an aircraft; a new call
//! site anywhere fails until it is listed here, and the only listed sites
//! outside `shell_confirm.rs` are internal steps of a guarded action. The
//! second table walks the confirmation rule for every action in the four
//! states a user can be in.

use std::path::{Path, PathBuf};

use crate::file_dialog::DialogTarget;
use crate::file_library::ObjectKind;
use crate::shell_confirm::Pending;
use crate::state::AppState;

/// The functions that replace the guided case or the sandbox aircraft.
const REPLACING: [&str; 15] = [
    "load_preset",
    "load_user_aircraft",
    "library_open",
    "library_open_path",
    "load_config",
    "start_clean_sheet",
    "new_sandbox_from_reference",
    "library_restore_autosave",
    "apply_workspace_document",
    "restore_case",
    "library_apply",
    "prior_case.take",
    "resolve_leave_sandbox",
    "library_restore_sandbox_autosave",
    "install_sandbox_design",
];

/// (function, file, production call sites, why this site needs no prompt of
/// its own). `shell_confirm.rs` is `perform`, the one place a guarded action runs.
const REPLACING_CALLS: [(&str, &str, usize, &str); 27] = [
    ("load_preset", "shell_confirm.rs", 1, "perform"),
    (
        "load_preset",
        "perf_tour.rs",
        1,
        "the opt-in performance tour (ALAS_PERF_TOUR) loads its measurement preset once",
    ),
    (
        "load_preset",
        "state/app_state/defaults.rs",
        1,
        "the first preset when the application starts",
    ),
    ("load_user_aircraft", "shell_confirm.rs", 1, "perform"),
    ("library_open", "shell_confirm.rs", 1, "perform"),
    (
        "library_open",
        "file_library_ops.rs",
        1,
        "library_open_path, itself run by perform",
    ),
    ("library_open_path", "shell_confirm.rs", 1, "perform"),
    ("load_config", "shell_confirm.rs", 1, "perform"),
    (
        "load_config",
        "file_library_ops.rs",
        1,
        "applying a configuration file, itself run by perform",
    ),
    ("start_clean_sheet", "shell_confirm.rs", 1, "perform"),
    (
        "new_sandbox_from_reference",
        "shell_confirm.rs",
        1,
        "perform",
    ),
    (
        "new_sandbox_from_reference",
        "sandbox/session.rs",
        1,
        "enter_sandbox(true) while active, which no menu offers",
    ),
    ("library_restore_autosave", "shell_confirm.rs", 1, "perform"),
    (
        "apply_workspace_document",
        "config_edit.rs",
        1,
        "load_config, run by perform",
    ),
    (
        "apply_workspace_document",
        "file_library_ops.rs",
        1,
        "library_apply, run by perform",
    ),
    (
        "apply_workspace_document",
        "file_library_autosave.rs",
        1,
        "the Sandbox autosave restore, run by perform",
    ),
    (
        "apply_workspace_document",
        "user_files_ops.rs",
        1,
        "load_user_aircraft, run by perform",
    ),
    (
        "restore_case",
        "sandbox/session.rs",
        1,
        "leaving the Sandbox with Discard, which has its own prompt",
    ),
    ("restore_case", "sandbox/case.rs", 0, "the definition"),
    (
        "library_apply",
        "file_library_ops.rs",
        1,
        "library_open_path, run by perform",
    ),
    (
        "library_apply",
        "file_library_autosave.rs",
        1,
        "the autosave restore, run by perform",
    ),
    (
        "prior_case.take",
        "sandbox/session.rs",
        2,
        "Discard (restore) and Promote, both behind their own prompts",
    ),
    (
        "resolve_leave_sandbox",
        "shell_confirm.rs",
        1,
        "perform: Promote",
    ),
    (
        "resolve_leave_sandbox",
        "sandbox/windows.rs",
        1,
        "the leave prompt's Discard and Cancel; Promote goes through guarded",
    ),
    (
        "library_restore_sandbox_autosave",
        "shell_confirm.rs",
        1,
        "perform",
    ),
    (
        "install_sandbox_design",
        "sandbox/session.rs",
        1,
        "enter_sandbox: opens the Sandbox, holding the guided case",
    ),
    (
        "install_sandbox_design",
        "sandbox/case.rs",
        1,
        "new_sandbox_from_reference, run by perform",
    ),
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// After an inline test module only items gated by `#[cfg(test)]` may follow
/// (fixtures, more test code). One rule: every non-indented line that is not an
/// attribute, a comment or the close of a bracket starts an item, and the item
/// must have been gated. The rule names no keyword, so no item form slips by.
fn assert_tail_gated(tail: &str) {
    let mut gated = false;
    // The body of a multi-line raw string (a fixture script) is not code.
    let mut in_raw = false;
    for line in tail.lines() {
        let top = line.trim_end();
        if in_raw {
            in_raw = !top.contains("\"#");
            continue;
        }
        if let Some(at) = top.find("r#\"") {
            in_raw = !top[at + 3..].contains("\"#");
        }
        if top.is_empty() || top.starts_with([' ', '\t', '}', ')', ']']) || top.starts_with("//") {
            continue;
        }
        if top.starts_with("#[cfg(test)]") {
            gated = true;
        } else if !top.starts_with("#[") {
            assert!(
                gated,
                "ungated production code follows a test module: {top}"
            );
            gated = false;
        }
    }
}

/// The file up to its inline test module (`#[cfg(test)] mod tests { .. }`);
/// a `#[cfg(test)] mod tests;` declaration or a test-only item does not end it.
fn production_code(text: &str) -> &str {
    let mut offset = 0;
    let mut pending: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            pending = Some(offset);
        } else if let Some(start) = pending {
            // Attributes and comments may sit between the cfg and the item.
            if !(trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty()) {
                let item = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
                if item.starts_with("mod ") && item.trim_end().ends_with('{') {
                    // The convention: after an inline test module only gated items follow.
                    assert_tail_gated(
                        text[offset..]
                            .split_once("\n}\n")
                            .map_or("", |(_, rest)| rest),
                    );
                    return &text[..start];
                }
                pending = None;
            }
        }
        offset += line.len();
    }
    text
}

#[test]
fn every_production_call_of_a_replacing_function_is_an_inventoried_step() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let mut found: Vec<(String, String, usize)> = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&root)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        if relative.ends_with("_tests.rs") || relative.contains("/tests") {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read");
        let text = text.replace("\r\n", "\n");
        let production = production_code(&text);
        for name in REPLACING {
            let calls = production
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .filter(|line| !line.contains(&format!("fn {name}(")))
                .filter(|line| {
                    line.contains(&format!(".{name}(")) || line.contains(&format!(" {name}("))
                })
                .count();
            if calls > 0 {
                found.push((name.to_owned(), relative.clone(), calls));
            }
        }
    }
    found.sort();
    let mut listed: Vec<(String, String, usize)> = REPLACING_CALLS
        .iter()
        .filter(|(_, _, count, _)| *count > 0)
        .map(|(name, file, count, _)| ((*name).to_owned(), (*file).to_owned(), *count))
        .collect();
    listed.sort();
    assert_eq!(
        found, listed,
        "a call of a function that replaces an aircraft was added or moved: route it through \
         AppState::guarded(Pending::..) or list it in REPLACING_CALLS with the reason"
    );
}

/// The four states a user can be in.
#[derive(Clone, Copy, Debug)]
enum Situation {
    /// Guided workspace, clean.
    Clean,
    /// Guided workspace with unsaved edits.
    GuidedDirty,
    /// Sandbox with unsaved sandbox edits, guided case clean.
    SandboxDirty,
    /// Sandbox untouched, holding a guided case with unsaved edits.
    HeldDirty,
}

fn situation(which: Situation) -> (AppState, PathBuf) {
    // One directory per call: threaded runs share the process id.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "alas-inventory-{which:?}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.load_preset("A320-200");
    state.reset_undo_baseline();
    state.shell.undo.mark_saved();
    if matches!(which, Situation::GuidedDirty | Situation::HeldDirty) {
        state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(805.0);
        state.on_config_modified();
    }
    if matches!(which, Situation::SandboxDirty | Situation::HeldDirty) {
        assert!(state.enter_sandbox(true));
    }
    if matches!(which, Situation::SandboxDirty) {
        state.on_sandbox_model_changed();
    }
    (state, root)
}

/// Which table row stands for each action. The match has no wildcard, so a new
/// `Pending` variant does not compile until it is placed here: give it a row in
/// the table below, or name the row whose rule it shares.
fn representative(action: &Pending) -> &'static str {
    match action {
        Pending::Preset(_) | Pending::CleanSheet(_) => "Preset",
        Pending::UserAircraft(_) => "UserAircraft",
        Pending::Library(_) | Pending::Path(_) => "Path",
        Pending::ConfigFile => "ConfigFile",
        Pending::NewSandbox => "NewSandbox",
        Pending::RestoreAutosave(_) => "RestoreAutosave",
        Pending::Promote => "Promote",
        Pending::RestoreSandbox => "RestoreSandbox",
        Pending::Exit => "Exit",
    }
}

/// One instance of every `Pending` variant. A new variant must be added to the
/// `vec!` below as well as to the match after it: the match only forces a look
/// at this function, it does not build the instance.
fn every_variant() -> Vec<Pending> {
    let variants = vec![
        Pending::Preset("A320-200".to_owned()),
        Pending::UserAircraft(PathBuf::from("a.json")),
        Pending::Library(crate::file_library::LibraryEntry {
            kind: ObjectKind::Aircraft,
            name: "A".to_owned(),
            saved_unix: 0,
            path: PathBuf::from("a.json"),
        }),
        Pending::Path(PathBuf::from("a.json")),
        Pending::CleanSheet(crate::config_edit::CleanSheetBrief::default()),
        Pending::ConfigFile,
        Pending::NewSandbox,
        Pending::RestoreAutosave(ObjectKind::Aircraft),
        Pending::Promote,
        Pending::RestoreSandbox,
        Pending::Exit,
    ];
    // A new variant makes this match non-exhaustive until it is added above.
    for variant in &variants {
        match variant {
            Pending::Preset(_)
            | Pending::UserAircraft(_)
            | Pending::Library(_)
            | Pending::Path(_)
            | Pending::CleanSheet(_)
            | Pending::ConfigFile
            | Pending::NewSandbox
            | Pending::RestoreAutosave(_)
            | Pending::Promote
            | Pending::RestoreSandbox
            | Pending::Exit => {}
        }
    }
    variants
}

/// What each action asks in each situation: `(action, [Clean, GuidedDirty,
/// SandboxDirty, HeldDirty])`. Leaving the application asks for either
/// aircraft; everything that replaces an aircraft asks for the one it replaces.
#[test]
fn every_action_asks_exactly_when_it_would_lose_edits() {
    let actions: Vec<(&str, Pending, [bool; 4])> = vec![
        (
            "Preset",
            Pending::Preset("A320-200".to_owned()),
            [false, true, true, false],
        ),
        (
            "UserAircraft",
            Pending::UserAircraft(PathBuf::from("a.json")),
            [false, true, true, false],
        ),
        (
            "Path",
            Pending::Path(PathBuf::from("a.json")),
            [false, true, true, false],
        ),
        (
            "ConfigFile",
            Pending::ConfigFile,
            [false, true, true, false],
        ),
        (
            "NewSandbox",
            Pending::NewSandbox,
            [false, true, true, false],
        ),
        (
            "RestoreAutosave",
            Pending::RestoreAutosave(ObjectKind::Aircraft),
            [false, true, true, false],
        ),
        ("Exit", Pending::Exit, [false, true, true, true]),
        // Promote replaces the guided case the Sandbox holds.
        ("Promote", Pending::Promote, [false, true, false, true]),
        // Restoring the Sandbox's crash copy replaces the sandbox aircraft only.
        (
            "RestoreSandbox",
            Pending::RestoreSandbox,
            [false, false, true, false],
        ),
        // A part of an aircraft is an undoable edit, never a replacement.
        (
            "RestoreAutosave(MissionPlan)",
            Pending::RestoreAutosave(ObjectKind::MissionPlan),
            [false, false, false, false],
        ),
    ];
    // Every variant has a row: one instance of each, whatever its payload.
    let names: Vec<&str> = actions
        .iter()
        .map(|(name, _, _)| name.split('(').next().unwrap_or(name))
        .collect();
    let rows: Vec<(&str, [bool; 4])> = actions
        .iter()
        .map(|(name, _, expected)| (name.split('(').next().unwrap_or(name), *expected))
        .collect();
    for variant in every_variant() {
        let name = representative(&variant);
        assert!(names.contains(&name), "{variant:?} has no row ({name})");
        // Every variant, not only the row's own instance, follows its row.
        let expected = rows
            .iter()
            .find(|(row, _)| *row == name)
            .map(|(_, expected)| *expected)
            .expect("the row exists");
        for (index, which) in [
            Situation::Clean,
            Situation::GuidedDirty,
            Situation::SandboxDirty,
            Situation::HeldDirty,
        ]
        .into_iter()
        .enumerate()
        {
            let (state, root) = situation(which);
            assert_eq!(
                state.needs_confirmation(&variant),
                expected[index],
                "{variant:?} in {which:?}"
            );
            let _ = std::fs::remove_dir_all(root);
        }
    }
    for (name, action, expected) in actions {
        assert_eq!(
            representative(&action),
            name.split('(').next().unwrap_or(name),
            "{name} has its own row"
        );
        for (index, which) in [
            Situation::Clean,
            Situation::GuidedDirty,
            Situation::SandboxDirty,
            Situation::HeldDirty,
        ]
        .into_iter()
        .enumerate()
        {
            let (state, root) = situation(which);
            assert_eq!(
                state.needs_confirmation(&action),
                expected[index],
                "{name} in {which:?}"
            );
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

/// The file-dialog results that load go through the same rule.
#[test]
fn the_loading_dialog_results_ask_before_replacing() {
    for which in [Situation::GuidedDirty, Situation::SandboxDirty] {
        for (target, expected) in [
            (DialogTarget::ConfigLoad, "ConfigFile"),
            (DialogTarget::LibraryOpenPath, "Path"),
        ] {
            let (mut state, root) = situation(which);
            state.apply_dialog_result(target, root.join("case.json"));
            let pending = state.shell.pending.take();
            let kind = match &pending {
                Some(Pending::ConfigFile) => "ConfigFile",
                Some(Pending::Path(_)) => "Path",
                _ => "nothing",
            };
            assert_eq!(kind, expected, "{which:?}");
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

/// Promote keeps the sandbox aircraft as the guided case: with a dirty held
/// case it asks, and answering Replace makes the promoted aircraft the case.
#[test]
fn promoting_over_a_dirty_held_case_asks_first() {
    let (mut state, root) = situation(Situation::HeldDirty);
    state.guarded(Pending::Promote);
    assert!(matches!(state.shell.pending, Some(Pending::Promote)));
    assert!(state.sandbox.active(), "nothing happened yet");
    let waiting = state.shell.pending.take().expect("pending");
    state.perform(waiting);
    assert!(!state.sandbox.active());
    let _ = std::fs::remove_dir_all(root);
    // A clean held case promotes at once.
    let (mut state, root) = situation(Situation::SandboxDirty);
    state.guarded(Pending::Promote);
    assert!(state.shell.pending.is_none() && !state.sandbox.active());
    let _ = std::fs::remove_dir_all(root);
}

/// The guided case and the Sandbox aircraft have their own crash-autosave
/// slots; neither overwrites the other, and both can be restored.
#[test]
fn the_sandbox_and_the_guided_case_autosave_to_their_own_slots() {
    let (mut state, root) = situation(Situation::GuidedDirty);
    let dir = state.data_dir();
    state.files.autosave_enabled = true;
    // The guided edit is pending when the Sandbox opens: it is written first.
    state.files.autosave_since = Some(std::time::Instant::now());
    assert!(state.enter_sandbox(true));
    assert!(state.files.autosave_since.is_none());
    let (_, guided) =
        crate::file_library::read_autosave(&dir, ObjectKind::Aircraft).expect("guided slot");
    assert_eq!(
        guided["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(805.0),
        "the guided edit made just before entering survives a crash"
    );
    // Sandbox edits go to the Sandbox slot only.
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    state.on_sandbox_model_changed();
    assert!(state.files.autosave_since.is_some(), "an edit schedules it");
    let before = std::fs::read(crate::file_library::autosave_path(
        &dir,
        ObjectKind::Aircraft,
    ))
    .expect("guided file");
    state.library_autosave();
    let after = std::fs::read(crate::file_library::autosave_path(
        &dir,
        ObjectKind::Aircraft,
    ))
    .expect("guided file");
    assert_eq!(before, after, "the Sandbox never writes the guided slot");
    let (_, sandbox) = crate::file_library::read_sandbox_autosave(&dir).expect("sandbox slot");
    assert_eq!(
        sandbox["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(777.0)
    );
    // Leaving on purpose retires the copy (autosave is on in this state).
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
    assert!(!crate::file_library::sandbox_autosave_path(&dir).exists());
    // Restoring the guided slot in the guided workspace: the edit comes back,
    // and a recovered copy stays unsaved.
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(900.0);
    state.on_config_modified();
    state.guarded(Pending::RestoreAutosave(ObjectKind::Aircraft));
    assert!(state.shell.pending.is_some(), "it replaces an edited case");
    let waiting = state.shell.pending.take().expect("pending");
    state.perform(waiting);
    assert_eq!(
        state.config_values["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(805.0)
    );
    assert!(state.has_unsaved_edits(), "a recovered copy is not saved");
    // Restoring the Sandbox slot enters the Sandbox with the crash copy and
    // holds the guided case, which stays as it was.
    assert!(crate::file_library::autosave_sandbox(&dir, &sandbox, 1).is_ok());
    assert!(state.library_restore_sandbox_autosave());
    assert!(state.sandbox.active() && state.sandbox.dirty());
    assert_eq!(
        state.config_values["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(777.0)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Restoring the crash copy opens the copy, also when the guided case is a
/// custom design rather than a registered preset.
#[test]
fn the_crash_copy_is_restored_over_a_custom_guided_case() {
    let (mut state, root) = situation(Situation::Clean);
    let dir = state.data_dir();
    assert!(state.enter_sandbox(true));
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    state.on_sandbox_model_changed();
    let crash_copy = state.workspace_document();
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
    // The guided case is a custom design: no registered preset.
    state.active_preset.clear();
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(810.0);
    assert!(crate::file_library::autosave_sandbox(&dir, &crash_copy, 1).is_ok());
    assert!(state.library_restore_sandbox_autosave());
    assert!(state.sandbox.active());
    assert_eq!(
        state.config_values["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(777.0),
        "the crash copy, not the guided case"
    );
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
    assert_eq!(
        state.config_values["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(810.0),
        "and the guided case was held untouched"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Promote writes the promoted aircraft to the guided slot before the crash
/// copy is deleted: no timer has to run in between.
#[test]
fn promote_keeps_the_crash_copy_until_the_guided_slot_holds_the_promoted_aircraft() {
    let (mut state, root) = situation(Situation::Clean);
    let dir = state.data_dir();
    state.files.autosave_enabled = true;
    assert!(state.enter_sandbox(true));
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    state.on_sandbox_model_changed();
    assert!(state.library_autosave());
    assert!(crate::file_library::sandbox_autosave_path(&dir).exists());
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Promote));
    let (_, guided) =
        crate::file_library::read_autosave(&dir, ObjectKind::Aircraft).expect("guided slot");
    assert_eq!(
        guided["mass_model"]["fuel_density_kg_m3"],
        serde_json::json!(777.0),
        "the promoted aircraft is on disk before any timer"
    );
    assert!(!crate::file_library::sandbox_autosave_path(&dir).exists());
    let _ = std::fs::remove_dir_all(root);
}

/// A state that never autosaves never deletes: tests and embedders keep the
/// files of the machine they run on.
#[test]
fn leaving_the_sandbox_touches_no_autosave_file_when_autosave_is_off() {
    let (mut state, root) = situation(Situation::Clean);
    let dir = state.data_dir();
    assert!(!state.files.autosave_enabled);
    assert!(state.enter_sandbox(true));
    assert!(crate::file_library::autosave_sandbox(&dir, &state.workspace_document(), 1).is_ok());
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
    assert!(crate::file_library::sandbox_autosave_path(&dir).exists());
    let _ = std::fs::remove_dir_all(root);
}

/// A fresh session over `root`'s user data, as after a restart: the guided
/// case is the startup preset and clean, autosave is on, and the crash copies
/// already on disk are protected.
fn restarted(root: &Path) -> AppState {
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.load_preset("A320-200");
    state.reset_undo_baseline();
    state.shell.undo.mark_saved();
    state.files.autosave_enabled = true;
    state.protect_existing_autosaves();
    state
}

fn fuel_density(body: &serde_json::Value) -> serde_json::Value {
    body["mass_model"]["fuel_density_kg_m3"].clone()
}

/// A crash copy left by the previous session survives the new session until
/// the user restores or dismisses it: neither Restore Sandbox then Leave nor a
/// guided edit overwrites it.
#[test]
fn the_previous_sessions_crash_copies_survive_until_restored_or_dismissed() {
    let (mut crashed, root) = situation(Situation::GuidedDirty);
    let dir = crashed.data_dir();
    crashed.files.autosave_enabled = true;
    assert!(crashed.library_autosave(), "the guided copy: fuel 805");
    assert!(crashed.enter_sandbox(true));
    crashed.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    crashed.on_sandbox_model_changed();
    assert!(crashed.library_autosave(), "the Sandbox copy: fuel 777");
    drop(crashed);

    // After the restart both slots are protected.
    let mut state = restarted(&root);
    assert!(state.slot_protected("aircraft") && state.slot_protected("sandbox"));
    // A guided edit does not overwrite the previous session's copy.
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(900.0);
    state.on_config_modified();
    assert!(!state.library_autosave(), "the protected slot is skipped");
    let (_, kept) = crate::file_library::read_autosave(&dir, ObjectKind::Aircraft).expect("slot");
    assert_eq!(fuel_density(&kept), serde_json::json!(805.0));
    // Restore Sandbox, then Leave > Discard: the guided copy is untouched.
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(805.0);
    state.shell.undo.mark_saved();
    assert!(state.library_restore_sandbox_autosave());
    assert!(!state.slot_protected("sandbox") && state.slot_protected("aircraft"));
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Discard));
    let (_, kept) = crate::file_library::read_autosave(&dir, ObjectKind::Aircraft).expect("slot");
    assert_eq!(
        fuel_density(&kept),
        serde_json::json!(805.0),
        "Discard writes no guided slot"
    );
    assert!(!crate::file_library::sandbox_autosave_path(&dir).exists());
    // Dismissing the guided copy releases it for new autosaves.
    state.library_dismiss_autosave("aircraft");
    assert!(!state.slot_protected("aircraft"));
    assert!(!crate::file_library::autosave_path(&dir, ObjectKind::Aircraft).exists());
    assert!(!state.library_autosave(), "the other slots are still held");
    state.library_dismiss_autosave("mission_plan");
    state.library_dismiss_autosave("payload_layout");
    assert!(state.library_autosave());
    let _ = std::fs::remove_dir_all(root);
}

/// Promote with the guided slot still held for the user's decision cannot
/// write the promoted aircraft there, so it keeps the Sandbox copy.
#[test]
fn promote_keeps_the_crash_copy_when_the_guided_slot_cannot_be_written() {
    let (mut crashed, root) = situation(Situation::GuidedDirty);
    let dir = crashed.data_dir();
    crashed.files.autosave_enabled = true;
    assert!(crashed.library_autosave());
    drop(crashed);
    let mut state = restarted(&root);
    assert!(state.slot_protected("aircraft"));
    assert!(state.enter_sandbox(true));
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    state.on_sandbox_model_changed();
    assert!(state.library_autosave());
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Promote));
    assert!(
        crate::file_library::sandbox_autosave_path(&dir).exists(),
        "the promoted aircraft is on disk only in the Sandbox copy"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// A recovered guided copy marks the guided case unsaved; inside the Sandbox
/// the same restore marks the sandbox aircraft and leaves the held case alone.
#[test]
fn a_restored_copy_marks_the_aircraft_that_is_active() {
    let (mut state, root) = situation(Situation::Clean);
    let dir = state.data_dir();
    state.files.autosave_enabled = true;
    assert!(state.library_autosave());
    assert!(crate::file_library::read_autosave(&dir, ObjectKind::Aircraft).is_some());
    assert!(state.enter_sandbox(true));
    state.perform(Pending::RestoreAutosave(ObjectKind::Aircraft));
    assert!(
        state.sandbox.dirty(),
        "the sandbox aircraft is the unsaved one"
    );
    assert!(!state.shell.undo.is_dirty(), "the held case stays clean");
    let _ = std::fs::remove_dir_all(root);
}

/// When the guided slots cannot be written, Promote keeps the Sandbox copy:
/// the promoted aircraft would otherwise exist nowhere on disk.
#[test]
fn promote_keeps_the_crash_copy_when_the_guided_autosave_fails() {
    let (mut state, root) = situation(Situation::Clean);
    let dir = state.data_dir();
    state.files.autosave_enabled = true;
    // A directory where the guided slot's file belongs: its write fails.
    std::fs::create_dir_all(crate::file_library::autosave_path(
        &dir,
        ObjectKind::Aircraft,
    ))
    .expect("blocking directory");
    assert!(state.enter_sandbox(true));
    state.config_values["mass_model"]["fuel_density_kg_m3"] = serde_json::json!(777.0);
    state.on_sandbox_model_changed();
    assert!(state.library_autosave(), "the Sandbox slot is writable");
    assert!(crate::file_library::sandbox_autosave_path(&dir).exists());
    assert!(state.resolve_leave_sandbox(crate::sandbox::ExitChoice::Promote));
    assert!(
        crate::file_library::sandbox_autosave_path(&dir).exists(),
        "the guided write failed, so the Sandbox copy stays"
    );
    // The failure is the blocked guided slot, not a protected Sandbox slot.
    assert!(!state.slot_protected("sandbox"));
    assert!(crate::file_library::autosave_path(&dir, ObjectKind::Aircraft).is_dir());
    assert!(
        !state.library_autosave(),
        "the guided slot still cannot be written"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// An ungated `impl<T>` or `extern` item after an inline test module is caught,
/// whatever its form.
#[test]
#[should_panic(expected = "ungated production code follows a test module")]
fn the_scanner_rejects_ungated_items_after_a_test_module() {
    production_code(
        "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n\nimpl<T> X<T> {\n}\n\npub extern \"C\" fn f() {}\n",
    );
}

/// The same items, gated, pass; and the code before the module is what is returned.
#[test]
fn the_scanner_accepts_gated_items_after_a_test_module() {
    let text = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n\n#[cfg(test)]\nimpl<T> X<T> {\n}\n\n#[cfg(test)]\n#[allow(dead_code)]\npub extern \"C\" fn f() {}\n";
    assert_eq!(production_code(text), "fn prod() {}\n");
}
