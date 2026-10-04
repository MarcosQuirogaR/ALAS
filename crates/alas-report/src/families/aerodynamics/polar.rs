// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, `figure_aero_panel`
// (L224-257), `figure_polar_comparison`
// (L283-362) and `figure_drag_breakdown` (L759-861).

use super::support::padded_range;
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette, BASELINE_COLOR, GHOST_COLOR, OPTIMIZED_COLOR};
use alas_pipeline::full_analysis::AnalysisReport;

mod panels;
pub use panels::*;
mod drag_breakdown;
pub use drag_breakdown::*;
