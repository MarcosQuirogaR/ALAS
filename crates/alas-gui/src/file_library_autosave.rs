// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Crash-autosave slots of the file library: the guided case's kinds and the
//! Sandbox aircraft's own slot, the protection of a previous session's copies
//! until the user restores or dismisses them, and their restore.

use std::path::PathBuf;

use serde_json::Value;

use crate::file_library::{self, ObjectKind};
use crate::file_library_ops::Verb;
use crate::state::{AppState, LogKind};
use crate::user_files::now_unix;
use crate::views::tr_fields;

impl AppState {
    /// Write the autosave slot of every kind that has something to save.
    /// Whether every slot that was due was written; a slot left by an earlier
    /// session that is still awaiting the user's decision is never written.
    pub fn library_autosave(&mut self) -> bool {
        let dir = self.data_dir();
        // In the Sandbox the document is the sandbox aircraft: it goes to its
        // own slot, and the guided slots keep the case the Sandbox holds.
        if self.sandbox.active() {
            return !self.slot_protected("sandbox")
                && file_library::autosave_sandbox(&dir, &self.workspace_document(), now_unix())
                    .is_ok();
        }
        let mut written = true;
        for kind in [
            ObjectKind::Aircraft,
            ObjectKind::MissionPlan,
            ObjectKind::PayloadLayout,
        ] {
            if self.slot_protected(kind.id()) {
                written = false;
            } else if let Ok(body) = self.library_capture(kind, "") {
                written &= file_library::autosave(&dir, kind, &body, now_unix()).is_ok();
            }
        }
        written
    }

    /// Whether the autosave slot `slot` still holds an earlier session's copy
    /// that the user has neither restored nor dismissed.
    pub(crate) fn slot_protected(&self, slot: &str) -> bool {
        self.files.protected_slots.iter().any(|s| s == slot)
    }

    /// At startup: every autosave file already on disk is the previous
    /// session's crash copy. It stays untouched until restored or dismissed.
    pub fn protect_existing_autosaves(&mut self) {
        let dir = self.data_dir();
        let mut slots: Vec<&str> = [
            ObjectKind::Aircraft,
            ObjectKind::MissionPlan,
            ObjectKind::PayloadLayout,
        ]
        .into_iter()
        .filter(|kind| file_library::autosave_path(&dir, *kind).exists())
        .map(ObjectKind::id)
        .collect();
        if file_library::sandbox_autosave_path(&dir).exists() {
            slots.push("sandbox");
        }
        if !slots.is_empty() {
            self.files.protected_slots = slots.into_iter().map(str::to_owned).collect();
            self.log(
                crate::views::tr(
                    "Crash copies of the previous session were found: restore or dismiss them under Files > Autosave.",
                ),
                LogKind::Warn,
            );
        }
    }

    /// The user has decided about the slot `slot`: it may be overwritten again.
    pub(crate) fn release_slot(&mut self, slot: &str) {
        self.files.protected_slots.retain(|s| s != slot);
    }

    /// Delete the previous session's copy in `slot` on the user's say-so.
    pub fn library_dismiss_autosave(&mut self, slot: &str) {
        let dir = self.data_dir();
        let path = match ObjectKind::ALL.into_iter().find(|kind| kind.id() == slot) {
            Some(kind) => file_library::autosave_path(&dir, kind),
            None => file_library::sandbox_autosave_path(&dir),
        };
        let _ = std::fs::remove_file(path);
        self.release_slot(slot);
    }

    /// Restore the Sandbox's crash-autosave slot: the Sandbox opens with that
    /// aircraft and holds the guided case.
    pub fn library_restore_sandbox_autosave(&mut self) -> bool {
        let Some((_, body)) = file_library::read_sandbox_autosave(&self.data_dir()) else {
            return false;
        };
        match self.apply_workspace_document(&body) {
            Ok(()) => {
                // A recovered crash copy is not on disk as a saved file: it
                // stays unsaved until the user saves it or sets it aside.
                self.sandbox.saved_revision = self.sandbox.revision.wrapping_sub(1);
                self.release_slot("sandbox");
                let message = crate::views::tr("Opened the Sandbox autosave.");
                self.files.status = Some(message.clone());
                self.log(message, LogKind::Info);
                true
            }
            Err(error) => {
                let message =
                    tr_fields("Sandbox autosave not opened: {error}", &[("error", error)]);
                self.files.status = Some(message.clone());
                self.log(message, LogKind::Error);
                false
            }
        }
    }

    /// Restore the autosave slot of `kind`.
    pub fn library_restore_autosave(&mut self, kind: ObjectKind) -> bool {
        let Some((_, body)) = file_library::read_autosave(&self.data_dir(), kind) else {
            return false;
        };
        let result = self.library_apply(kind, &body).map(|()| PathBuf::new());
        let opened = self.library_report(kind, "Autosave", result, Verb::Opened);
        if opened {
            self.release_slot(kind.id());
        }
        opened
    }
}

impl AppState {
    /// Note that the case changed, so the shell autosaves it after a pause.
    pub(crate) fn note_autosave_due(&mut self) {
        self.note_edit_for_undo();
        if self.files.autosave_enabled {
            self.files
                .autosave_since
                .get_or_insert_with(std::time::Instant::now);
        }
    }
}

/// How many differing paths a restore prompt lists.
const DIFF_PATHS_SHOWN: usize = 5;

/// Paths (JSON-pointer style) where `saved` differs from `current`: the leaf
/// values that differ, and the node itself where an array changed length or a
/// value changed type.
pub(crate) fn changed_paths(current: &Value, saved: &Value) -> Vec<String> {
    fn walk(path: &str, current: &Value, saved: &Value, out: &mut Vec<String>) {
        match (current, saved) {
            (Value::Object(a), Value::Object(b)) => {
                let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
                keys.sort();
                keys.dedup();
                for key in keys {
                    let child = format!("{path}/{key}");
                    match (a.get(key), b.get(key)) {
                        (Some(x), Some(y)) => walk(&child, x, y, out),
                        _ => out.push(child),
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (index, (x, y)) in a.iter().zip(b).enumerate() {
                    walk(&format!("{path}/{index}"), x, y, out);
                }
            }
            _ => {
                if current != saved {
                    out.push(if path.is_empty() {
                        "/".to_owned()
                    } else {
                        path.to_owned()
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    walk("", current, saved, &mut out);
    out
}

impl AppState {
    /// A short summary of how the autosave in `slot` differs from the current
    /// case (changed-field count and the first paths), or `None` when the
    /// slot cannot be compared (the Sandbox slot, or no readable autosave).
    pub(crate) fn autosave_diff_summary(&self, slot: &str) -> Option<String> {
        let kind = ObjectKind::ALL.into_iter().find(|kind| kind.id() == slot)?;
        let (_, saved) = file_library::read_autosave(&self.data_dir(), kind)?;
        let current = self.library_capture(kind, "").ok()?;
        let changed = changed_paths(&current, &saved);
        if changed.is_empty() {
            return Some(crate::views::tr("The autosave matches the current case."));
        }
        let shown: Vec<&str> = changed
            .iter()
            .take(DIFF_PATHS_SHOWN)
            .map(String::as_str)
            .collect();
        let more = changed.len().saturating_sub(DIFF_PATHS_SHOWN);
        let mut text = tr_fields(
            "{count} fields differ: {paths}",
            &[
                ("count", changed.len().to_string()),
                ("paths", shown.join(", ")),
            ],
        );
        if more > 0 {
            text.push_str(&tr_fields(
                " and {more} more",
                &[("more", more.to_string())],
            ));
        }
        Some(text)
    }
}

#[cfg(test)]
mod diff_tests {
    use super::changed_paths;
    use serde_json::json;

    #[test]
    fn changed_paths_lists_differing_leaves() {
        let current = json!({"a": {"x": 1, "y": [1, 2]}, "b": "same"});
        let saved = json!({"a": {"x": 2, "y": [1, 3]}, "b": "same", "c": 0});
        assert_eq!(changed_paths(&current, &saved), ["/a/x", "/a/y/1", "/c"]);
        assert!(changed_paths(&current, &current).is_empty());
    }
}
