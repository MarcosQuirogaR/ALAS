// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (figure_propulsion_carpet_plot, figure_propulsion_efficiency_decomposition,
// figure_propulsion_bpr_sensitivity)
// Reference: alas @ rust-port-baseline.

//! Parametric trade-space figures: the OPR x TIT carpet plot, efficiency
//! decomposition vs OPR, and specific-thrust/TSFC sensitivity to bypass
//! ratio (dual-axis).

include!("sweeps_parts/part_01.rs");
include!("sweeps_parts/part_02.rs");
