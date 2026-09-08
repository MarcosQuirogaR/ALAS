// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generic form, rendered straight from a configuration group's schema.
//!
//! A port of the reference desktop app's `DynamicForm`: it walks the [`Field`]
//! list the derive macro emits and edits the matching `serde_json::Value` in
//! place, so every group's every field appears with zero per-field code and a
//! new field on the model side needs none here. Field prose is looked up in the
//! active language the way the schema documents.

include!("form_parts/part_01.rs");
include!("form_parts/part_02.rs");
