// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Composable chart chrome built from [`crate::scene`] primitives: colorbars
//! and legends. Kept separate from `scene` so the primitive scene graph
//! stays backend-neutral while these helpers can grow without bloating it.

use crate::colormap::Colormap;
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::Palette;

mod axes;
pub use axes::*;
mod legends;
pub use legends::*;
#[cfg(test)]
mod tests;
