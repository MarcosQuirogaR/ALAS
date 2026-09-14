// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Session-wide undo and redo for the sandbox.
//!
//! Every committed edit (a form field, a direct-manipulation drag, a reset)
//! records the state before it. A drag is one transaction: the state before
//! the pointer went down is recorded once, and the intermediate pointer
//! positions are never stored, so undo returns to the pre-drag geometry in
//! one step. The history lives only for the current session; it is never
//! persisted.

use std::collections::BTreeMap;

use serde_json::Value;

/// The editable state one undo step restores.
#[derive(Debug, Clone, PartialEq)]
pub struct EditSnapshot {
    /// The configuration edit buffer.
    pub config_values: Value,
    /// The design vector by variable name.
    pub design_values: BTreeMap<String, f64>,
}

/// The undo and redo stacks.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<EditSnapshot>,
    redo: Vec<EditSnapshot>,
    transaction: Option<EditSnapshot>,
    capacity: usize,
}

impl UndoStack {
    /// A stack retaining at most `capacity` undo steps.
    pub fn new(capacity: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            transaction: None,
            capacity: capacity.max(1),
        }
    }

    /// Record the state before one committed edit.
    pub fn record(&mut self, before: EditSnapshot) {
        if self.transaction.is_some() {
            // Edits inside a transaction belong to it; the pre-transaction
            // state is already retained.
            return;
        }
        self.push_undo(before);
        self.redo.clear();
    }

    /// Start a drag: retain the state before the first pointer motion.
    pub fn begin_transaction(&mut self, before: EditSnapshot) {
        if self.transaction.is_none() {
            self.transaction = Some(before);
        }
    }

    /// Whether a drag transaction is open.
    pub fn in_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// End a drag. The pre-drag state becomes one undo step when the drag
    /// changed something; a drag that ended where it started records nothing.
    pub fn commit_transaction(&mut self, changed: bool) {
        if let Some(before) = self.transaction.take() {
            if changed {
                self.push_undo(before);
                self.redo.clear();
            }
        }
    }

    /// Whether current differs from the state retained at the start of
    /// the open transaction.
    pub fn transaction_differs(&self, current: &EditSnapshot) -> bool {
        self.transaction
            .as_ref()
            .is_some_and(|before| before != current)
    }

    /// Abandon a drag without recording it.
    pub fn cancel_transaction(&mut self) -> Option<EditSnapshot> {
        self.transaction.take()
    }

    /// Step back: `current` moves to the redo stack and the previous state
    /// is returned for the caller to apply.
    pub fn undo(&mut self, current: EditSnapshot) -> Option<EditSnapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Step forward after an undo.
    pub fn redo(&mut self, current: EditSnapshot) -> Option<EditSnapshot> {
        let next = self.redo.pop()?;
        self.push_undo(current);
        Some(next)
    }

    /// Whether an undo step exists.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a redo step exists.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Retained undo steps.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Forget every step, for a new session.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.transaction = None;
    }

    fn push_undo(&mut self, snapshot: EditSnapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > self.capacity {
            let overflow = self.undo.len() - self.capacity;
            self.undo.drain(0..overflow);
        }
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(span: f64) -> EditSnapshot {
        let mut design_values = BTreeMap::new();
        design_values.insert("span_m".to_owned(), span);
        EditSnapshot {
            config_values: serde_json::json!({ "span": span }),
            design_values,
        }
    }

    #[test]
    fn undo_and_redo_walk_the_recorded_history() {
        let mut stack = UndoStack::new(10);
        stack.record(snapshot(1.0));
        stack.record(snapshot(2.0));
        let current = snapshot(3.0);
        assert!(stack.can_undo() && !stack.can_redo());
        let back = stack.undo(current.clone()).expect("one step back");
        assert_eq!(back, snapshot(2.0));
        assert!(stack.can_redo());
        let forward = stack.redo(back).expect("one step forward");
        assert_eq!(forward, current);
        assert!(!stack.can_redo());
    }

    #[test]
    fn a_new_edit_after_undo_discards_the_redo_branch() {
        let mut stack = UndoStack::new(10);
        stack.record(snapshot(1.0));
        let _ = stack.undo(snapshot(2.0));
        assert!(stack.can_redo());
        stack.record(snapshot(1.0));
        assert!(!stack.can_redo());
    }

    #[test]
    fn a_drag_is_one_undo_step_regardless_of_intermediate_motion() {
        let mut stack = UndoStack::new(10);
        stack.begin_transaction(snapshot(10.0));
        for intermediate in [11.0, 12.0, 13.0] {
            stack.record(snapshot(intermediate));
        }
        stack.commit_transaction(true);
        assert_eq!(stack.undo_depth(), 1);
        let restored = stack.undo(snapshot(14.0)).expect("pre-drag state");
        assert_eq!(restored, snapshot(10.0));
    }

    #[test]
    fn an_unchanged_drag_records_nothing() {
        let mut stack = UndoStack::new(10);
        stack.begin_transaction(snapshot(10.0));
        stack.commit_transaction(false);
        assert!(!stack.can_undo());
    }

    #[test]
    fn the_capacity_drops_the_oldest_steps() {
        let mut stack = UndoStack::new(3);
        for span in [1.0, 2.0, 3.0, 4.0, 5.0] {
            stack.record(snapshot(span));
        }
        assert_eq!(stack.undo_depth(), 3);
        let restored = stack.undo(snapshot(6.0)).expect("latest");
        assert_eq!(restored, snapshot(5.0));
    }
}
