// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A fixed front view (looking aft) of the committed aircraft, drawn by the
//! same lofted-model renderer as the live preview and the Sandbox, with a
//! ground line and labelled markers on the wing.
//!
//! Frame: model `y` metres outboard of the centreline, `z` metres up. The
//! view is orthographic, so a marker needs only its `(y, z)`.

use alas_report::scene::{
    Camera3D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};

use crate::state::AppState;

use super::LivePreviewFit;
use super::{fit_reference, interactive_geometry, live_preview_cache, live_preview_options};

/// Station label font, scene units, and the height of its box.
const LABEL_FONT: f64 = 12.0;
const LABEL_HEIGHT: f64 = 16.0;

/// Ground-line caption font, pt.
const CAPTION_FONT: f64 = 11.0;

/// Band kept free at the bottom of the canvas, under the drawing, for the
/// ground line's caption: its gap to the line and its line height.
const CAPTION_BAND: f64 = 4.0 + CAPTION_FONT * alas_report::scene::CSS_PIXELS_PER_POINT * 1.2 + 6.0;

/// A label's box in scene units, y down.
#[derive(Debug, Clone, Copy)]
struct LabelRect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl LabelRect {
    fn contains(&self, point: [f64; 2], pad: f64) -> bool {
        point[0] >= self.left - pad
            && point[0] <= self.right + pad
            && point[1] >= self.top - pad
            && point[1] <= self.bottom + pad
    }

    /// Whether the polygon has a vertex in the box, or covers one of a grid of
    /// sample points of it.
    fn touches_polygon(&self, polygon: &[[f64; 2]], pad: f64) -> bool {
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for point in polygon {
            x0 = x0.min(point[0]);
            x1 = x1.max(point[0]);
            y0 = y0.min(point[1]);
            y1 = y1.max(point[1]);
        }
        if x1 < self.left - pad
            || x0 > self.right + pad
            || y1 < self.top - pad
            || y0 > self.bottom + pad
        {
            return false;
        }
        if polygon.iter().any(|point| self.contains(*point, pad)) {
            return true;
        }
        for column in 0..=6 {
            for row in 0..=2 {
                let point = [
                    self.left - pad
                        + (self.right - self.left + 2.0 * pad) * f64::from(column) / 6.0,
                    self.top - pad + (self.bottom - self.top + 2.0 * pad) * f64::from(row) / 2.0,
                ];
                if point_in_polygon(point, polygon) {
                    return true;
                }
            }
        }
        false
    }

    fn overlaps(&self, other: &Self, pad: f64) -> bool {
        self.left - pad < other.right
            && self.right + pad > other.left
            && self.top - pad < other.bottom
            && self.bottom + pad > other.top
    }
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len().wrapping_sub(1);
    for (index, a) in polygon.iter().enumerate() {
        let b = polygon[previous];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        previous = index;
    }
    inside
}

/// Screen y of the wing line (the markers joined by straight segments) at `x`;
/// beyond the end markers the nearest one holds.
fn wing_line_y(points: &[[f64; 2]], x: f64) -> f64 {
    let mut sorted: Vec<[f64; 2]> = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let Some(first) = sorted.first() else {
        return 0.0;
    };
    if x <= first[0] {
        return first[1];
    }
    for pair in sorted.windows(2) {
        if x <= pair[1][0] {
            let span = (pair[1][0] - pair[0][0]).max(1.0e-9);
            return pair[0][1] + (pair[1][1] - pair[0][1]) * (x - pair[0][0]) / span;
        }
    }
    sorted.last().map_or(0.0, |last| last[1])
}

/// One station marker on the right wing.
#[derive(Debug, Clone, PartialEq)]
pub struct WingMarker {
    /// Metres outboard of the centreline.
    pub y_m: f64,
    /// Metres above the geometry origin, in the built (flight) shape.
    pub z_m: f64,
    /// Short label drawn above the marker; empty for a bare dot.
    pub label: String,
}

/// Draw the front view on a `canvas` (scene units), or `None` when the
/// configuration does not build. The aircraft is fitted above a band at the
/// bottom of the canvas that holds the ground line's caption, so the caption
/// is never cut by the canvas edge.
pub fn build_front_view(
    state: &AppState,
    canvas: (f64, f64),
    markers: &[WingMarker],
) -> Option<Scene> {
    build_front_view_from(
        &state.typed_config()?,
        state.current_design()?,
        state.theme.figure_theme_name(),
        canvas,
        markers,
    )
}

/// [`build_front_view`] from owned inputs, for a worker thread.
pub fn build_front_view_from(
    config: &alas_config::AlasConfig,
    design: alas_config::DesignVector,
    theme: &str,
    canvas: (f64, f64),
    markers: &[WingMarker],
) -> Option<Scene> {
    let model = live_preview_cache::model(interactive_geometry(config), design)?;
    let drawing = (canvas.0, (canvas.1 - CAPTION_BAND).max(80.0));
    let mut options = live_preview_options();
    options.canvas = drawing;
    options.reference = Some(fit_reference(
        &model,
        LivePreviewFit {
            canvas: drawing,
            elev_deg: 0.0,
            azim_deg: 90.0,
            reserved: (0.0, 0.0),
        },
    ));
    let (mut scene, framing) = model.render(Some(Camera3D::front()), Some(theme), &options);
    scene.height = scene.height.max(canvas.1);
    let palette = alas_report::theme::get_palette(Some(theme));
    let ink = Color::from_hex(palette.title);
    let muted = Color::from_hex(palette.tick);

    let mut z_min = f64::INFINITY;
    let mut y_max = 0.0_f64;
    for point in model.faces().iter().flat_map(|face| face.points.iter()) {
        z_min = z_min.min(point[2]);
        y_max = y_max.max(point[1].abs());
    }
    let mut ground = Vec::new();
    let mut marks = Vec::new();
    if z_min.is_finite() && y_max > 0.0 {
        let half = y_max * 1.04;
        let left = framing.project([0.0, -half, z_min]);
        let right = framing.project([0.0, half, z_min]);
        ground.push(SceneElement::Line {
            p1: left,
            p2: right,
            stroke: Stroke::new(muted, 1.2),
        });
        ground.push(SceneElement::Text {
            text: crate::views::tr("Lowest point of the airframe"),
            pos: [left[0].min(right[0]) + 4.0, left[1] + 4.0],
            font_size: CAPTION_FONT,
            color: muted,
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    // Every projected face of the airframe, for the label collision check.
    let faces: Vec<Vec<[f64; 2]>> = if markers.iter().any(|marker| !marker.label.is_empty()) {
        model
            .faces()
            .iter()
            .map(|face| {
                face.points
                    .iter()
                    .map(|point| framing.project(*point))
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    let ground_y =
        (z_min.is_finite() && y_max > 0.0).then(|| framing.project([0.0, 0.0, z_min])[1]);
    let at: Vec<[f64; 2]> = markers
        .iter()
        .map(|marker| framing.project([0.0, marker.y_m, marker.z_m]))
        .collect();
    let centre_x = framing.project([0.0, 0.0, 0.0])[0];
    let outward = markers
        .iter()
        .zip(&at)
        .max_by(|a, b| a.0.y_m.abs().total_cmp(&b.0.y_m.abs()))
        .map_or(
            1.0,
            |(_, point)| if point[0] >= centre_x { 1.0 } else { -1.0 },
        );
    let mut placed: Vec<LabelRect> = Vec::new();
    for (marker, &dot) in markers.iter().zip(&at) {
        marks.push(SceneElement::Circle {
            center: dot,
            radius: if marker.label.is_empty() { 2.5 } else { 4.0 },
            fill: Some(Fill::new(Color::from_hex(palette.accent))),
            stroke: Some(Stroke::new(ink, 1.2)),
        });
        if marker.label.is_empty() {
            continue;
        }
        let width = marker.label.chars().count() as f64 * LABEL_FONT * 0.62 + 8.0;
        let side = if (dot[0] - centre_x).abs() < 1.0e-6 {
            outward
        } else if dot[0] > centre_x {
            1.0
        } else {
            -1.0
        };
        let wing_y = |x: f64| wing_line_y(&at, x);
        let candidate = |shift: f64, lift: f64| {
            let cx = dot[0] + side * shift;
            let bottom = wing_y(cx) - lift;
            LabelRect {
                left: cx - 0.5 * width,
                right: cx + 0.5 * width,
                top: bottom - LABEL_HEIGHT,
                bottom,
            }
        };
        // The leader from the dot to the label may leave the panel the dot
        // sits on, but never crosses another part of the airframe.
        let leader_clear = |from: [f64; 2], to: [f64; 2]| {
            (2..=10).all(|step| {
                let t = f64::from(step) / 10.0;
                let probe = [
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                ];
                !faces
                    .iter()
                    .any(|face| !point_in_polygon(from, face) && point_in_polygon(probe, face))
            })
        };
        let valid = |rect: &LabelRect| {
            rect.left >= 4.0
                && rect.right <= canvas.0 - 4.0
                && rect.top >= 4.0
                && ground_y.is_none_or(|y| rect.bottom < y - 6.0 || rect.top > y + 22.0)
                && !placed.iter().any(|other| rect.overlaps(other, 3.0))
                && !faces.iter().any(|face| rect.touches_polygon(face, 3.0))
        };
        let leader_ok = |rect: &LabelRect| {
            leader_clear(dot, [0.5 * (rect.left + rect.right), rect.bottom + 2.0])
        };
        let mut options: Vec<(f64, f64)> = Vec::new();
        for shift in [0.0, 12.0, 24.0, 40.0, 58.0, 80.0, 104.0] {
            for lift in [14.0, 22.0, 32.0, 44.0, 58.0] {
                options.push((shift, lift));
            }
        }
        // The nearest clear spot wins: the shortest leader.
        options.sort_by(|a, b| (a.0 + 1.5 * a.1).total_cmp(&(b.0 + 1.5 * b.1)));
        // A spot whose leader stays off the rest of the airframe wins; a
        // marker inside the fuselage (the root) has none and takes the
        // nearest clear spot.
        let (shift, lift) = options
            .iter()
            .copied()
            .find(|&(shift, lift)| {
                let rect = candidate(shift, lift);
                valid(&rect) && leader_ok(&rect)
            })
            .or_else(|| {
                options
                    .iter()
                    .copied()
                    .find(|&(shift, lift)| valid(&candidate(shift, lift)))
            })
            .unwrap_or((0.0, 22.0));
        let rect = candidate(shift, lift);
        let anchor = [0.5 * (rect.left + rect.right), rect.bottom + 2.0];
        if (anchor[0] - dot[0]).abs() > 3.0 || (anchor[1] - dot[1]).abs() > 12.0 {
            marks.push(SceneElement::Line {
                p1: [dot[0], dot[1] - 4.0],
                p2: anchor,
                stroke: Stroke::new(muted, 0.9),
            });
        }
        marks.push(SceneElement::Text {
            text: marker.label.clone(),
            pos: [anchor[0], rect.bottom],
            font_size: LABEL_FONT,
            color: ink,
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        placed.push(rect);
    }
    // The ground line goes behind the airframe; the markers on top of it.
    for (index, element) in ground.into_iter().enumerate() {
        scene.elements.insert(index, element);
    }
    scene.elements.extend(marks);
    Some(scene)
}

/// How far inside the scene canvas the engine places its viewport, scene
/// units per side (the margin `SandboxSceneModel::render` keeps).
pub const FRAMED_CANVAS_MARGIN: f64 = 20.0;

/// The azimuth of the view from behind the aircraft, looking forward: model
/// `+y` is drawn to the right and `+z` up, as the payload editor's numbers read.
pub const BEHIND_AZIM_DEG: f64 = -90.0;

/// The aircraft drawn at an exact scale and place, for a caller that paints
/// its own overlay on the render.
///
/// `viewport_px` is the size, in scene units, of the area the picture fills;
/// the scene canvas is larger by [`FRAMED_CANVAS_MARGIN`] on every side. The
/// model point `(y, z) = centre_yz` is at the middle of the viewport and one
/// metre is `px_per_m` scene units both ways, so a scene point maps back to
/// `(y, z)` without a fit or a stretch. The view is orthographic, so
/// depth does not move a point.
pub fn build_framed_view(
    geometry: alas_config::GeometryConfig,
    design: alas_config::DesignVector,
    theme: &str,
    viewport_px: (f64, f64),
    centre_yz: (f64, f64),
    px_per_m: f64,
) -> Option<(Scene, alas_report::families::geometry::SceneFraming)> {
    let model = live_preview_cache::model(super::interactive_geometry_of(geometry), design)?;
    let mut options = live_preview_options();
    options.canvas = (
        viewport_px.0 + 2.0 * FRAMED_CANVAS_MARGIN,
        viewport_px.1 + 2.0 * FRAMED_CANVAS_MARGIN,
    );
    options.reference = Some(alas_report::families::geometry::FramingReference {
        center: [0.0, centre_yz.0, centre_yz.1],
        extent: viewport_px.0.min(viewport_px.1) * 0.45 / px_per_m.max(1.0e-9),
    });
    let camera = Camera3D {
        elev_deg: 0.0,
        azim_deg: BEHIND_AZIM_DEG,
        zoom: 1.0,
    };
    Some(model.render(Some(camera), Some(theme), &options))
}

/// The azimuth of the side view: model `+x` (towards the tail) is drawn to the
/// right and `+z` up, as the payload editor's station axis reads.
pub const SIDE_AZIM_DEG: f64 = 0.0;

/// The aircraft seen from the side at an exact scale and place, as
/// [`build_framed_view`] does from behind: the model point `(x, z) =
/// centre_xz` is at the middle of the viewport and one metre is `px_per_m`
/// scene units both ways.
pub fn build_framed_side_view(
    geometry: alas_config::GeometryConfig,
    design: alas_config::DesignVector,
    theme: &str,
    viewport_px: (f64, f64),
    centre_xz: (f64, f64),
    px_per_m: f64,
) -> Option<(Scene, alas_report::families::geometry::SceneFraming)> {
    let model = live_preview_cache::model(super::interactive_geometry_of(geometry), design)?;
    let mut options = live_preview_options();
    options.canvas = (
        viewport_px.0 + 2.0 * FRAMED_CANVAS_MARGIN,
        viewport_px.1 + 2.0 * FRAMED_CANVAS_MARGIN,
    );
    options.reference = Some(alas_report::families::geometry::FramingReference {
        center: [centre_xz.0, 0.0, centre_xz.1],
        extent: viewport_px.0.min(viewport_px.1) * 0.45 / px_per_m.max(1.0e-9),
    });
    let camera = Camera3D {
        elev_deg: 0.0,
        azim_deg: SIDE_AZIM_DEG,
        zoom: 1.0,
    };
    Some(model.render(Some(camera), Some(theme), &options))
}

/// The cabin's seat rows seen from the side on a transparent canvas, at the
/// same scale, centre and margin as [`build_framed_side_view`].
pub fn build_framed_seat_layer(
    rows: &[alas_report::families::geometry::SeatProfile],
    viewport_px: (f64, f64),
    centre_xz: (f64, f64),
    px_per_m: f64,
) -> Scene {
    alas_report::families::geometry::side_seat_layer(
        rows,
        viewport_px,
        centre_xz,
        px_per_m,
        FRAMED_CANVAS_MARGIN,
    )
}
