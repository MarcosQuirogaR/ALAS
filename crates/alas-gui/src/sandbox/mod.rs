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
mod handle_cache;
mod hump_defaults;
pub mod persist;
pub mod policy;
pub mod quick;
pub mod scene;
pub mod search;
pub mod session;
pub mod undo;

pub use session::{
    ExitChoice, SandboxDesign, SandboxLayout, SandboxSession, StartingDesign, WorkspaceMode,
};
pub mod advanced;
pub mod built_sections;
pub mod camera_row;
pub mod case;
pub mod editors;
pub mod estimates;
mod feasibility_cards;
pub mod framing;
mod list_editors;
#[cfg(test)]
mod live_input_tests;
pub mod navigation;
pub mod overlays;
pub mod panel;
pub mod section_views;
pub mod viewport;
pub mod windows;
pub mod workspace;
