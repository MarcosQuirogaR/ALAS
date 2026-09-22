// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Central application state.
//!
//! The edited configuration is held as a `serde_json::Value`: the same shape
//! the reference desktop app's React `configValues` had, rather than as a
//! typed [`AlasConfig`]. That is what lets one generic form render every field
//! of every group straight from the schema the derive macro emits, instead of
//! a hand-written control per field that drifts from the model. The typed
//! configuration is recovered with [`AppState::typed_config`] wherever a
//! discipline actually needs it: validation, the live preview, and a run.
//!
//! The struct and its bookkeeping live here; editing a configuration lives in
//! [`crate::config_edit`] and running the pipeline in [`crate::run`], both as
//! further `impl AppState` blocks: kept in their own files so this one stays
//! under the project's line limit.

include!("state_parts/part_01.rs");
include!("state_parts/part_02.rs");
