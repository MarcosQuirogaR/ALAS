// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Interactive scene visualization and rendering into `egui` surfaces.
//!
//! [`render`] converts backend-neutral [`alas_report::scene::Scene`] vector primitives
//! into `egui::Shape` lists.
//!
//! [`raster`] rasterizes the SVG export of a scene, plus its bundled textures,
//! for static previews.
//!
//! [`view`] provides the interactive [`SceneView`] widget with pan, zoom, and fit-to-view.

pub mod raster;
pub mod render;
#[cfg(test)]
mod text_block_tests;
pub mod view;

pub use render::{
    render_scene_to_shapes, render_scene_to_shapes_with_context, to_egui_color, to_egui_stroke,
    ViewportTransform,
};
pub use view::{SceneView, SceneViewState};
