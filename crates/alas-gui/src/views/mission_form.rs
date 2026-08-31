// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission Analysis editor with segment-level hierarchy and live feedback.
//!
//! The native mission solver has a defined schedule rather than an arbitrary
//! free-form route. This editor exposes every supported segment separately,
//! lets the user choose how many cruise legs and descent rungs are active,
//! and keeps the live schematic adjacent to those decisions.

include!("mission_form_parts/part_01.rs");
include!("mission_form_parts/part_02.rs");
