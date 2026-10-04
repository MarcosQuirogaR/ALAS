// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (`figure_geometry` L1018-1077, `figure_planform_comparison` L695-718,
// `figure_design_evolution` L178-220)

//! Top-view planform figures: the three-projection geometry view, a
//! baseline-vs-optimized overlay, and the sampled-design evolution montage.

use super::shared::{airplane_bbox, draw_planform, equal_aspect_ranges};
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::{get_palette, BASELINE_COLOR, OPTIMIZED_COLOR};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_opt::history::OptimizationHistory;

mod outline;
pub use outline::*;
mod evolution;
pub use evolution::*;
#[cfg(test)]
mod tests;
