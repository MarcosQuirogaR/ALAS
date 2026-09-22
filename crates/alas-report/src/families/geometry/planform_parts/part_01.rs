// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_opt::history::OptimizationHistory;

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::{get_palette, BASELINE_COLOR, OPTIMIZED_COLOR};

use super::shared::{airplane_bbox, draw_planform, equal_aspect_ranges};

/// Generate the three-projection (top / side / front) geometry view of an
/// [`Airplane`]'s wings and fuselages: `figure_geometry`.
///
/// Reproduces upstream's fuselage silhouette exactly, including the
/// documented quirk it does *not* fix: every `FuselageXSec` uses `width / 2`
/// as the vertical half-extent in the side and front panels too, rather than
/// `height / 2`, so a body whose declared height differs from its width (an
/// ovoid double-decker) is drawn as if it were circular in those two panels.
pub fn figure_geometry(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(760.0, 560.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Aircraft Geometry (top / side / front)".to_owned());

    let (x_min, x_max, y_min, y_max, z_min, z_max) = airplane_bbox(plane);

    // Top: horizontal = Y (span), vertical = -X (nose at the top, matching
    // `ax_top.invert_yaxis()`).
    let top_rect = (50.0, 50.0, 660.0, 210.0);
    let (top_u, top_v) =
        equal_aspect_ranges(y_min, y_max, top_rect.2, -x_max, -x_min, top_rect.3, 0.15);
    let top_axes = Axes2D::new(top_rect, top_u, top_v);
    top_axes.draw_frame_with_labels(&mut scene, pal, "span Y [m]", "longitudinal X [m]");

    // Side: horizontal = X, vertical = Z, no inversion.
    let side_rect = (50.0, 310.0, 320.0, 200.0);
    let (side_u, side_v) =
        equal_aspect_ranges(x_min, x_max, side_rect.2, z_min, z_max, side_rect.3, 0.2);
    let side_axes = Axes2D::new(side_rect, side_u, side_v);
    side_axes.draw_frame_with_labels(&mut scene, pal, "X [m]", "Z [m]");

    // Front: horizontal = Y, vertical = Z, no inversion.
    let front_rect = (410.0, 310.0, 300.0, 200.0);
    let (front_u, front_v) =
        equal_aspect_ranges(y_min, y_max, front_rect.2, z_min, z_max, front_rect.3, 0.2);
    let front_axes = Axes2D::new(front_rect, front_u, front_v);
    front_axes.draw_frame_with_labels(&mut scene, pal, "span Y [m]", "Z [m]");

    let edge = Stroke::new(Color::rgb(0, 0, 0), 0.5);
    for (index, wing) in plane.wings.iter().enumerate() {
        let wing_color = match index {
            0 => Color::from_hex("#2563eb"),
            1 => Color::from_hex("#e67e22"),
            _ => Color::from_hex("#16a085"),
        };
        let (x, y, z) = wing_loop(wing);
        let sides: &[f64] = if wing.symmetric { &[1.0, -1.0] } else { &[1.0] };
        for &side in sides {
            let ys: Vec<f64> = y.iter().map(|&yi| yi * side).collect();
            fill_polygon(
                &mut scene,
                &top_axes,
                &ys,
                &x,
                wing_color,
                0.35,
                Some(edge.clone()),
                true,
            );
            fill_polygon(
                &mut scene,
                &side_axes,
                &x,
                &z,
                wing_color,
                0.35,
                Some(edge.clone()),
                false,
            );
            fill_polygon(
                &mut scene,
                &front_axes,
                &ys,
                &z,
                wing_color,
                0.35,
                Some(edge.clone()),
                false,
            );
        }
    }
    panel_label(&mut scene, top_rect, "top (Y-X)", pal);
    panel_label(&mut scene, side_rect, "side (X-Z)", pal);
    panel_label(&mut scene, front_rect, "front (Y-Z)", pal);

    let fus_color = Color::from_hex("#9b59b6");
    for fus in &plane.fuselages {
        let xc: Vec<f64> = fus.xsecs.iter().map(|s| s.xyz_c[0]).collect();
        let zc: Vec<f64> = fus.xsecs.iter().map(|s| s.xyz_c[2]).collect();
        // Reproduces upstream's width-for-height substitution; see the doc.
        let r: Vec<f64> = fus.xsecs.iter().map(|s| s.width / 2.0).collect();
        let mut x_loop = xc.clone();
        x_loop.extend(xc.iter().rev());
        let mut z_loop: Vec<f64> = zc.iter().zip(&r).map(|(&z, &ri)| z + ri).collect();
        z_loop.extend(zc.iter().zip(&r).rev().map(|(&z, &ri)| z - ri));
        let mut y_loop = r.clone();
        y_loop.extend(r.iter().rev().map(|&ri| -ri));

        fill_polygon(
            &mut scene, &side_axes, &x_loop, &z_loop, fus_color, 0.4, None, false,
        );
        fill_polygon(
            &mut scene, &top_axes, &y_loop, &x_loop, fus_color, 0.4, None, true,
        );
        fill_polygon(
            &mut scene,
            &front_axes,
            &y_loop,
            &z_loop,
            fus_color,
            0.2,
            None,
            false,
        );
    }

    draw_legend(
        &mut scene,
        [535.0, 56.0],
        &[
            (
                "Main wing".to_owned(),
                LegendMarker::Patch(Color::from_hex("#2563eb")),
            ),
            (
                "Horizontal stabilizer".to_owned(),
                LegendMarker::Patch(Color::from_hex("#e67e22")),
            ),
            (
                "Vertical stabilizer".to_owned(),
                LegendMarker::Patch(Color::from_hex("#16a085")),
            ),
            ("Fuselage".to_owned(), LegendMarker::Patch(fus_color)),
        ],
        pal,
        8.0,
    );

    scene
}

/// One wing's `(x, y, z)` leading-edge-then-reversed-trailing-edge loop:
/// upstream's local `wing_loops`.
fn wing_loop(w: &Wing) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let le: Vec<[f64; 3]> = w.xsecs.iter().map(|s| s.xyz_le).collect();
    let te: Vec<[f64; 3]> = w
        .xsecs
        .iter()
        .map(|s| [s.xyz_le[0] + s.chord, s.xyz_le[1], s.xyz_le[2]])
        .collect();
    let x = le
        .iter()
        .map(|p| p[0])
        .chain(te.iter().rev().map(|p| p[0]))
        .collect();
    let y = le
        .iter()
        .map(|p| p[1])
        .chain(te.iter().rev().map(|p| p[1]))
        .collect();
    let z = le
        .iter()
        .map(|p| p[2])
        .chain(te.iter().rev().map(|p| p[2]))
        .collect();
    (x, y, z)
}

/// Fill a closed loop given as parallel `(u, v)` data-coordinate slices:
/// `ax.fill(u, v, ...)`. `invert_v` negates `v` before mapping, for a panel
/// built over a negated range (see [`inverted_y`](super::shared::inverted_y)).
#[allow(clippy::too_many_arguments)]
fn fill_polygon(
    scene: &mut Scene,
    axes: &Axes2D,
    u: &[f64],
    v: &[f64],
    color: Color,
    alpha: f64,
    stroke: Option<Stroke>,
    invert_v: bool,
) {
    if u.len() < 3 {
        return;
    }
    let points: Vec<Point2D> = u
        .iter()
        .zip(v)
        .map(|(&ui, &vi)| axes.map_point(ui, if invert_v { -vi } else { vi }))
        .collect();
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, a))),
        stroke,
    });
}

fn panel_label(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    text: &str,
    pal: &crate::theme::Palette,
) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [rect.0 + 4.0, rect.1 - 6.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: crate::scene::TextAlign::Left,
        baseline: crate::scene::TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Generate a top-view planform overlay of a baseline and an optimized
/// airplane: `figure_planform_comparison`.
pub fn figure_planform_comparison(
    baseline: &Airplane,
    optimized: &Airplane,
    labels: (&str, &str),
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    // Matplotlib autoscaled only the outlines `_draw_planform` puts on the
    // axes. Fitting the all-component bounding box instead let nacelles and
    // fuselage stations change a wing-comparison plot's range.
    let mut scene = Scene::new(1100.0, 800.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Planform Comparison".to_owned());
    draw_title(&mut scene, "Planform Comparison", pal);
    scene.suppress_derived_title();

    let (baseline_x, baseline_y) = planform_limits(baseline);
    let (optimized_x, optimized_y) = planform_limits(optimized);
    let x_range = autoscaled_range(
        baseline_x.0.min(optimized_x.0),
        baseline_x.1.max(optimized_x.1),
    );
    let y_range = autoscaled_range(
        baseline_y.0.min(optimized_y.0),
        baseline_y.1.max(optimized_y.1),
    );

    // The source figure is 11 by 8 inches at 100 dpi. Its tight-layout axes
    // box is part of the rendered scale contract captured in W6.5.
    let rect = (137.5, 88.0, 852.5, 616.0);
    let axes = Axes2D::new(rect, x_range, (-y_range.1, -y_range.0));
    axes.draw_frame_with_labels(&mut scene, pal, "span Y [m]", "longitudinal X [m]");

    draw_planform(
        &mut scene,
        &axes,
        baseline,
        Color::from_hex(BASELINE_COLOR),
        2.0,
        true,
        None,
        true,
    );
    draw_planform(
        &mut scene,
        &axes,
        optimized,
        Color::from_hex(OPTIMIZED_COLOR),
        2.0,
        false,
        None,
        true,
    );

    draw_legend(
        &mut scene,
        [rect.0 + rect.2 - 130.0, rect.1 + 8.0],
        &[
            (
                labels.0.to_owned(),
                LegendMarker::Line(Stroke::dashed(
                    Color::from_hex(BASELINE_COLOR),
                    2.0,
                    6.0,
                    4.0,
                )),
            ),
            (
                labels.1.to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0)),
            ),
        ],
        pal,
        10.0,
    );

    scene
}

/// Limits of the top-view wing outlines `_draw_planform` emits.
fn planform_limits(plane: &Airplane) -> ((f64, f64), (f64, f64)) {
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for wing in &plane.wings {
        for section in &wing.xsecs {
            let span = section.xyz_le[1];
            let leading = section.xyz_le[0];
            let trailing = leading + section.chord;
            x_min = x_min.min(span);
            x_max = x_max.max(span);
            y_min = y_min.min(leading);
            y_max = y_max.max(trailing);
            if wing.symmetric {
                x_min = x_min.min(-span);
                x_max = x_max.max(-span);
            }
        }
    }
    if x_min.is_finite() {
        ((x_min, x_max), (y_min, y_max))
    } else {
        ((-1.0, 1.0), (-1.0, 1.0))
    }
}

/// Matplotlib's default 5% data margin for an autoscaled linear axis.
fn autoscaled_range(minimum: f64, maximum: f64) -> (f64, f64) {
    let span = (maximum - minimum).abs();
    let margin = if span > 0.0 { span * 0.05 } else { 0.05 };
    (minimum - margin, maximum + margin)
}

/// `np.linspace(0, len - 1, num, dtype=int)`: truncated (not rounded) index
/// samples spread across `[0, len - 1]`.
fn linspace_int_indices(len: usize, num: usize) -> Vec<usize> {
    if len == 0 || num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![0];
    }
    let step = (len - 1) as f64 / (num - 1) as f64;
    (0..num)
        .map(|i| ((i as f64 * step) as usize).min(len - 1))
        .collect()
}

/// Draw the progress colorbar below a planform so its label can stay
/// horizontal and inside the figure bounds.
fn draw_horizontal_progress_bar(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
    label: &str,
    pal: &crate::theme::Palette,
) {
    let (x, y, width, height) = rect;
    let steps = 64;
    for index in 0..steps {
        let t = index as f64 / (steps - 1) as f64;
        scene.add(SceneElement::Rect {
            x: x + index as f64 * width / steps as f64,
            y,
            width: width / steps as f64 + 0.5,
            height,
            rx: 0.0,
            fill: Some(Fill::new(cmap.sample(t))),
            stroke: None,
        });
    }
    scene.add(SceneElement::Rect {
        x,
        y,
        width,
        height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
    });

    for (fraction, value) in [(0.0, vmin), (0.5, (vmin + vmax) * 0.5), (1.0, vmax)] {
        scene.add(SceneElement::Text {
            text: format!("{value:.3}"),
            pos: [x + fraction * width, y + height + 10.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: if fraction == 0.0 {
                TextAlign::Left
            } else if fraction == 1.0 {
                TextAlign::Right
            } else {
                TextAlign::Center
            },
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [x + width * 0.5, y + height + 25.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}
