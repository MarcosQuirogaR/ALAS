// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Aerodynamic figures: polars, drag breakdowns, VLM-derived spanwise loading
//! and wake flow, MSES section results, and the NeuralFoil Reynolds/alpha
//! sweep.
//!
//! Split into topical submodules, following the pattern
//! `crates/alas-geom/src/aircraft/airfoil/` already set, so no single file grows
//! past this crate's 700-line limit: [`polar`] for the report-fed polar
//! panels (`figure_aero_panel`, `figure_polar_comparison`,
//! `figure_drag_breakdown`), [`model_comparison`] for the native-model/MSES
//! overlay, [`mses`] for the two section-level MSES figures, [`vlm`] for
//! the two figures that need a fresh vortex-lattice solve
//! (`figure_span_loading`, `figure_vlm_flow`), [`reynolds`] for the NeuralFoil
//! contour sweep, and [`status`] for the generic placeholder Python falls
//! back to when an optional analysis is unavailable. [`support`] holds what
//! more than one of them needs: padded axis ranges computed from real data
//! and NaN-aware heatmap-cell binning.

mod drag_preview;
mod model_comparison;
mod mses;
mod polar;
mod reynolds;
mod status;
mod support;
mod vlm;

pub use drag_preview::figure_drag_preview;
pub use model_comparison::{figure_model_comparison, figure_optimized_aircraft_comparison};
pub use mses::{figure_mses_mach_contours, figure_mses_pressure_distribution};
pub use polar::{figure_aero_panel, figure_drag_breakdown, figure_polar_comparison};
pub use reynolds::figure_airfoil_reynolds;
pub use status::figure_status_message;
pub use vlm::{figure_span_loading, figure_vlm_flow};
