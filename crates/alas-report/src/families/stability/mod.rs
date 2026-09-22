// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Stability and control figures: dynamic modes, static margins, side views,
//! and control surfaces.
//!
//! Split into one file per figure (plus the pieces they share) rather than
//! kept as a single `stability.rs`, following the pattern
//! `crates/alas-geom/src/aircraft/airfoil/` already set:
//!
//! - [`scalars`] -- `_stability_scalars`, the shared geometry/stability read
//!   [`side_view`] and [`metrics`] both compute once from an
//!   [`alas_pipeline::full_analysis::AnalysisReport`] so the two figures agree
//!   with each other.
//! - [`label_rows`] -- `_assign_label_rows`, the greedy label-stacking layout
//!   shared by the same two figures.
//! - [`dynamic_modes`] -- `figure_dynamic_modes`: the s-plane pole plot, fed
//!   by a real `alas-stab::dynamics::compute_dynamic_modes` solve.
//! - [`control_surfaces`] -- `figure_control_surfaces`, and the span/chord
//!   patch geometry helpers (`_le_chord_at_span`, `_span_stations`,
//!   `_cs_surface_patch`, `_cs_surface_area`) it alone uses.
//! - [`side_view`] -- `figure_stability_side_view`: the fuselage-profile
//!   longitudinal stability diagram.
//! - [`metrics`] -- `figure_stability_metrics`: the % MAC number line plus the
//!   Cm-vs-CL polar and metrics table.

#[cfg(test)]
mod figure_contract_tests;
mod label_rows;
mod scalars;
#[cfg(test)]
mod test_support;

pub mod control_surfaces;
pub mod dynamic_modes;
pub mod metrics;
pub mod side_view;

pub use control_surfaces::figure_control_surfaces;
pub use dynamic_modes::figure_dynamic_modes;
pub use metrics::figure_stability_metrics;
pub use side_view::figure_stability_side_view;

use crate::scene::Scene;
use crate::theme::Palette;

/// A status message in place of a figure that cannot be computed from its
/// inputs (missing geometry, a solve that failed to converge, ...), drawn by
/// the shared placeholder in [`crate::status_figure`] rather than a panic or
/// a fabricated value.
pub(crate) fn status_scene(title: &str, message: &str, pal: &Palette) -> Scene {
    crate::status_figure::status_scene(title, message, false, pal)
}
