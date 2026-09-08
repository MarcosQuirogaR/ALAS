// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Composable chart chrome built from [`crate::scene`] primitives: colorbars
//! and legends. Kept separate from `scene` so the primitive scene graph
//! stays backend-neutral while these helpers can grow without bloating it.

include!("chart_kit_parts/part_01.rs");
include!("chart_kit_parts/part_02.rs");
include!("chart_kit_parts/part_03.rs");
