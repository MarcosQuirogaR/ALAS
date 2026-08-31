// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Interactive scene viewport widget with pan, zoom, fit-to-view, and coordinate readouts.

use alas_report::scene::{Point2D, Scene};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{
    pos2, vec2, Color32, Id, Rect, Response, Sense, Stroke, TextureHandle, Ui, Vec2, Widget,
};

use crate::raster::render_scene_rgba_scaled;
use crate::render::{to_egui_color, ViewportTransform};

/// Interactive scene viewport state for persistent pan and zoom.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneViewState {
    /// Pan offset in screen pixels relative to center.
    pub pan: Vec2,
    /// User zoom multiplier (1.0 = fit size).
    pub zoom: f32,
    /// Whether automatic fit-to-view is active.
    pub auto_fit: bool,
    /// Last cursor position in scene data coordinates.
    pub cursor_canvas: Option<Point2D>,
}

impl Default for SceneViewState {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            auto_fit: true,
            cursor_canvas: None,
        }
    }
}

impl SceneViewState {
    /// Reset viewport state back to centered fit-to-view.
    pub fn reset(&mut self) {
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.auto_fit = true;
        self.cursor_canvas = None;
    }

    /// Zoom around `pointer` instead of the canvas origin.
    ///
    /// `fit_offset` is the upper-left position of an unpanned fitted scene.
    /// Keeping the canvas coordinate below the pointer fixed avoids the
    /// apparent jump toward the upper-left corner on every wheel tick.
    pub fn zoom_about(&mut self, fit_offset: Vec2, pointer: Vec2, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let old_zoom = if self.auto_fit || !self.zoom.is_finite() {
            1.0
        } else {
            self.zoom.clamp(0.05, 50.0)
        };
        let old_pan = if self.auto_fit { Vec2::ZERO } else { self.pan };
        let new_zoom = (old_zoom * factor).clamp(0.05, 50.0);
        let applied_factor = new_zoom / old_zoom;
        let old_offset = fit_offset + old_pan;
        self.pan = pointer - (pointer - old_offset) * applied_factor - fit_offset;
        self.zoom = new_zoom;
        self.auto_fit = false;
    }

    /// Keep a panned canvas intersecting the viewport so it can always be
    /// recovered without relying on a hidden or unreachable reset gesture.
    pub fn clamp_pan(&mut self, viewport_size: Vec2, rendered_size: Vec2) {
        let max_x = ((viewport_size.x + rendered_size.x) * 0.5).max(0.0);
        let max_y = ((viewport_size.y + rendered_size.y) * 0.5).max(0.0);
        self.pan.x = if self.pan.x.is_finite() {
            self.pan.x.clamp(-max_x, max_x)
        } else {
            0.0
        };
        self.pan.y = if self.pan.y.is_finite() {
            self.pan.y.clamp(-max_y, max_y)
        } else {
            0.0
        };
    }
}

/// Interactive widget displaying an ALAS [`Scene`] inside an `egui::Ui`.
pub struct SceneView<'a> {
    scene: &'a Scene,
    state: &'a mut SceneViewState,
    desired_size: Option<Vec2>,
    show_toolbar: bool,
    allow_pan: bool,
    allow_wheel_zoom: bool,
    raster_scale: f64,
    cache_id: Option<Id>,
    cache_revision: Option<u64>,
}

impl<'a> SceneView<'a> {
    /// Create a new interactive view for `scene` with mutable state reference.
    pub fn new(scene: &'a Scene, state: &'a mut SceneViewState) -> Self {
        Self {
            scene,
            state,
            desired_size: None,
            show_toolbar: true,
            allow_pan: true,
            allow_wheel_zoom: false,
            raster_scale: 2.0,
            cache_id: None,
            cache_revision: None,
        }
    }

    /// Set a desired size for the viewport rectangle.
    pub fn desired_size(mut self, size: Vec2) -> Self {
        self.desired_size = Some(size);
        self
    }

    /// Enable or disable the overlay toolbar.
    pub fn show_toolbar(mut self, show: bool) -> Self {
        self.show_toolbar = show;
        self
    }

    /// Make this view a camera/orbit surface instead of a 2-D canvas.
    ///
    /// A 3-D preview owns rotation in the caller because the scene is rebuilt
    /// from the camera angles.  Letting this widget also apply the same drag
    /// to its 2-D pan would move the projected aircraft out of the frame and
    /// couple two unrelated camera models.
    pub fn orbit_only(mut self) -> Self {
        self.allow_pan = false;
        self
    }

    /// Keep a gallery card fixed in place while retaining its toolbar.
    /// Maximized figures keep the default drag-to-pan interaction.
    pub fn static_view(mut self) -> Self {
        self.allow_pan = false;
        self
    }

    /// Enable mouse-wheel zooming for a maximized figure.
    pub fn wheel_zoom(mut self, enabled: bool) -> Self {
        self.allow_wheel_zoom = enabled;
        self
    }

    /// Set the raster density used for this view's cached scene texture.
    ///
    /// A route globe changes on each orbit frame, so it favors native scene
    /// density over the supersampling used by static report cards.
    pub fn raster_scale(mut self, scale: f64) -> Self {
        self.raster_scale = scale.clamp(1.0, 2.0);
        self
    }

    /// Reuse one GPU texture while this view's scene changes.
    ///
    /// Static figures naturally key their texture by scene content. An
    /// orbiting globe instead supplies a stable view key so camera motion
    /// replaces pixels in its existing texture rather than retaining one GPU
    /// allocation per drag frame.
    pub fn cache_key(mut self, key: impl Hash) -> Self {
        self.cache_id = Some(Id::new(("alas_scene_png", key)));
        self
    }

    /// Use a caller-owned revision instead of formatting and hashing the scene graph.
    ///
    /// Dense orbiting scenes are rebuilt whenever their camera changes. Their
    /// owner already knows that revision, so walking thousands of polygons to
    /// rediscover the same fact would consume a significant part of a frame.
    pub fn cache_revision(mut self, revision: u64) -> Self {
        self.cache_revision = Some(revision);
        self
    }

    /// Draw the viewport in the provided `ui` and return the interaction response.
    pub fn show(self, ui: &mut Ui) -> Response {
        let size = self
            .desired_size
            .unwrap_or_else(|| ui.available_size().max(vec2(100.0, 100.0)));
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let fit_transform = ViewportTransform::fit(self.scene.width, self.scene.height, rect);

        // Handle panning via mouse drag
        if self.allow_pan
            && (response.dragged_by(egui::PointerButton::Primary)
                || response.dragged_by(egui::PointerButton::Middle))
        {
            // `drag_motion` is the per-frame motion.  Applying a drag total
            // on every frame makes the same gesture grow quadratically.
            self.state.pan += response.drag_motion();
            self.state.auto_fit = false;
        }

        if self.allow_wheel_zoom && response.hovered() {
            let scroll_y = ui.ctx().input(|input| input.smooth_scroll_delta.y);
            if scroll_y.abs() > f32::EPSILON {
                let factor = (1.0 + scroll_y * 0.0015).clamp(0.5, 1.5);
                let pointer = response.hover_pos().unwrap_or(rect.center());
                self.state.zoom_about(
                    vec2(fit_transform.offset_x, fit_transform.offset_y),
                    pointer.to_vec2(),
                    factor,
                );
            }
        }

        // Compute effective transform
        let mut base_transform = fit_transform;
        if !self.state.auto_fit {
            base_transform.scale *= self.state.zoom;
            base_transform.offset_x += self.state.pan.x;
            base_transform.offset_y += self.state.pan.y;
        } else {
            self.state.pan = Vec2::ZERO;
            self.state.zoom = 1.0;
        }

        if !self.state.auto_fit {
            // Keep some part of the scene reachable after a 2-D drag.  The
            // Fit button remains an explicit hard reset, while this bound
            // prevents a canvas from being stranded outside its viewport.
            let rendered_w = self.scene.width.max(1.0) as f32 * base_transform.scale;
            let rendered_h = self.scene.height.max(1.0) as f32 * base_transform.scale;
            self.state
                .clamp_pan(rect.size(), vec2(rendered_w, rendered_h));
        }

        // Track cursor coordinates
        if let Some(hover) = response.hover_pos() {
            self.state.cursor_canvas = Some(base_transform.to_canvas(hover));
        } else {
            self.state.cursor_canvas = None;
        }

        // Render one cached PNG texture. The scene hash changes when a run,
        // theme, or configuration changes; ordinary repaint frames reuse the
        // uploaded texture and only rescale it for the current viewport.
        let painter = ui.painter_at(rect);
        // Clip to viewport rectangle
        let clip_rect = rect.intersect(ui.clip_rect());
        let clipped_painter = painter.with_clip_rect(clip_rect);

        if let Some(background) = self.scene.background {
            clipped_painter.rect_filled(rect, 0.0, to_egui_color(&background));
        }

        if let Some(texture) = cached_scene_texture(
            self.scene,
            ui,
            self.raster_scale,
            self.cache_id,
            self.cache_revision,
        ) {
            let image_rect = Rect::from_min_size(
                pos2(base_transform.offset_x, base_transform.offset_y),
                vec2(
                    self.scene.width as f32 * base_transform.scale,
                    self.scene.height as f32 * base_transform.scale,
                ),
            );
            clipped_painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Render border
        ui.painter()
            .rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(60)));

        // Render minimal overlay controls if enabled
        if self.show_toolbar {
            let pad = 8.0;
            let btn_size = vec2(26.0, 24.0);
            let bar_rect = Rect::from_min_size(
                pos2(rect.right() - pad - 90.0, rect.top() + pad),
                vec2(90.0, 28.0),
            );

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                    if ui
                        .add_sized(btn_size, egui::Button::new("+").small())
                        .on_hover_text("Zoom in")
                        .clicked()
                    {
                        self.state.zoom = (self.state.zoom * 1.2).clamp(0.05, 50.0);
                        self.state.auto_fit = false;
                    }

                    if ui
                        .add_sized(btn_size, egui::Button::new("-").small())
                        .on_hover_text("Zoom out")
                        .clicked()
                    {
                        self.state.zoom = (self.state.zoom / 1.2).clamp(0.05, 50.0);
                        self.state.auto_fit = false;
                    }

                    if ui
                        .add_sized(btn_size, egui::Button::new("Fit").small())
                        .on_hover_text("Reset to fit view")
                        .clicked()
                    {
                        self.state.reset();
                    }
                });
            });
        }

        response
    }
}

#[derive(Clone)]
struct CachedSceneTexture {
    scene_hash: u64,
    texture: TextureHandle,
}

fn cached_scene_texture(
    scene: &Scene,
    ui: &Ui,
    raster_scale: f64,
    cache_id: Option<Id>,
    cache_revision: Option<u64>,
) -> Option<TextureHandle> {
    let scene_hash = cache_revision.unwrap_or_else(|| {
        let mut hasher = DefaultHasher::new();
        format!("{scene:?}").hash(&mut hasher);
        hasher.finish()
    });
    let id = cache_id.unwrap_or_else(|| Id::new(("alas_scene_png", scene_hash)));
    if let Some(mut cached) = ui
        .ctx()
        .data(|data| data.get_temp::<CachedSceneTexture>(id))
    {
        if cached.scene_hash == scene_hash {
            return Some(cached.texture);
        }
        let image = scene_color_image(scene, raster_scale)?;
        cached.texture.set(image, egui::TextureOptions::LINEAR);
        cached.scene_hash = scene_hash;
        let texture = cached.texture.clone();
        ui.ctx().data_mut(|data| data.insert_temp(id, cached));
        return Some(texture);
    }
    // Result cards are commonly displayed larger than the authored scene;
    // supersampling here keeps labels and one-pixel plot strokes legible.
    let image = scene_color_image(scene, raster_scale)?;
    let texture = ui.ctx().load_texture(
        format!("alas-scene-png-{scene_hash}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            id,
            CachedSceneTexture {
                scene_hash,
                texture: texture.clone(),
            },
        )
    });
    Some(texture)
}

fn scene_color_image(scene: &Scene, raster_scale: f64) -> Option<egui::ColorImage> {
    let (width, height, rgba) = render_scene_rgba_scaled(scene, raster_scale).ok()?;
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

impl<'a> Widget for SceneView<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
