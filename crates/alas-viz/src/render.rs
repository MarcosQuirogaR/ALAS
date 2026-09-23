// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversion of [`Scene`] elements into `egui` painter shapes.

use alas_report::scene::{
    text_line_center_offsets, visual_title, Color, Fill, Point2D, Scene, SceneElement, Stroke,
    TextAlign, TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};
use egui::epaint::{CircleShape, Mesh, PathShape, RectShape, Rounding, TextShape};
use egui::{pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke as EguiStroke};

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

/// The largest miter factor the egui closed-path feathering may apply before
/// a polygon is tessellated without it. A corner whose adjacent edges turn by
/// `theta` gets its feathering vertices displaced by `1 / cos(theta / 2)`
/// times the feathering width; this bound keeps that under four pixels
/// (interior angles down to about 29 degrees).
const MAX_MITER_FACTOR: f32 = 4.0;

/// Screen-space polygon vertices with consecutive duplicates removed and the
/// closing vertex dropped, so every edge has a defined direction.
fn sanitized_polygon(points: impl IntoIterator<Item = Pos2>) -> Vec<Pos2> {
    const MIN_EDGE: f32 = 1e-3;
    let mut out: Vec<Pos2> = Vec::new();
    for p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            continue;
        }
        if out.last().is_some_and(|last| last.distance(p) < MIN_EDGE) {
            continue;
        }
        out.push(p);
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) < MIN_EDGE {
        out.pop();
    }
    out
}

/// Whether the egui miter feathering stays bounded on every corner.
///
/// egui places the feathering vertices of a closed path at
/// `normal / |normal|^2`, where `normal` is the mean of the two adjacent
/// edge normals. A lofted face seen edge-on projects to a sliver whose
/// consecutive edges nearly reverse, so `normal` tends to zero and the
/// vertices fly off the viewport (or become NaN for an exact reversal).
fn polygon_corners_are_well_conditioned(points: &[Pos2]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let min_length_sq = 1.0 / (MAX_MITER_FACTOR * MAX_MITER_FACTOR);
    (0..n).all(|i| {
        let prev = points[(i + n - 1) % n];
        let here = points[i];
        let next = points[(i + 1) % n];
        let n0 = (here - prev).normalized().rot90();
        let n1 = (next - here).normalized().rot90();
        let normal = (n0 + n1) * 0.5;
        normal.length_sq() >= min_length_sq
    })
}

/// Whether a polygon is convex in screen space.
///
/// `PathShape::convex_polygon` is deliberately used only for this subset of
/// faces. Native OpenVSP meshes can retain a concave boundary, and egui's
/// convex path fill would cover the indentation as if it were part of the
/// face. Collinear boundary points are allowed because tessellated CAD faces
/// commonly contain them.
fn polygon_is_convex(points: &[Pos2]) -> bool {
    const EPSILON: f32 = 1.0e-5;
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut turn_sign = 0.0_f32;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let c = points[(i + 2) % n];
        let cross = cross_2d(b - a, c - b);
        if cross.abs() <= EPSILON {
            continue;
        }
        if turn_sign == 0.0 {
            turn_sign = cross.signum();
        } else if cross.signum() != turn_sign {
            return false;
        }
    }
    turn_sign != 0.0
}

#[inline]
fn cross_2d(left: egui::Vec2, right: egui::Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

fn signed_polygon_area(points: &[Pos2]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(left, right)| left.x * right.y - right.x * left.y)
        .sum::<f32>()
        * 0.5
}

fn point_in_or_on_triangle(point: Pos2, a: Pos2, b: Pos2, c: Pos2, orientation: f32) -> bool {
    const EPSILON: f32 = 1.0e-5;
    let ab = cross_2d(b - a, point - a) * orientation;
    let bc = cross_2d(c - b, point - b) * orientation;
    let ca = cross_2d(a - c, point - c) * orientation;
    ab >= -EPSILON && bc >= -EPSILON && ca >= -EPSILON
}

/// Triangulate a simple screen-space polygon without changing its boundary.
///
/// This is used only by the egui backend, which accepts filled convex paths
/// but has no general concave path fill. The scene still carries the native
/// face as one polygon; the triangles are a renderer detail. Returning
/// `None` is safer than filling a self-intersecting or otherwise ambiguous
/// face with a fabricated fan.
fn triangulate_polygon(points: &[Pos2]) -> Option<Vec<[usize; 3]>> {
    const EPSILON: f32 = 1.0e-5;
    if points.len() < 3 {
        return None;
    }
    let area = signed_polygon_area(points);
    if !area.is_finite() || area.abs() <= EPSILON {
        return None;
    }
    let orientation = area.signum();
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    let mut guard = 0usize;
    let max_iterations = points.len().saturating_mul(points.len()).max(1);

    while remaining.len() > 3 {
        let mut ear_found = false;
        let count = remaining.len();
        for offset in 0..count {
            let prev = remaining[(offset + count - 1) % count];
            let current = remaining[offset];
            let next = remaining[(offset + 1) % count];
            let turn = cross_2d(
                points[next] - points[current],
                points[prev] - points[current],
            );
            if turn * orientation <= EPSILON {
                continue;
            }
            if remaining.iter().any(|&candidate| {
                candidate != prev
                    && candidate != current
                    && candidate != next
                    && point_in_or_on_triangle(
                        points[candidate],
                        points[prev],
                        points[current],
                        points[next],
                        orientation,
                    )
            }) {
                continue;
            }
            triangles.push([prev, current, next]);
            remaining.remove(offset);
            ear_found = true;
            break;
        }
        if !ear_found {
            return None;
        }
        guard += 1;
        if guard > max_iterations {
            return None;
        }
    }

    if remaining.len() == 3 {
        triangles.push([remaining[0], remaining[1], remaining[2]]);
    }
    Some(triangles)
}

/// A polygon the feathered egui path cannot tessellate safely, or a concave
/// polygon for which that path's convex-only fill would be wrong: triangulate
/// the fill and draw the original boundary as independent segments.
fn triangulated_polygon_shapes(
    points: &[Pos2],
    fill_color: Color32,
    outline: EguiStroke,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if fill_color != Color32::TRANSPARENT {
        let triangles = triangulate_polygon(points).unwrap_or_default();
        let mut mesh = Mesh::default();
        for &p in points {
            mesh.colored_vertex(p, fill_color);
        }
        for [a, b, c] in triangles {
            mesh.add_triangle(a as u32, b as u32, c as u32);
        }
        if !mesh.indices.is_empty() {
            shapes.push(Shape::mesh(mesh));
        }
    }
    if outline != EguiStroke::NONE {
        let n = points.len();
        for i in 0..n {
            shapes.push(Shape::line_segment(
                [points[i], points[(i + 1) % n]],
                outline,
            ));
        }
    }
    shapes
}

struct EguiBackend<'a> {
    transform: &'a ViewportTransform,
    context: egui::Context,
    shapes: Vec<Shape>,
}

impl<'a> EguiBackend<'a> {
    fn new(scene: &Scene, transform: &'a ViewportTransform, context: egui::Context) -> Self {
        Self {
            transform,
            context,
            shapes: Vec::with_capacity(scene.elements.len() + 1),
        }
    }

    /// Screen position of a scene point, snapped to the nearest whole pixel
    /// so axis lines and frames stay crisp.
    fn snapped(&self, point: Point2D) -> Pos2 {
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

    fn fill_rect(&mut self, p1: Pos2, p2: Pos2, color: Color) {
        self.shapes.push(Shape::Rect(RectShape::filled(
            Rect::from_two_pos(p1, p2),
            Rounding::ZERO,
            to_egui_color(&color),
        )));
    }

    fn draw_scene_element(&mut self, elem: &SceneElement) {
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
                let font_size = self.screen_font_size(*font_size);
                let text_style = TextStyle {
                    font_size,
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

/// A [`SceneElement::Text`] as laid out on screen: font size in screen
/// points and a premultiplied color.
struct TextStyle {
    font_size: f64,
    color: Color32,
    align: TextAlign,
    baseline: TextBaseline,
    angle_deg: f64,
    bold: bool,
}
