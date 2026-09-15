// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_report::scene::{
    text_line_center_offsets, visual_title, Color, Point2D, Scene, SceneElement, Stroke, TextAlign,
    TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};
use egui::epaint::{CircleShape, Mesh, PathShape, RectShape, Rounding, TextShape};
use egui::{pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke as EguiStroke};
use plotters::style::{Color as PlottersColor, RGBColor, ShapeStyle, TextStyle};
use plotters_backend::{
    text_anchor::{HPos, VPos},
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};

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
pub fn to_egui_color(c: &Color) -> Color32 {
    Color32::from_rgba_premultiplied(c.r, c.g, c.b, c.a)
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
        backend.fill_rect((0, 0), backend.to_coord([scene.width, scene.height]), bg);
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

/// A polygon the feathered egui path cannot tessellate safely: the fill as a
/// fan mesh without feathering and the outline as independent segments,
/// neither of which uses a corner miter.
fn sliver_polygon_shapes(points: &[Pos2], fill_color: Color32, outline: EguiStroke) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if fill_color != Color32::TRANSPARENT {
        let mut mesh = Mesh::default();
        for &p in points {
            mesh.colored_vertex(p, fill_color);
        }
        for i in 2..points.len() as u32 {
            mesh.add_triangle(0, i - 1, i);
        }
        shapes.push(Shape::mesh(mesh));
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
    scene: &'a Scene,
    transform: &'a ViewportTransform,
    context: egui::Context,
    shapes: Vec<Shape>,
}

impl<'a> EguiBackend<'a> {
    fn new(scene: &'a Scene, transform: &'a ViewportTransform, context: egui::Context) -> Self {
        Self {
            scene,
            transform,
            context,
            shapes: Vec::with_capacity(scene.elements.len() + 1),
        }
    }

    fn to_coord(&self, point: Point2D) -> BackendCoord {
        let screen = self.transform.to_screen(point);
        (screen.x.round() as i32, screen.y.round() as i32)
    }

    fn color(color: BackendColor) -> Color32 {
        let alpha = (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgba_premultiplied(
            ((color.rgb.0 as f64) * color.alpha).round() as u8,
            ((color.rgb.1 as f64) * color.alpha).round() as u8,
            ((color.rgb.2 as f64) * color.alpha).round() as u8,
            alpha,
        )
    }

    fn style(stroke: &Stroke, scale: f32) -> ShapeStyle {
        ShapeStyle::from(
            RGBColor(stroke.color.r, stroke.color.g, stroke.color.b).mix(stroke.color.alpha_f64()),
        )
        .stroke_width((stroke.width as f32 * scale).max(0.5) as u32)
    }

    fn screen_font_size(&self, authored_points: f64) -> f64 {
        let pixels_per_point = f64::from(self.context.pixels_per_point().max(1e-4));
        let physical_pixels = authored_points
            * CSS_PIXELS_PER_POINT
            * f64::from(self.transform.scale)
            * pixels_per_point;
        (physical_pixels / pixels_per_point).max(0.1)
    }

    fn fill_rect(&mut self, p1: BackendCoord, p2: BackendCoord, color: Color) {
        let rect = Rect::from_two_pos(
            pos2(p1.0 as f32, p1.1 as f32),
            pos2(p2.0 as f32, p2.1 as f32),
        );
        self.shapes.push(Shape::Rect(RectShape::filled(
            rect,
            Rounding::ZERO,
            to_egui_color(&color),
        )));
    }

    fn draw_scene_element(&mut self, elem: &SceneElement) {
        match elem {
            SceneElement::Line { p1, p2, stroke } => {
                let style = Self::style(stroke, self.transform.scale);
                let _ = self.draw_line(self.to_coord(*p1), self.to_coord(*p2), &style);
            }
            SceneElement::Polyline { points, stroke } => {
                let style = Self::style(stroke, self.transform.scale);
                let coords = points.iter().map(|p| self.to_coord(*p)).collect::<Vec<_>>();
                let _ = self.draw_path(coords, &style);
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
                let fill_color = fill
                    .map(|f| to_egui_color(&f.color))
                    .unwrap_or(Color32::TRANSPARENT);
                let outline = stroke
                    .as_ref()
                    .map(|s| to_egui_stroke(s, self.transform.scale))
                    .unwrap_or(EguiStroke::NONE);
                if polygon_corners_are_well_conditioned(&egui_points) {
                    self.shapes.push(Shape::Path(PathShape::convex_polygon(
                        egui_points,
                        fill_color,
                        outline,
                    )));
                } else {
                    self.shapes
                        .extend(sliver_polygon_shapes(&egui_points, fill_color, outline));
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
                let p1 = self.to_coord([*x, *y]);
                let p2 = self.to_coord([*x + *width, *y + *height]);
                let rect = Rect::from_two_pos(
                    pos2(p1.0 as f32, p1.1 as f32),
                    pos2(p2.0 as f32, p2.1 as f32),
                );
                let fill_color = fill
                    .map(|f| to_egui_color(&f.color))
                    .unwrap_or(Color32::TRANSPARENT);
                let outline = stroke
                    .as_ref()
                    .map(|s| to_egui_stroke(s, self.transform.scale))
                    .unwrap_or(EguiStroke::NONE);
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
                let (x, y) = self.to_coord(*center);
                let fill_color = fill
                    .map(|f| to_egui_color(&f.color))
                    .unwrap_or(Color32::TRANSPARENT);
                let outline = stroke
                    .as_ref()
                    .map(|s| to_egui_stroke(s, self.transform.scale))
                    .unwrap_or(EguiStroke::NONE);
                self.shapes.push(Shape::Circle(CircleShape {
                    center: pos2(x as f32, y as f32),
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
                // Plotters is intentionally geometry-only here: loading an external
                // raster into the scene would reintroduce the expensive image handoff.
                self.fill_rect(
                    self.to_coord([*x, *y]),
                    self.to_coord([*x + *width, *y + *height]),
                    Color::rgb(225, 225, 225),
                );
            }
            SceneElement::SphericalImage { center, radius, .. } => {
                let (x, y) = self.to_coord(*center);
                self.shapes.push(Shape::Circle(CircleShape {
                    center: pos2(x as f32, y as f32),
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
                let anchor =
                    plotters_backend::text_anchor::Pos::new(to_hpos(*align), to_vpos(*baseline));
                let text_color = RGBColor(color.r, color.g, color.b).mix(color.alpha_f64());
                let style = TextStyle::from(("sans-serif", self.screen_font_size(*font_size)))
                    .color(&text_color)
                    .pos(anchor);
                let _ = self.draw_text_with_options(
                    text,
                    &style,
                    self.to_coord(*pos),
                    *angle_deg,
                    *bold,
                );
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

    fn draw_text_with_options<TStyle: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &TStyle,
        pos: BackendCoord,
        angle_deg: f64,
        bold: bool,
    ) -> Result<(), DrawingErrorKind<std::io::Error>> {
        let color = Self::color(style.color());
        let lines = text.split('\n').collect::<Vec<_>>();
        let line_height = style.size() * TEXT_LINE_HEIGHT_EM;
        let baseline = match style.anchor().v_pos {
            VPos::Top => TextBaseline::Top,
            VPos::Center => TextBaseline::Middle,
            VPos::Bottom => TextBaseline::Bottom,
        };
        let centers = text_line_center_offsets(lines.len(), line_height, baseline);
        let angle = angle_deg.to_radians() as f32;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let anchor = pos2(pos.0 as f32, pos.1 as f32);
        let font_id = FontId::new(style.size() as f32, FontFamily::Proportional);
        for (line, center) in lines.iter().zip(centers) {
            let galley = self
                .context
                .fonts(|fonts| fonts.layout_no_wrap((*line).to_owned(), font_id.clone(), color));
            let horizontal = match style.anchor().h_pos {
                HPos::Left => 0.0,
                HPos::Center => -galley.rect.width() * 0.5,
                HPos::Right => -galley.rect.width(),
            };
            let offset = vec2(horizontal, center as f32 - galley.rect.height() * 0.5);
            let rotated_offset = vec2(
                offset.x * cos_angle - offset.y * sin_angle,
                offset.x * sin_angle + offset.y * cos_angle,
            );
            let text_shape =
                TextShape::new(anchor + rotated_offset, galley, color).with_angle(angle);
            self.shapes.push(Shape::Text(text_shape.clone()));
            if bold {
                // Egui's default proportional font has no weight field. A tiny
                // offset duplicate is a stable faux-bold that keeps the same
                // font atlas and remains visible at chart-label sizes.
                let mut weight = text_shape;
                weight.pos += vec2(0.35 * cos_angle, 0.35 * sin_angle);
                self.shapes.push(Shape::Text(weight));
            }
        }
        Ok(())
    }
}
