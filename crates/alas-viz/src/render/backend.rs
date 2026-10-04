// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawing backend that emits egui shapes for scene elements.

use alas_report::scene::{
    text_line_center_offsets, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign,
    TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};
use egui::epaint::{CircleShape, PathShape, RectShape, Rounding, TextShape};
use egui::{pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke as EguiStroke};

use super::polygon::*;
use super::transform::*;

/// How one line of text is placed and painted.
struct TextStyle {
    /// Font size in screen points.
    font_size: f64,
    color: Color32,
    align: TextAlign,
    baseline: TextBaseline,
    angle_deg: f64,
    bold: bool,
}

pub(super) struct EguiBackend<'a> {
    transform: &'a ViewportTransform,
    context: egui::Context,
    pub(super) shapes: Vec<Shape>,
}

impl<'a> EguiBackend<'a> {
    pub(super) fn new(
        scene: &Scene,
        transform: &'a ViewportTransform,
        context: egui::Context,
    ) -> Self {
        Self {
            transform,
            context,
            shapes: Vec::with_capacity(scene.elements.len() + 1),
        }
    }

    /// Screen position of a scene point, snapped to the nearest whole pixel
    /// so axis lines and frames stay crisp.
    pub(super) fn snapped(&self, point: Point2D) -> Pos2 {
        let screen = self.transform.to_screen(point);
        pos2(screen.x.round(), screen.y.round())
    }

    fn screen_font_size(&self, authored_points: f64) -> f64 {
        let pixels_per_point = f64::from(self.context.pixels_per_point().max(1e-4));
        let physical_pixels = authored_points
            * CSS_PIXELS_PER_POINT
            * f64::from(self.transform.scale)
            * pixels_per_point;
        (physical_pixels / pixels_per_point).max(0.1)
    }

    pub(super) fn fill_rect(&mut self, p1: Pos2, p2: Pos2, color: Color) {
        self.shapes.push(Shape::Rect(RectShape::filled(
            Rect::from_two_pos(p1, p2),
            Rounding::ZERO,
            to_egui_color(&color),
        )));
    }

    /// Fill color and outline of a closed shape; transparent and no stroke
    /// when absent.
    fn paint(&self, fill: Option<&Fill>, stroke: Option<&Stroke>) -> (Color32, EguiStroke) {
        (
            fill.map_or(Color32::TRANSPARENT, |f| to_egui_color(&f.color)),
            stroke.map_or(EguiStroke::NONE, |s| {
                to_egui_stroke(s, self.transform.scale)
            }),
        )
    }

    pub(super) fn draw_scene_element(&mut self, elem: &SceneElement) {
        match elem {
            SceneElement::Line { p1, p2, stroke } => {
                self.shapes.push(Shape::line_segment(
                    [self.snapped(*p1), self.snapped(*p2)],
                    to_egui_stroke(stroke, self.transform.scale),
                ));
            }
            SceneElement::Polyline { points, stroke } => {
                if points.len() >= 2 {
                    let points = points.iter().map(|p| self.snapped(*p)).collect();
                    self.shapes.push(Shape::Path(PathShape::line(
                        points,
                        to_egui_stroke(stroke, self.transform.scale),
                    )));
                }
            }
            SceneElement::Polygon {
                points,
                fill,
                stroke,
            } => {
                // Sub-pixel positions: rounding to whole pixels collapses
                // thin faces into exactly reversed edges, whose corner
                // normals divide by zero inside the egui tessellator.
                let egui_points =
                    sanitized_polygon(points.iter().map(|p| self.transform.to_screen(*p)));
                if egui_points.len() < 3 {
                    return;
                }
                let (fill_color, outline) = self.paint(fill.as_ref(), stroke.as_ref());
                if polygon_corners_are_well_conditioned(&egui_points)
                    && polygon_is_convex(&egui_points)
                {
                    self.shapes.push(Shape::Path(PathShape::convex_polygon(
                        egui_points,
                        fill_color,
                        outline,
                    )));
                } else {
                    self.shapes.extend(triangulated_polygon_shapes(
                        &egui_points,
                        fill_color,
                        outline,
                    ));
                }
            }
            SceneElement::Rect {
                x,
                y,
                width,
                height,
                fill,
                stroke,
                ..
            } => {
                let rect = Rect::from_two_pos(
                    self.snapped([*x, *y]),
                    self.snapped([*x + *width, *y + *height]),
                );
                let (fill_color, outline) = self.paint(fill.as_ref(), stroke.as_ref());
                self.shapes.push(Shape::Rect(RectShape::new(
                    rect,
                    Rounding::ZERO,
                    fill_color,
                    outline,
                )));
            }
            SceneElement::Circle {
                center,
                radius,
                fill,
                stroke,
            } => {
                let (fill_color, outline) = self.paint(fill.as_ref(), stroke.as_ref());
                self.shapes.push(Shape::Circle(CircleShape {
                    center: self.snapped(*center),
                    radius: (*radius as f32 * self.transform.scale).max(0.0),
                    fill: fill_color,
                    stroke: outline,
                }));
            }
            SceneElement::Image {
                x,
                y,
                width,
                height,
                ..
            } => {
                // Geometry-only placeholder: loading an external raster into
                // the scene would reintroduce the expensive image handoff.
                self.fill_rect(
                    self.snapped([*x, *y]),
                    self.snapped([*x + *width, *y + *height]),
                    Color::rgb(225, 225, 225),
                );
            }
            SceneElement::SphericalImage { center, radius, .. } => {
                self.shapes.push(Shape::Circle(CircleShape {
                    center: self.snapped(*center),
                    radius: (*radius as f32 * self.transform.scale).max(0.0),
                    fill: Color32::from_rgb(8, 33, 61),
                    stroke: EguiStroke::NONE,
                }));
            }
            SceneElement::Text {
                text,
                pos,
                font_size,
                color,
                align,
                baseline,
                angle_deg,
                bold,
            } => {
                let text_style = TextStyle {
                    font_size: self.screen_font_size(*font_size),
                    color: to_egui_color(color),
                    align: *align,
                    baseline: *baseline,
                    angle_deg: *angle_deg,
                    bold: *bold,
                };
                self.draw_text(text, self.snapped(*pos), &text_style);
            }
            SceneElement::TextBlock {
                text,
                pos,
                width,
                font_size,
                color,
                bold,
            } => self.draw_text_block(text, *pos, *width, *font_size, *color, *bold),
        }
    }

    /// Lay out a paragraph with the context's real glyph metrics, wrapped to
    /// the block width in screen pixels, so it never extends past its box
    /// however the scene is scaled into the card.
    fn draw_text_block(
        &mut self,
        text: &str,
        pos: Point2D,
        width: f64,
        font_size: f64,
        color: Color,
        bold: bool,
    ) {
        let color = to_egui_color(&color);
        let font_id = FontId::new(
            self.screen_font_size(font_size) as f32,
            FontFamily::Proportional,
        );
        let wrap_width = (width.max(0.0) as f32 * self.transform.scale).max(1.0);
        let galley = self
            .context
            .fonts(|fonts| fonts.layout(text.to_owned(), font_id.clone(), color, wrap_width));
        let text_shape = TextShape::new(self.transform.to_screen(pos), galley, color);
        self.shapes.push(Shape::Text(text_shape.clone()));
        if bold {
            let mut weight = text_shape;
            weight.pos += vec2(0.35, 0.0);
            self.shapes.push(Shape::Text(weight));
        }
    }

    fn draw_text(&mut self, text: &str, anchor: Pos2, style: &TextStyle) {
        let lines = text.split('\n').collect::<Vec<_>>();
        let line_height = style.font_size * TEXT_LINE_HEIGHT_EM;
        let centers = text_line_center_offsets(lines.len(), line_height, style.baseline);
        let angle = style.angle_deg.to_radians() as f32;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let font_id = FontId::new(style.font_size as f32, FontFamily::Proportional);
        for (line, center) in lines.iter().zip(centers) {
            let galley = self.context.fonts(|fonts| {
                fonts.layout_no_wrap((*line).to_owned(), font_id.clone(), style.color)
            });
            let horizontal = match style.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => -galley.rect.width() * 0.5,
                TextAlign::Right => -galley.rect.width(),
            };
            let offset = vec2(horizontal, center as f32 - galley.rect.height() * 0.5);
            let rotated_offset = vec2(
                offset.x * cos_angle - offset.y * sin_angle,
                offset.x * sin_angle + offset.y * cos_angle,
            );
            let text_shape =
                TextShape::new(anchor + rotated_offset, galley, style.color).with_angle(angle);
            self.shapes.push(Shape::Text(text_shape.clone()));
            if style.bold {
                // Egui's default proportional font has no weight field. A tiny
                // offset duplicate is a stable faux-bold that keeps the same
                // font atlas and remains visible at chart-label sizes.
                let mut weight = text_shape;
                weight.pos += vec2(0.35 * cos_angle, 0.35 * sin_angle);
                self.shapes.push(Shape::Text(weight));
            }
        }
    }
}
