// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The clean-sheet Sandbox workspace.
//!
//! The sandbox is a second case inside the desktop state: its own editable
//! aircraft, design vector, run log and results, swapped in and out of the
//! shared [`crate::state::AppState`] so every existing view keeps reading
//! one authoritative configuration while the guided case is preserved.

pub mod drag;
pub mod fields;
pub mod persist;
pub mod policy;
pub mod quick;
pub mod scene;
pub mod session;
pub mod undo;

pub use session::{
    ExitChoice, SandboxDesign, SandboxLayout, SandboxSession, StartingDesign, WorkspaceMode,
};
pub mod advanced;
pub mod case;
pub mod editors;
pub mod estimates;
pub mod fuselage_editor;
pub mod panel;
pub mod viewport;
pub mod windows;
pub mod workspace;
