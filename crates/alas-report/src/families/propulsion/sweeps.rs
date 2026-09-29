// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (figure_propulsion_carpet_plot, figure_propulsion_efficiency_decomposition,
// figure_propulsion_bpr_sensitivity)

//! Parametric trade-space figures: the OPR x TIT carpet plot, efficiency
//! decomposition vs OPR, and specific-thrust/TSFC sensitivity to bypass
//! ratio (dual-axis).

use super::support::axis_labels;
use super::{design_point, linspace, turbofan_spec};
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_prop::cycle::compute_turbofan_cycle;
use alas_prop::cycle::sweeps::{
    compute_bpr_sensitivity, compute_carpet_plot, compute_efficiency_decomposition,
};

mod carpet;
pub use carpet::*;
mod bpr_sensitivity;
pub use bpr_sensitivity::*;
// A test asserts on values it constructed or read off a fixed config here
// directly, so a failed unwrap or expect is the assertion failing, not a
// library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests;
