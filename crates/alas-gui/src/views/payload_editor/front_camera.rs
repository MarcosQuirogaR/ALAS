// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The front view's camera (pan, zoom, framing) and its metres-to-points
//! mapping.

use egui::{pos2, Pos2, Rect, Vec2};

use super::front_render::Placement;
use super::side_view::SideView;

const MARGIN: f32 = 30.0;
/// Room beyond the wing tips in the whole-aircraft framing, as a factor.
pub(super) const WING_TIP_ROOM: f64 = 1.10;
/// Free space around the fuselage in the default framing, as a factor.
const PADDING: f64 = 1.10;
/// Free space around the fuselage section in the fuselage framing, as a factor.
const FUSELAGE_PADDING: f64 = 1.08;
/// Share of the canvas height the fuselage section fills in the fuselage framing.
pub(super) const FUSELAGE_FILL: f64 = 0.85;
/// Closest zoom, relative to the fuselage framing.
pub(super) const MAX_ZOOM: f64 = 24.0;
/// Farthest zoom-out, relative to the whole-aircraft framing.
pub(super) const MIN_ZOOM: f64 = 0.4;

/// The view's pan and zoom. The default frames the whole front section (the
/// fuselage, the wings and the engines) at the station; `zoom` is relative to
/// that framing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Camera {
    pub zoom: f64,
    /// The `(y, z)` at the middle of the canvas, m; the middle of the whole
    /// aircraft's reach when unset.
    pub centre: Option<(f64, f64)>,
    /// Whether the user zoomed or panned since the last framing.
    pub touched: bool,
    /// Whether the last framing was the fuselage one (else the whole aircraft).
    pub fuselage: bool,
    /// What the camera was last framed for (see [`super::camera::fit_key`]).
    pub key: u64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            centre: None,
            touched: false,
            fuselage: false,
            key: 0,
        }
    }
}

/// Half width, lowest and highest point of the fuselage section at `x`, m.
fn fuselage_box(view: &SideView, x: f64) -> (f64, f64, f64) {
    let half = (view.at(&view.widths, x) / 2.0).max(0.5);
    let (low, high) = (view.at(&view.bottom, x), view.at(&view.top, x));
    (half, low, high.max(low + 0.5))
}

/// Half width and height range of the whole airframe reach at `x`, m.
fn aircraft_box(view: &SideView, x: f64) -> (f64, f64, f64) {
    let (half, low, high) = fuselage_box(view, x);
    let (z_low, z_high) = view.z_bounds(x).unwrap_or((low, high));
    let y_half = view.semi_span().max(half * 1.4).max(3.0) * WING_TIP_ROOM;
    // The fin and the tailplane show behind the section in the render.
    let top = view
        .tail_top_z
        .map_or(z_high.max(high), |tail| z_high.max(high).max(tail));
    (y_half, z_low.min(low), top)
}

#[cfg(test)]
/// Height of a canvas `width` points wide that frames the whole aircraft
/// (wings, fin, engines) at its true aspect, margins included.
pub(super) fn fit_height(view: &SideView, width: f32) -> f32 {
    let [z0, z1] = view.render_z_range();
    let y_half = view.semi_span().max(3.0) * WING_TIP_ROOM;
    let scale = f64::from((width - 2.0 * MARGIN).max(1.0)) / (2.0 * y_half);
    ((z1 - z0) * PADDING * scale) as f32 + 2.0 * MARGIN
}

impl Camera {
    /// Frame again when the canvas, the aircraft or the view changed (`key`)
    /// unless the user zoomed or panned since the last framing; the framing
    /// last asked for (whole aircraft or fuselage) is the one repeated.
    pub(super) fn sync(&mut self, rect: Rect, view: &SideView, x: f64, key: u64) {
        if self.key == key {
            return;
        }
        if !self.touched {
            *self = if self.fuselage {
                Self::fuselage(rect, view, x)
            } else {
                Self::default()
            };
        }
        self.key = key;
    }

    /// The camera closer in on the fuselage section at `x`.
    pub(super) fn fuselage(rect: Rect, view: &SideView, x: f64) -> Self {
        // The cabin cross-section fills the canvas: its width and its height, nothing else (the
        // fin and the wings behind it are the whole-aircraft framing's business).
        let (half, low, high) = fuselage_box(view, x);
        let across =
            f64::from((rect.width() - 2.0 * MARGIN).max(1.0)) / (2.0 * half * FUSELAGE_PADDING);
        let scale = across.min(FUSELAGE_FILL * f64::from(rect.height()) / (high - low).max(1.0e-6));
        Self {
            zoom: (scale / Frame::fit_scale(rect, view, x)).clamp(0.01, MAX_ZOOM),
            centre: Some((0.0, f64::midpoint(low, high))),
            fuselage: true,
            ..Self::default()
        }
    }

    /// Zoom by `factor` about the screen point `cursor`, which stays put.
    pub(super) fn zoom_about(
        &mut self,
        rect: Rect,
        view: &SideView,
        x: f64,
        cursor: Pos2,
        factor: f64,
    ) {
        let before = Frame::new(rect, view, x, *self);
        let (y, z) = before.to_world(cursor);
        let low = Self::min_zoom(rect, view, x);
        self.touched = true;
        self.zoom = (self.zoom * factor).clamp(low, MAX_ZOOM);
        let scale = Frame::new(rect, view, x, *self).scale;
        self.centre = Some((
            y - f64::from((cursor.x - rect.center().x) / scale),
            z + f64::from((cursor.y - rect.center().y) / scale),
        ));
        self.centre = Some(Frame::new(rect, view, x, *self).centre());
    }

    /// Move the view by a pointer drag of `delta` points.
    pub(super) fn pan_by(&mut self, rect: Rect, view: &SideView, x: f64, delta: Vec2) {
        self.touched = true;
        let frame = Frame::new(rect, view, x, *self);
        let (cy, cz) = frame.centre();
        self.centre = Some((
            cy - f64::from(delta.x / frame.scale),
            cz + f64::from(delta.y / frame.scale),
        ));
        self.centre = Some(Frame::new(rect, view, x, *self).centre());
    }

    /// The smallest zoom: a little beyond the whole aircraft.
    fn min_zoom(_rect: Rect, _view: &SideView, _x: f64) -> f64 {
        MIN_ZOOM
    }
}

/// Mapping between `(y, z)` in metres and screen points.
#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    rect: Rect,
    cy: f64,
    cz: f64,
    scale: f32,
}

impl Frame {
    /// Points per metre that fit a `half`-wide, `height`-tall box in `rect`.
    fn box_scale(rect: Rect, half: f64, height: f64) -> f64 {
        let width = f64::from((rect.width() - 2.0 * MARGIN).max(1.0));
        let tall = f64::from((rect.height() - 2.0 * MARGIN).max(1.0));
        (width / (2.0 * half)).min(tall / height.max(1.0e-6))
    }

    /// Points per metre of the default framing: the whole aircraft's reach
    /// at the station, with a margin.
    fn fit_scale(rect: Rect, view: &SideView, x: f64) -> f64 {
        let (y_half, low, high) = aircraft_box(view, x);
        Self::box_scale(rect, y_half, (high - low) * PADDING)
    }

    pub(super) fn new(rect: Rect, view: &SideView, x: f64, camera: Camera) -> Self {
        let (y_half, z_low, z_high) = aircraft_box(view, x);
        let scale = Self::fit_scale(rect, view, x) * camera.zoom;
        let (cy, cz) = camera.centre.unwrap_or((0.0, f64::midpoint(z_low, z_high)));
        Self {
            rect,
            cy: cy.clamp(-y_half, y_half),
            cz: cz.clamp(z_low - 1.0, z_high + 1.0),
            scale: scale as f32,
        }
    }

    pub(super) fn centre(self) -> (f64, f64) {
        (self.cy, self.cz)
    }

    /// The mapping the render behind the overlay is drawn through.
    pub(super) fn placement(self) -> Placement {
        Placement {
            origin: self.to_screen(0.0, 0.0),
            scale: self.scale,
        }
    }

    pub(super) fn to_screen(self, y: f64, z: f64) -> Pos2 {
        pos2(
            self.rect.center().x + (y - self.cy) as f32 * self.scale,
            self.rect.center().y - (z - self.cz) as f32 * self.scale,
        )
    }

    pub(super) fn to_world(self, point: Pos2) -> (f64, f64) {
        (
            self.cy + f64::from((point.x - self.rect.center().x) / self.scale),
            self.cz - f64::from((point.y - self.rect.center().y) / self.scale),
        )
    }
}
