// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Central application state.
//!
//! The edited configuration is held as a `serde_json::Value` rather than as a
//! typed [`AlasConfig`]. That is what lets one generic form render every field
//! of every group straight from the schema the derive macro emits, instead of
//! a hand-written control per field that drifts from the model. The typed
//! configuration is recovered with [`AppState::typed_config`] wherever a
//! discipline actually needs it: validation, the live preview, and a run.
//!
//! The struct and its bookkeeping live here; editing a configuration lives in
//! [`crate::config_edit`] and running the pipeline in [`crate::run`], both as
//! further `impl AppState` blocks in their own files.

mod accessors;
mod app_state;
#[cfg(test)]
mod tests;
mod tools;
mod types;

pub(crate) mod state_memo;

pub use crate::nav_overlay::{nav_overlay_open, nav_overlay_open_with_bounds};
pub use crate::viewport::PreviewCamera;
pub use app_state::AppState;
pub(crate) use app_state::MAX_LOG_LINES;
pub use types::{
    Language, LogKind, LogLine, PreviewTab, RunLogTab, RunOptions, WalkthroughRestore,
    WorkerMessage, AIRCRAFT_PREVIEW_CAMERA_ID,
};
