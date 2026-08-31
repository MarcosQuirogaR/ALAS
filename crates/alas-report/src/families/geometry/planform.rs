// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (`figure_geometry` L1018-1077, `figure_planform_comparison` L695-718,
// `figure_design_evolution` L178-220)
// Reference: alas @ rust-port-baseline.

//! Top-view planform figures: the three-projection geometry view, a
//! baseline-vs-optimized overlay, and the sampled-design evolution montage.

include!("planform_parts/part_01.rs");
include!("planform_parts/part_02.rs");
