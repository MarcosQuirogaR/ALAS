// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mapping from scene coordinates to the egui viewport, and colour conversions.

use alas_report::scene::{Color, Point2D, Stroke};
use egui::{pos2, Color32, Pos2, Rect, Stroke as EguiStroke};

/// Viewport coordinate transformation mapping scene canvas coordinates to on-screen egui points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportTransform {
    /// Screen destination bounding rectangle.
    pub target_rect: Rect,
    /// Uniform scale factor from scene pixels to screen pixels.
    pub scale: f32,
    /// Horizontal offset in screen pixels.
    pub offset_x: f32,
    /// Vertical offset in screen pixels.
    pub offset_y: f32,
}

impl ViewportTransform {
    /// Compute a fit-to-view transform that centers the scene within `target_rect` preserving aspect ratio.
    pub fn fit(scene_width: f64, scene_height: f64, target_rect: Rect) -> Self {
        let sw = scene_width.max(1.0) as f32;
        let sh = scene_height.max(1.0) as f32;
        let sx = target_rect.width() / sw;
        let sy = target_rect.height() / sh;
        let scale = sx.min(sy).max(1e-4);

        let rendered_w = sw * scale;
        let rendered_h = sh * scale;
        let offset_x = target_rect.left() + (target_rect.width() - rendered_w) * 0.5;
        let offset_y = target_rect.top() + (target_rect.height() - rendered_h) * 0.5;

        Self {
            target_rect,
            scale,
            offset_x,
            offset_y,
        }
    }

    /// Map a 2D canvas point to an on-screen position.
    pub fn to_screen(&self, pt: Point2D) -> Pos2 {
        pos2(
            self.offset_x + (pt[0] as f32) * self.scale,
            self.offset_y + (pt[1] as f32) * self.scale,
        )
    }

    /// Map an on-screen position back to canvas coordinates.
    pub fn to_canvas(&self, screen: Pos2) -> Point2D {
        if self.scale.abs() < 1e-6 {
            return [0.0, 0.0];
        }
        let x = (screen.x - self.offset_x) / self.scale;
        let y = (screen.y - self.offset_y) / self.scale;
        [x as f64, y as f64]
    }
}

/// Convert an ALAS RGBA color to an `egui::Color32`.
///
/// [`Color`] carries straight (unassociated) alpha, as SVG `fill-opacity`
/// does, while `Color32` stores premultiplied channels. The channels are
/// multiplied in sRGB space, which is how the SVG export composites under
/// resvg and in browsers, so a translucent overlay matches the exported
/// figure.
pub fn to_egui_color(c: &Color) -> Color32 {
    let alpha = c.alpha_f64();
    let premultiply = |channel: u8| (f64::from(channel) * alpha).round() as u8;
    Color32::from_rgba_premultiplied(premultiply(c.r), premultiply(c.g), premultiply(c.b), c.a)
}

/// Convert an ALAS stroke to an `egui::Stroke`.
pub fn to_egui_stroke(s: &Stroke, scale: f32) -> EguiStroke {
    let width = ((s.width as f32) * scale).max(0.5);
    EguiStroke::new(width, to_egui_color(&s.color))
}
