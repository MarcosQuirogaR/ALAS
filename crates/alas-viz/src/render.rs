// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversion of [`Scene`] elements into `egui` painter shapes.

mod backend;
mod polygon;
mod transform;

pub use transform::{to_egui_color, to_egui_stroke, ViewportTransform};

use alas_report::scene::{visual_title, Scene};
use egui::Shape;

use backend::EguiBackend;

/// Render all elements of a [`Scene`] to a list of [`Shape`] primitives ready for egui painting.
pub fn render_scene_to_shapes(scene: &Scene, transform: &ViewportTransform) -> Vec<Shape> {
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |_| {});
    render_scene_to_shapes_with_context(scene, transform, &context)
}

/// Render a scene using the font atlas of the context that will paint it.
///
/// A [`TextShape`] contains a galley whose texture identifiers belong to the
/// context that laid it out. The desktop path must therefore use the active UI
/// context rather than a private one, or axes and annotations become invisible
/// even though their shapes exist.
pub fn render_scene_to_shapes_with_context(
    scene: &Scene,
    transform: &ViewportTransform,
    context: &egui::Context,
) -> Vec<Shape> {
    let mut backend = EguiBackend::new(scene, transform, context.clone());

    if let (true, Some(bg)) = (scene.paint_background, scene.background) {
        backend.fill_rect(
            backend.snapped([0.0, 0.0]),
            backend.snapped([scene.width, scene.height]),
            bg,
        );
    }

    if let Some(title) = visual_title(scene) {
        backend.draw_scene_element(&title);
    }
    for elem in &scene.elements {
        backend.draw_scene_element(elem);
    }

    backend.shapes
}
