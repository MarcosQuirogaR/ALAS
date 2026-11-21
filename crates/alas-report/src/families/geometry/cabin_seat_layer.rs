// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cabin's seat rows seen from the side at an exact scale and place, on a
//! transparent canvas: the layer an editor draws over an airframe render.
//!
//! A row is drawn as the seat it is (pedestal, cushion, reclined backrest and
//! headrest, with the proportions of the 3-D cabin preview and its class
//! colours). When the rows are so close on screen that individual seats can
//! no longer be told apart, each run of rows of one class is one band in the
//! class colour instead.
//!
//! Frame: `x` aft, `z` up, metres. The model point `centre_xz` is at the
//! middle of the `viewport_px` area and one metre is `px_per_m` scene units
//! both ways, with the same `margin` the airframe render keeps around its
//! viewport, so this scene and that one register exactly.

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke};

/// One seat row as the side view needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatProfile {
    /// Station of the row's centre, m.
    pub x: f64,
    /// Floor under the row, m.
    pub floor_z: f64,
    /// Top of the seat pan, m: where the row's occupied envelope begins.
    pub pan_z: f64,
    /// Top of the seat back, m: where the row's occupied envelope ends.
    pub top_z: f64,
    /// Row pitch (longitudinal footprint), m.
    pub length: f64,
    /// The row's class name (`First`, `Business`, `Premium`, else economy).
    pub cls: &'static str,
}

/// Height of a seat back above the cabin floor, m: a seat drawn at its true size is never lower
/// (the occupied envelope of the layout can be thinner than the seat).
pub const SEAT_BACK_HEIGHT_M: f64 = 1.1;

/// Row pitch, scene units, below which seats are drawn as class bands.
pub const MIN_SEAT_PITCH_PX: f64 = 4.0;

/// The cabin legend's colour of a seat class (the 3-D preview's).
#[must_use]
pub fn seat_class_color(cls: &str) -> Color {
    Color::from_hex(match cls {
        "First" => "#8e44ad",
        "Business" => "#2980b9",
        "Premium" => "#16a085",
        _ => "#27ae60",
    })
}

fn shade(color: Color, factor: f64) -> Color {
    let channel = |value: u8| (f64::from(value) * factor).clamp(0.0, 255.0) as u8;
    Color::rgba(channel(color.r), channel(color.g), channel(color.b), 255)
}

/// The seat rows of `rows` seen from the side, on a transparent scene of
/// `viewport_px` plus `margin` per side.
#[must_use]
pub fn side_seat_layer(
    rows: &[SeatProfile],
    viewport_px: (f64, f64),
    centre_xz: (f64, f64),
    px_per_m: f64,
    margin: f64,
) -> Scene {
    let (width, height) = (viewport_px.0 + 2.0 * margin, viewport_px.1 + 2.0 * margin);
    let mut scene = Scene::new(width, height, None);
    scene.render_title = false;
    let project = |x: f64, z: f64| {
        [
            width / 2.0 + (x - centre_xz.0) * px_per_m,
            height / 2.0 - (z - centre_xz.1) * px_per_m,
        ]
    };
    let mut visible: Vec<&SeatProfile> = rows
        .iter()
        .filter(|row| {
            let x = project(row.x, row.floor_z)[0];
            let reach = row.length * px_per_m + 4.0;
            x > -reach && x < width + reach
        })
        .collect();
    visible.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut polygon = |points: &[[f64; 2]], fill: Color, edge: Color, line: f64| {
        scene.add(SceneElement::Polygon {
            points: points.iter().map(|p| project(p[0], p[1])).collect(),
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(edge, line)),
        });
    };
    let mut run: Vec<&SeatProfile> = Vec::new();
    for row in visible {
        if row.length * px_per_m >= MIN_SEAT_PITCH_PX {
            flush_band(&mut polygon, &mut run);
            seat(&mut polygon, row);
            continue;
        }
        let continues = run.last().is_some_and(|last| {
            last.cls == row.cls
                && (row.x - last.x) <= 1.6 * last.length.max(row.length)
                && (row.floor_z - last.floor_z).abs() < 0.2
        });
        if !continues {
            flush_band(&mut polygon, &mut run);
        }
        run.push(row);
    }
    flush_band(&mut polygon, &mut run);
    scene
}

type Polygon<'a> = dyn FnMut(&[[f64; 2]], Color, Color, f64) + 'a;

/// One band over a run of rows too close to tell apart: the seats' footprint
/// from the floor to the top of the backrests.
fn flush_band(polygon: &mut Polygon<'_>, run: &mut Vec<&SeatProfile>) {
    if let (Some(first), Some(last)) = (run.first(), run.last()) {
        let colour = seat_class_color(first.cls);
        let floor = first.floor_z;
        let top = first.top_z.max(floor + SEAT_BACK_HEIGHT_M);
        let (x0, x1) = (first.x - first.length * 0.5, last.x + last.length * 0.5);
        polygon(
            &[[x0, floor], [x1, floor], [x1, top], [x0, top]],
            shade(colour, 0.95),
            shade(colour, 0.6),
            0.6,
        );
    }
    run.clear();
}

/// One seat in profile: pedestal, cushion, reclined backrest and headrest,
/// the backrest at the aft end (occupants face forward).
fn seat(polygon: &mut Polygon<'_>, row: &SeatProfile) {
    let colour = seat_class_color(row.cls);
    let floor = row.floor_z;
    let cushion_length = (row.length * 0.54).max(0.22);
    // A frozen frame starts the envelope at the floor: the pan is then the
    // usual seat height.
    let back_top = row.top_z.max(floor + SEAT_BACK_HEIGHT_M);
    let pan_top = if row.pan_z - floor < 0.2 {
        floor + 0.45_f64.min(0.5 * (back_top - floor))
    } else {
        row.pan_z.min(back_top - 0.2)
    };
    let cushion_thickness = (pan_top - floor).clamp(0.1, 0.4) * 0.30;
    let pan_bottom = pan_top - cushion_thickness;
    let back_length = (row.length * 0.13).clamp(0.08, 0.18);
    let back_x = row.x + cushion_length * 0.42;
    let recline = 0.10 * (back_top - pan_bottom);
    let line = 0.5;
    // Pedestal.
    polygon(
        &[
            [row.x - 0.03, floor],
            [row.x + 0.03, floor],
            [row.x + 0.02, pan_bottom],
            [row.x - 0.02, pan_bottom],
        ],
        shade(colour, 0.55),
        shade(colour, 0.4),
        line,
    );
    // Backrest, reclined from the cushion to its top.
    let head = ((back_top - pan_bottom) * 0.26).clamp(0.08, 0.2);
    let lean = |z: f64| recline * (z - pan_bottom) / (back_top - pan_bottom).max(1.0e-6);
    let (half, z_head) = (back_length * 0.5, back_top - head);
    polygon(
        &[
            [back_x - half, pan_bottom],
            [back_x + half, pan_bottom],
            [back_x + half + lean(z_head), z_head],
            [back_x - half + lean(z_head), z_head],
        ],
        shade(colour, 0.86),
        shade(colour, 0.55),
        line,
    );
    // Headrest, a little thicker and lighter.
    polygon(
        &[
            [back_x - half - 0.03 + lean(z_head), z_head],
            [back_x + half + 0.01 + lean(z_head), z_head],
            [back_x + half + 0.01 + lean(back_top), back_top],
            [back_x - half - 0.03 + lean(back_top), back_top],
        ],
        shade(colour, 1.12),
        shade(colour, 0.6),
        line,
    );
    // Cushion, rounded at the front, joined to the backrest.
    let front = row.x - cushion_length * 0.5;
    polygon(
        &[
            [front, pan_bottom + cushion_thickness * 0.4],
            [front + cushion_thickness * 0.35, pan_top],
            [back_x - half, pan_top],
            [back_x - half, pan_bottom],
            [front + cushion_thickness * 0.3, pan_bottom],
        ],
        colour,
        shade(colour, 0.6),
        line,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(x: f64, cls: &'static str) -> SeatProfile {
        SeatProfile {
            x,
            floor_z: -0.5,
            pan_z: -0.05,
            top_z: 0.5,
            length: 0.8,
            cls,
        }
    }

    fn polygons(scene: &Scene) -> usize {
        scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polygon { .. }))
            .count()
    }

    #[test]
    fn seats_register_at_the_given_scale_and_place() {
        let scene = side_seat_layer(
            &[row(10.0, "Economy")],
            (400.0, 200.0),
            (10.0, 0.0),
            40.0,
            20.0,
        );
        assert!((scene.width - 440.0).abs() < 1e-9 && (scene.height - 240.0).abs() < 1e-9);
        let (mut x0, mut y1) = (f64::MAX, f64::MIN);
        for element in &scene.elements {
            if let SceneElement::Polygon { points, .. } = element {
                for p in points {
                    x0 = x0.min(p[0]);
                    y1 = y1.max(p[1]);
                }
            }
        }
        // Cushion front at x - 0.54 * 0.8 / 2, floor at z = -0.5.
        assert!((x0 - (220.0 - 0.216 * 40.0)).abs() < 1e-6, "{x0}");
        assert!((y1 - (120.0 + 0.5 * 40.0)).abs() < 1e-6, "{y1}");
        assert!(scene.background.is_none());
    }

    #[test]
    fn a_seat_has_four_parts_and_close_rows_become_class_bands() {
        let far = side_seat_layer(&[row(0.0, "First")], (200.0, 100.0), (0.0, 0.0), 40.0, 20.0);
        assert_eq!(polygons(&far), 4);
        let rows: Vec<_> = (0..10)
            .map(|i| row(f64::from(i) * 0.8, "Economy"))
            .chain((10..14).map(|i| row(f64::from(i) * 0.8, "Business")))
            .collect();
        let near = side_seat_layer(&rows, (200.0, 100.0), (4.0, 0.0), 4.0, 20.0);
        assert_eq!(polygons(&near), 2, "one band per class run");
    }
}
