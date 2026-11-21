// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawing aids of the landing-gear planform figure: the dimension
//! callouts and the horizontal legend.

use crate::chart_kit::LegendMarker;
use crate::scene::{Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline};

/// Dimension lines on the drawing, each in a band outside the airframe: the
/// wheelbase (nose to main gear, along the station axis) on the left, the
/// main-gear track (across the outermost wheel centres) under the tail. Thin
/// extension lines run from the wheels to their dimension line and the value
/// is written beside it, away from the drawing.
///
/// `line_at` is the lateral position of the wheelbase line and the station of
/// the track line, in data coordinates.
pub(super) fn draw_dimension_callouts(
    scene: &mut Scene,
    gear: &alas_perf::landing_gear::LandingGearLayout,
    to_px: &dyn Fn(f64, f64) -> Point2D,
    (wheelbase_line_y, track_line_x): (f64, f64),
    pal: &crate::theme::Palette,
) {
    let mean_x = |nose: bool| {
        let xs: Vec<f64> = gear
            .wheels
            .iter()
            .filter(|wheel| (wheel.group == "NLG") == nose)
            .map(|wheel| wheel.x)
            .collect();
        (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
    };
    let (Some(x_nose), Some(x_main)) = (mean_x(true), mean_x(false)) else {
        return;
    };
    let ink = Color::from_hex(pal.title);
    let stroke = Stroke::new(ink, 0.9);
    let extension = Stroke::dashed(Color::from_hex(pal.tick), 0.6, 3.0, 3.0);
    let line = |scene: &mut Scene, p1: Point2D, p2: Point2D, stroke: &Stroke| {
        scene.add(SceneElement::Line {
            p1,
            p2,
            stroke: stroke.clone(),
        });
    };
    let dimension = |scene: &mut Scene,
                     p1: Point2D,
                     p2: Point2D,
                     label: String,
                     at: Point2D,
                     baseline: TextBaseline,
                     angle_deg: f64| {
        line(scene, p1, p2, &stroke);
        let horizontal = (p2[0] - p1[0]).abs() > (p2[1] - p1[1]).abs();
        let (tx, ty) = if horizontal { (0.0, 4.0) } else { (4.0, 0.0) };
        for end in [p1, p2] {
            line(
                scene,
                [end[0] - tx, end[1] - ty],
                [end[0] + tx, end[1] + ty],
                &stroke,
            );
        }
        scene.add(SceneElement::Text {
            text: label,
            pos: at,
            font_size: 11.0,
            color: ink,
            align: TextAlign::Center,
            baseline,
            angle_deg,
            bold: false,
        });
    };
    // Track: the outermost main-gear wheel centres, projected aft onto the
    // line under the tail.
    let half_track = gear.track_width_m / 2.0;
    if half_track > 0.0 {
        for side in [-half_track, half_track] {
            line(
                scene,
                to_px(side, x_main),
                to_px(side, track_line_x),
                &extension,
            );
        }
        let (a, b) = (
            to_px(-half_track, track_line_x),
            to_px(half_track, track_line_x),
        );
        dimension(
            scene,
            a,
            b,
            format!("Track {:.2} m", gear.track_width_m),
            [(a[0] + b[0]) / 2.0, a[1] + 5.0],
            TextBaseline::Top,
            0.0,
        );
    }
    // Wheelbase: nose and main stations projected outboard onto the line in
    // the left band.
    for station in [x_nose, x_main] {
        line(
            scene,
            to_px(0.0, station),
            to_px(wheelbase_line_y, station),
            &extension,
        );
    }
    let (a, b) = (
        to_px(wheelbase_line_y, x_nose),
        to_px(wheelbase_line_y, x_main),
    );
    dimension(
        scene,
        a,
        b,
        format!("Wheelbase {:.2} m", gear.wheelbase_m),
        [a[0] - 5.0, (a[1] + b[1]) / 2.0],
        TextBaseline::Bottom,
        -90.0,
    );
}

/// Draw the gear legend in a compact horizontal grid so each strut category
/// remains visible without consuming the aircraft's longitudinal axis.
pub(super) fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: Point2D,
    width: f64,
    entries: &[(String, LegendMarker)],
    pal: &crate::theme::Palette,
    font_size: f64,
) {
    if entries.is_empty() {
        return;
    }
    let columns = entries.len().clamp(1, 3);
    let column_width = width / columns as f64;
    let row_height = font_size + 6.0;
    for (index, (label, marker)) in entries.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = pos[0] + column as f64 * column_width;
        let y = pos[1] + row as f64 * row_height;
        let mid_y = y + font_size * 0.5;
        match marker {
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y,
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}
