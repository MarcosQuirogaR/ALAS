// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure-scene construction for the live preview, the per-page side previews,
//! and the results gallery.
//!
//! Kept out of [`crate::state`] so that module stays the data hub: these
//! functions read a whole [`AppState`] and return the [`Scene`] a viewport then
//! draws. Everything here degrades to `None` rather than panicking -- a figure
//! that needs a completed run, or a geometry that will not build mid-edit, is a
//! blank slot, not a crash.

include!("scene_parts/part_01.rs");
include!("scene_parts/part_02.rs");
