// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one spacing and type-size system of the desktop GUI, in points.
//!
//! Every card, card and section header, field caption and form grid takes its
//! spacing from these tokens; a page keeps no spacing constant of its own. A
//! caption sits closer to its own value box ([`LABEL_TO_BOX`]) than to the
//! box of the row above ([`ROW_GAP`]), so it reads as belonging to its box.

use egui::{vec2, Vec2};

/// From the bottom of a card or section header to the top of its first field.
pub const HEADER_TO_BODY: f32 = 6.0;

/// From the last line of a field caption to the top of its value box.
pub const LABEL_TO_BOX: f32 = 4.0;

/// Room kept at the right end of a caption's last line for the markers of its
/// box, whether or not they show, so a field that turns modified never
/// re-wraps its caption: the reset glyph's framed button (an 18.4 pt glyph at
/// the 15.5 pt body size, 3 pt padding a side), the 7 pt modified dot and two
/// 4 pt gaps take 39.4 pt; a lock and the dot take less.
pub const MARKER_ROOM: f32 = 40.0;

/// From the bottom of a grid row (its tallest cell) to the next row's captions.
pub const ROW_GAP: f32 = 12.0;

/// Between two columns of a grid.
pub const COLUMN_GUTTER: f32 = 16.0;

/// Inner margin of a card: horizontal, vertical.
pub const CARD_PADDING: Vec2 = vec2(14.0, 10.0);

/// Size of the body text, the `Body` text style every label and value uses.
pub const BODY_TEXT: f32 = 15.5;

/// Size the smallest text of a figure in a card (a tick label, a legend
/// entry) is drawn at: never smaller than the body text beside the figure.
/// Every card figure takes its scale from it (see [`super::figure`]).
pub const FIGURE_TEXT: f32 = BODY_TEXT;

/// Spacing of every label-and-value grid, column then row, in points. The
/// form grid and the plain `egui::Grid` tables of the result and tool pages
/// read it, so no page keeps a spacing pair of its own.
pub const GRID_SPACING: Vec2 = vec2(12.0, 4.0);
