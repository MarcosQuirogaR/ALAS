// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_cabin_payload` and
// `figure_cabin_payload_3d`)

//! Draw the detailed payload layout from the shared layout engine.

use super::cabin_seat_map::draw_main_deck_services;
use super::shared::equal_aspect_ranges;
use crate::chart_kit::{draw_horizontal_legend, LegendMarker};
use crate::scene::Axes2D;
use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_payload::geometry::CabinGeometry;
use alas_payload::layout::{DeckItem, ItemKind, ItemMeta, LayoutSummary, PayloadLayout, SeatMeta};

mod payload;
pub use payload::*;
mod seat_map;
pub use seat_map::*;
#[cfg(test)]
mod tests;
