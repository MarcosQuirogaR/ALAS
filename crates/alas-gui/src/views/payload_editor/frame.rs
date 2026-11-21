// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The side view's window on the body and its metres-to-points mapping.

use egui::{pos2, Pos2, Rect, Vec2};

use super::side_view::SideView;

/// The closest the side view zooms in.
pub(super) const MAX_ZOOM: f64 = 24.0;
pub(super) const MARGIN: f32 = 24.0;
/// Vertical room above and below the fitted body, points: the station scale
/// sits in the lower one.
pub(super) const VMARGIN: f32 = 18.0;

impl SideView {
    /// The x range the side view frames: the body, and with the airframe behind it every lifting
    /// surface (a fin or tailplane can reach past the end of the body).
    pub(super) fn frame_x_range(&self) -> [f64; 2] {
        let [mut low, mut high] = self.x_range();
        if self.airframe.is_some() {
            for point in self.surfaces.iter().flatten() {
                low = low.min(point[0]);
                high = high.max(point[0]);
            }
        }
        [low, high]
    }
}

/// The part of the body the canvas shows: `zoom` times closer than the whole
/// length, starting at the fraction `pan` of the free travel, and centred
/// vertically on `centre` (the middle of the body when unset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Window {
    pub zoom: f64,
    pub pan: f64,
    pub centre: Option<f64>,
    /// Whether the user zoomed or panned since the last fit.
    pub touched: bool,
    /// What the window was last fitted for (see [`super::camera::fit_key`]).
    pub key: u64,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: 0.0,
            centre: None,
            touched: false,
            key: 0,
        }
    }
}

impl Window {
    /// Re-fit when the canvas, the aircraft or the view changed (`key`)
    /// unless the user zoomed or panned since the last fit.
    pub(super) fn sync(&mut self, key: u64) {
        if self.key != key {
            if !self.touched {
                *self = Self::default();
            }
            self.key = key;
        }
    }

    /// The stations shown, m.
    pub(super) fn span(self, view: &SideView) -> [f64; 2] {
        let [x0, x1] = view.frame_x_range();
        let zoom = self.zoom.max(1.0);
        let length = (x1 - x0) / zoom;
        let start = x0 + self.pan.clamp(0.0, 1.0) * ((x1 - x0) - length);
        [start, start + length]
    }

    /// Zoom by `factor` keeping the point under `cursor` where it is.
    pub(super) fn zoom_about(&mut self, view: &SideView, rect: Rect, cursor: Pos2, factor: f64) {
        let before = Frame::windowed(rect, view, *self);
        let (x, z) = before.to_world(cursor);
        // A cursor over empty sky anchors on the body at that station instead, so zooming there
        // never carries the airframe out of the view.
        let (low, high) = (view.at(&view.bottom, x), view.at(&view.top, x));
        let z = if low.is_finite() && high.is_finite() && low <= high {
            z.clamp(low, high)
        } else {
            z
        };
        let [x0, x1] = view.frame_x_range();
        let zoom = (self.zoom.max(1.0) * factor).clamp(1.0, MAX_ZOOM);
        let length = (x1 - x0) / zoom;
        let (offset, strip) = strip(rect, view);
        let fraction =
            f64::from(((cursor.x - rect.left() - MARGIN - offset) / strip).clamp(0.0, 1.0));
        let start = (x - fraction * length).clamp(x0, x1 - length);
        self.touched = true;
        self.zoom = zoom;
        self.pan = if (x1 - x0 - length) > 1.0e-9 {
            (start - x0) / (x1 - x0 - length)
        } else {
            0.0
        };
        let after = Frame::windowed(rect, view, *self);
        // Keep the height under the cursor too.
        self.centre = Some(z + f64::from((cursor.y - after.rect.center().y) / after.sz));
        let settled = Frame::windowed(rect, view, *self);
        self.centre = Some(settled.zc);
    }

    /// Move the view by a pointer drag of `delta` points.
    pub(super) fn pan_by(&mut self, view: &SideView, rect: Rect, delta: Vec2) {
        self.touched = true;
        let frame = Frame::windowed(rect, view, *self);
        let [x0, x1] = view.frame_x_range();
        let [start, _] = self.span(view);
        let length = (x1 - x0) / self.zoom.max(1.0);
        let start = (start - f64::from(delta.x / frame.sx)).clamp(x0, x1 - length);
        self.pan = if (x1 - x0 - length) > 1.0e-9 {
            (start - x0) / (x1 - x0 - length)
        } else {
            0.0
        };
        self.centre = Some(frame.zc + f64::from(delta.y / frame.sz));
        let settled = Frame::windowed(rect, view, *self);
        self.centre = Some(settled.zc);
    }
}

/// Mapping between metres and screen points.
#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    rect: Rect,
    x0: f64,
    /// Free room left of the whole airframe when the canvas is height-limited, points.
    xoff: f32,
    /// The height at the vertical middle of the canvas, m.
    zc: f64,
    pub(super) sx: f32,
    sz: f32,
}

impl Frame {
    #[cfg(test)]
    pub(super) fn new(rect: Rect, view: &SideView) -> Self {
        Self::windowed(rect, view, Window::default())
    }

    pub(super) fn windowed(rect: Rect, view: &SideView, window: Window) -> Self {
        let [x0, x1] = view.frame_x_range();
        let [start, _] = window.span(view);
        let [z0, z1] = view.render_z_range();
        let width = (rect.width() - 2.0 * MARGIN).max(1.0);
        let height = (rect.height() - 2.0 * VMARGIN).max(1.0);
        let fit_x = width / (x1 - x0).max(1.0e-6) as f32;
        let fit_z = height / (z1 - z0).max(1.0e-6) as f32;
        let base = fit_x.min(fit_z);
        let (xoff, _) = strip(rect, view);
        let zoom = window.zoom.clamp(1.0, MAX_ZOOM);
        let sx = base * zoom as f32;
        let sz = sx; // one scale for both axes: the drawing is never distorted
        let middle = f64::midpoint(z0, z1);
        // The body may be dragged half a view past its edges, to reach items
        // outside it (belly, under the wing).
        let half_view = f64::from(height) / 2.0 / f64::from(sz);
        let slack = (f64::from(sz) * (z1 - z0) - f64::from(height)).max(0.0) / 2.0 / f64::from(sz)
            + 0.5 * half_view;
        let zc = window
            .centre
            .unwrap_or(middle)
            .clamp(middle - slack, middle + slack);
        // Zoomed in, the framed range (fin, belly, engines) can be far from
        // the body at the stations on view: keep the view centre within half a
        // view of the body there, so the airframe never leaves the view.
        let station = start + f64::from(width) / f64::from(sx) / 2.0;
        let (low, high) = (view.at(&view.bottom, station), view.at(&view.top, station));
        let zc = if low.is_finite() && high.is_finite() && low <= high {
            // At least a quarter of the view stays on the body at the stations on view.
            zc.clamp(low - 0.5 * half_view, high + 0.5 * half_view)
        } else {
            zc
        };
        Self {
            rect,
            x0: start,
            xoff,
            zc,
            sx,
            sz,
        }
    }

    /// Points per metre along the body.
    pub(super) fn pixels_per_metre(self) -> f32 {
        self.sx
    }

    pub(super) fn to_screen(self, x: f64, z: f64) -> Pos2 {
        pos2(
            self.rect.left() + MARGIN + self.xoff + (x - self.x0) as f32 * self.sx,
            self.rect.center().y - (z - self.zc) as f32 * self.sz,
        )
    }

    pub(super) fn to_world(self, point: Pos2) -> (f64, f64) {
        (
            self.x0 + f64::from((point.x - self.rect.left() - MARGIN - self.xoff) / self.sx),
            self.zc - f64::from((point.y - self.rect.center().y) / self.sz),
        )
    }
}

/// The horizontal strip the whole airframe takes at the fitted scale: its left offset from the
/// plot's inner margin and its width, points. The whole length fits the width; when the canvas is
/// height-limited the airframe is centred with room either side.
pub(super) fn strip(rect: Rect, view: &SideView) -> (f32, f32) {
    let [x0, x1] = view.frame_x_range();
    let [z0, z1] = view.render_z_range();
    let width = (rect.width() - 2.0 * MARGIN).max(1.0);
    let height = (rect.height() - 2.0 * VMARGIN).max(1.0);
    let base = (width / (x1 - x0).max(1.0e-6) as f32).min(height / (z1 - z0).max(1.0e-6) as f32);
    let used = ((x1 - x0) as f32 * base).clamp(1.0, width);
    ((width - used) / 2.0, used)
}
