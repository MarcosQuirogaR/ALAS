// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Assemble the cabin scene from a completed analysis.

mod recommendations;

use recommendations::recommended_sections;

include!("build_parts/part_01.rs");
include!("build_parts/part_02.rs");
