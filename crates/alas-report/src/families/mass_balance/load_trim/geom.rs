// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Canvas geometry of the load-and-trim sheet.
//!
//! The report draws the sheet on a fixed canvas, chart left and a narrow
//! panel (key and tables) right. The desktop draws it on a canvas the size of
//! its card, in points, so every text keeps a legible size at any window
//! width: a wide card puts the panel beside the chart, a narrow one stacks it
//! below. The plot keeps a fixed aspect, so a weight-and-balance envelope is
//! never stretched with the window.

use crate::scene::CSS_PIXELS_PER_POINT;

/// Left margin of the chart: weight tick labels and the rotated axis title.
const LEFT_MARGIN: f64 = 92.0;
/// Space between the chart frame and a panel beside it, px.
const PANEL_GAP: f64 = 28.0;
/// Space between the canvas's right edge and a panel beside the chart, px.
const RIGHT_MARGIN: f64 = 22.0;
/// Margin of a stacked sheet on either side of the canvas, px.
const STACKED_MARGIN: f64 = 14.0;
/// Room below the frame for the CG tick labels and the axis title, px.
const BELOW_FRAME: f64 = 58.0;
/// Room above the frame for the title and the reference line, px.
const ABOVE_FRAME: f64 = 66.0;
/// Cards narrower than this stack the panel below the chart.
const SIDE_PANEL_MIN_WIDTH: f64 = 900.0;
/// Smallest text of a sheet drawn for a card, in points.
const SIZED_MIN_FONT_PT: f64 = 9.6;
/// Text scale of a sheet drawn for a card.
const SIZED_FONT_SCALE: f64 = 0.84;
/// Plot height over plot width.
const PLOT_ASPECT: f64 = 0.80;

/// Where the pieces of one sheet sit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geom {
    pub(crate) width: f64,
    /// Canvas height without the panel growing it, px.
    pub(crate) height: f64,
    /// Chart frame `(left, top, width, height)`.
    pub(crate) chart: (f64, f64, f64, f64),
    /// Panel `(left, top, width)`; stacked, it starts below the chart.
    pub(crate) panel: (f64, f64, f64),
    /// Text scale applied to every font of the chart and the panel.
    scale: f64,
    /// Smallest font, pt; zero for no floor.
    min_font: f64,
    /// Whether the panel stands below the chart.
    pub(crate) stacked: bool,
}

impl Geom {
    /// The report's fixed canvas.
    pub(crate) fn standard() -> Self {
        let width = 1380.0;
        let panel_w = 460.0;
        let chart_w = width - LEFT_MARGIN - PANEL_GAP - panel_w - RIGHT_MARGIN;
        let chart_h = (chart_w * PLOT_ASPECT).round();
        Self {
            width,
            height: ABOVE_FRAME + chart_h + BELOW_FRAME,
            chart: (LEFT_MARGIN, ABOVE_FRAME, chart_w, chart_h),
            panel: (LEFT_MARGIN + chart_w + PANEL_GAP, ABOVE_FRAME, panel_w),
            scale: 1.0,
            min_font: 0.0,
            stacked: false,
        }
    }

    /// A canvas of `width` points for a desktop card. The canvas height is
    /// the chart's plus, for a stacked panel, the panel's own `panel_height`.
    pub(crate) fn sized(width: f64, panel_height: f64) -> Self {
        let width = width.clamp(420.0, 2400.0);
        let stacked = width < SIDE_PANEL_MIN_WIDTH;
        if stacked {
            let chart_w = width - LEFT_MARGIN - STACKED_MARGIN;
            let chart_h = (chart_w * 1.0).clamp(300.0, 760.0);
            let panel_top = ABOVE_FRAME + chart_h + BELOW_FRAME + 6.0;
            Self {
                width,
                height: panel_top + panel_height + STACKED_MARGIN,
                chart: (LEFT_MARGIN, ABOVE_FRAME, chart_w, chart_h),
                panel: (STACKED_MARGIN, panel_top, width - 2.0 * STACKED_MARGIN),
                scale: SIZED_FONT_SCALE,
                min_font: SIZED_MIN_FONT_PT,
                stacked,
            }
        } else {
            let panel_w = (width * 0.33).clamp(330.0, 480.0);
            let chart_w = width - LEFT_MARGIN - PANEL_GAP - panel_w - RIGHT_MARGIN;
            let chart_h = (chart_w * PLOT_ASPECT).clamp(360.0, 760.0);
            let plot_bottom = ABOVE_FRAME + chart_h + BELOW_FRAME;
            Self {
                width,
                height: plot_bottom.max(ABOVE_FRAME + panel_height + STACKED_MARGIN),
                chart: (LEFT_MARGIN, ABOVE_FRAME, chart_w, chart_h),
                panel: (LEFT_MARGIN + chart_w + PANEL_GAP, ABOVE_FRAME, panel_w),
                scale: SIZED_FONT_SCALE,
                min_font: SIZED_MIN_FONT_PT,
                stacked,
            }
        }
    }

    /// A font size of the standard sheet as drawn on this canvas, pt.
    pub(crate) fn font(&self, standard_pt: f64) -> f64 {
        (standard_pt * self.scale).max(self.min_font)
    }
}

/// Estimated width, px, of `text` at `font_pt` (an upper-ish mean advance of
/// 0.58 em, so a label placed with it never runs into its neighbour).
pub(crate) fn text_width(text: &str, font_pt: f64) -> f64 {
    text.chars().count() as f64 * font_pt * CSS_PIXELS_PER_POINT * 0.58
}

/// Height, px, of one line of text at `font_pt`.
pub(crate) fn text_height(font_pt: f64) -> f64 {
    font_pt * CSS_PIXELS_PER_POINT * 1.2
}
