// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_vn_diagram`)
// Reference: alas @ rust-port-baseline.

//! The CS-25 V-n (flight envelope) diagram: filled colour bands between the
//! stall boundary and the limit/ultimate load factors, the hatched
//! stall-limited region, and the VA/VS/VD/cruise markers.

use alas_perf::performance::VnDiagramData;

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

use super::support::diamond_points;

const RED: (u8, u8, u8) = (255, 77, 77);
const ORANGE: &str = "#ff9933";
const YELLOW: &str = "#ffeb3b";
const GREEN: &str = "#66cc66";
const CRUISE_BLUE: &str = "tab:blue";

/// Generate the CS-25-style flight maneuver and gust envelope (V-n diagram).
///
/// Every boundary comes straight off `vn`: the output of
/// [`alas_perf::performance::build_vn_diagram`], which is itself derived from
/// `DesignRequirements`/`PerformanceConfig` and the analyzed wing's own
/// reference area, rather than being recomputed here; this function only
/// draws it. Reproduces upstream's layered bands in the same paint order
/// (never-exceed red, structural-margin orange, caution yellow / normal
/// green, the hatched stall-limited region, then the boundary lines and
/// markers) rather than the earlier stub's single boundary trace.
pub fn figure_vn_diagram(vn: &VnDiagramData, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(640.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("V-n Flight Envelope (EAS [kt] vs Load Factor)".to_owned());
    draw_title(
        &mut scene,
        "V-n Flight Envelope (EAS [kt] vs Load Factor)",
        pal,
    );
    scene.suppress_derived_title();
    if let Err(message) = vn.validate_speed_order() {
        scene.add(SceneElement::Text {
            text: format!("INVALID: {message}"),
            pos: [320.0, 450.0],
            font_size: 10.0,
            color: Color::rgba(RED.0, RED.1, RED.2, 255),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    let v_max = vn
        .v_kt
        .last()
        .copied()
        .unwrap_or(vn.v_d_kt * 1.15)
        .max(vn.v_d_kt);
    let y_pad = 0.15 * (vn.n_ult_pos - vn.n_ult_neg);
    let axes = Axes2D::new(
        (70.0, 40.0, 500.0, 340.0),
        (0.0, v_max),
        (vn.n_ult_neg - y_pad, vn.n_ult_pos + y_pad),
    );
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "Equivalent airspeed EAS [kt]",
        "Load factor n [-]",
    );

    // The envelope curves are only ever traced up to VD; beyond it the
    // never-exceed band fills flat to the axis edge (`ax.axvspan`).
    let env_idx: Vec<usize> = vn
        .v_kt
        .iter()
        .enumerate()
        .filter(|&(_, &v)| v <= vn.v_d_kt)
        .map(|(i, _)| i)
        .collect();
    let v_env: Vec<f64> = env_idx.iter().map(|&i| vn.v_kt[i]).collect();
    let stall_pos: Vec<f64> = env_idx.iter().map(|&i| vn.n_stall_pos[i]).collect();
    let stall_neg: Vec<f64> = env_idx.iter().map(|&i| vn.n_stall_neg[i]).collect();

    // --- Never-exceed band (outermost, red, alpha 0.35) ---
    let never_exceed = Color::rgba(RED.0, RED.1, RED.2, 89);
    fill_band(
        &mut scene,
        &axes,
        &v_env,
        &stall_neg,
        &stall_pos,
        never_exceed,
    );
    let p0 = axes.map_point(vn.v_d_kt, axes.y_max);
    let p1 = axes.map_point(v_max, axes.y_min);
    scene.add(SceneElement::Rect {
        x: p0[0],
        y: p0[1],
        width: (p1[0] - p0[0]).max(0.0),
        height: (p1[1] - p0[1]).max(0.0),
        rx: 0.0,
        fill: Some(Fill::new(never_exceed)),
        stroke: None,
    });

    // --- Structural margin band (limit -> ultimate load, orange) ---
    let org_up_pos: Vec<f64> = stall_pos.iter().map(|&s| s.min(vn.n_ult_pos)).collect();
    let org_lo_pos: Vec<f64> = stall_pos.iter().map(|&s| s.min(vn.n_lim_pos)).collect();
    let org_up_neg: Vec<f64> = stall_neg.iter().map(|&s| s.max(vn.n_ult_neg)).collect();
    let org_lo_neg: Vec<f64> = stall_neg.iter().map(|&s| s.max(vn.n_lim_neg)).collect();
    let orange = Color::from_hex(ORANGE);
    fill_band(&mut scene, &axes, &v_env, &org_lo_pos, &org_up_pos, orange);
    fill_band(&mut scene, &axes, &v_env, &org_up_neg, &org_lo_neg, orange);
    let boundary_color = if Color::from_hex(pal.bg).relative_luminance() < 0.5 {
        Color::from_hex("#ffffff")
    } else {
        Color::from_hex("#000000")
    };
    let boundary = Stroke::new(boundary_color, 1.4);
    line_series(&mut scene, &axes, &v_env, &org_up_pos, boundary.clone());
    line_series(&mut scene, &axes, &v_env, &org_up_neg, boundary.clone());

    // --- Caution (VC..VD, yellow) and normal (<=VC, green) bands ---
    let lim_up: Vec<f64> = stall_pos.iter().map(|&s| s.min(vn.n_lim_pos)).collect();
    let lim_dw: Vec<f64> = stall_neg.iter().map(|&s| s.max(vn.n_lim_neg)).collect();
    let split = v_env
        .iter()
        .position(|&v| v >= vn.v_c_kt)
        .unwrap_or(v_env.len());
    fill_band_slice(
        &mut scene,
        &axes,
        &v_env,
        &lim_dw,
        &lim_up,
        0,
        split,
        Color::from_hex(GREEN),
    );
    fill_band_slice(
        &mut scene,
        &axes,
        &v_env,
        &lim_dw,
        &lim_up,
        split,
        v_env.len(),
        Color::from_hex(YELLOW),
    );
    line_series(&mut scene, &axes, &v_env, &lim_up, boundary.clone());
    line_series(&mut scene, &axes, &v_env, &lim_dw, boundary.clone());

    // Make the normal/caution and caution/never-exceed transitions explicit;
    // relying on adjacent fills leaves those boundaries visually ambiguous.
    scene.add(SceneElement::Line {
        p1: axes.map_point(vn.v_c_kt, vn.n_lim_neg),
        p2: axes.map_point(vn.v_c_kt, vn.n_lim_pos),
        stroke: boundary.clone(),
    });

    // Vertical VD boundary from ultimate-negative to ultimate-positive.
    scene.add(SceneElement::Line {
        p1: axes.map_point(vn.v_d_kt, vn.n_ult_neg),
        p2: axes.map_point(vn.v_d_kt, vn.n_ult_pos),
        stroke: boundary,
    });

    draw_hatch(&mut scene, &axes, vn.v_s_kt);

    draw_legend(
        &mut scene,
        [axes.left + 10.0, axes.top + axes.height - 68.0],
        &[
            (
                "Normal".to_owned(),
                LegendMarker::Patch(Color::from_hex(GREEN)),
            ),
            (
                "Caution".to_owned(),
                LegendMarker::Patch(Color::from_hex(YELLOW)),
            ),
            ("Structural margin".to_owned(), LegendMarker::Patch(orange)),
            (
                "Never exceed".to_owned(),
                LegendMarker::Patch(Color::rgba(RED.0, RED.1, RED.2, 153)),
            ),
        ],
        pal,
        8.0,
    );

    draw_markers(&mut scene, &axes, vn, pal.title);

    scene
}

/// One `fill_between(x, y_lower, y_upper)` as a closed polygon.
fn fill_band(
    scene: &mut Scene,
    axes: &Axes2D,
    x: &[f64],
    y_lower: &[f64],
    y_upper: &[f64],
    color: Color,
) {
    fill_band_slice(scene, axes, x, y_lower, y_upper, 0, x.len(), color);
}

/// A `fill_between` restricted to `[start, end)`, for the caution/normal
/// split: upstream's `where=` masked calls.
#[allow(clippy::too_many_arguments)]
fn fill_band_slice(
    scene: &mut Scene,
    axes: &Axes2D,
    x: &[f64],
    y_lower: &[f64],
    y_upper: &[f64],
    start: usize,
    end: usize,
    color: Color,
) {
    if end.saturating_sub(start) < 2 {
        return;
    }
    let mut points = Vec::with_capacity((end - start) * 2);
    for i in start..end {
        points.push(axes.map_point(
            x[i].clamp(axes.x_min, axes.x_max),
            y_upper[i].clamp(axes.y_min, axes.y_max),
        ));
    }
    for i in (start..end).rev() {
        points.push(axes.map_point(
            x[i].clamp(axes.x_min, axes.x_max),
            y_lower[i].clamp(axes.y_min, axes.y_max),
        ));
    }
    scene.add(SceneElement::Polygon {
        points,
        fill: Some(Fill::new(color)),
        stroke: None,
    });
}

fn line_series(scene: &mut Scene, axes: &Axes2D, x: &[f64], y: &[f64], stroke: Stroke) {
    let pts: Vec<(f64, f64)> = x.iter().zip(y).map(|(&a, &b)| (a, b)).collect();
    axes.add_line_series(scene, &pts, stroke);
}

/// Diagonal hatch lines across the stall-limited region (`0..v_s_kt`, full
/// axis height): the closest approximation [`crate::scene`]'s primitives
/// offer to matplotlib's `hatch="///"`, since there is no dedicated hatch fill.
fn draw_hatch(scene: &mut Scene, axes: &Axes2D, v_s_kt: f64) {
    let top_left = axes.map_point(0.0, axes.y_max);
    let bottom_right = axes.map_point(v_s_kt, axes.y_min);
    let (x0, y0) = (top_left[0], top_left[1]);
    let (x1, y1) = (bottom_right[0], bottom_right[1]);
    let w = x1 - x0;
    let h = y1 - y0;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let stroke = Stroke::new(Color::rgba(0, 0, 0, 38), 1.0);
    let spacing = 9.0;
    let mut c = 0.0;
    while c <= w + h {
        let u_start = (c - h).max(0.0);
        let u_end = c.min(w);
        if u_end > u_start {
            let p_a = [x0 + u_start, y0 + (c - u_start)];
            let p_b = [x0 + u_end, y0 + (c - u_end)];
            scene.add(SceneElement::Line {
                p1: p_a,
                p2: p_b,
                stroke: stroke.clone(),
            });
        }
        c += spacing;
    }
}

fn draw_markers(scene: &mut Scene, axes: &Axes2D, vn: &VnDiagramData, label_fg: &str) {
    let fg = Color::from_hex(label_fg);
    let black = Color::rgb(0, 0, 0);

    let va = axes.map_point(vn.v_a_kt, vn.n_lim_pos);
    scene.add(dot(va, black));
    two_line_label(
        scene,
        va,
        (0.0, 10.0),
        TextAlign::Center,
        fg,
        "VA",
        &format!("{:.0} kt", vn.v_a_kt),
    );

    let vs = axes.map_point(vn.v_s_kt, 1.0);
    scene.add(dot(vs, black));
    two_line_label(
        scene,
        vs,
        (-10.0, 0.0),
        TextAlign::Right,
        fg,
        "VS",
        &format!("{:.0} kt", vn.v_s_kt),
    );

    let vd = axes.map_point(vn.v_d_kt, vn.n_ult_pos);
    let vd_color = Color::rgb(192, 57, 43);
    two_line_label(
        scene,
        vd,
        (-6.0, 6.0),
        TextAlign::Right,
        vd_color,
        "VD",
        &format!("{:.0} kt", vn.v_d_kt),
    );

    let cruise = axes.map_point(vn.v_cruise_op_kt, 1.0);
    let cruise_color = Color::from_hex(CRUISE_BLUE);
    scene.add(SceneElement::Polygon {
        points: diamond_points(cruise, 5.0),
        fill: Some(Fill::new(cruise_color)),
        stroke: None,
    });
    two_line_label(
        scene,
        cruise,
        (10.0, 10.0),
        TextAlign::Left,
        cruise_color,
        "Cruise",
        &format!("{:.0} kt", vn.v_cruise_op_kt),
    );
}

fn dot(pos: Point2D, color: Color) -> SceneElement {
    SceneElement::Circle {
        center: pos,
        radius: 3.0,
        fill: Some(Fill::new(color)),
        stroke: None,
    }
}

/// Two stacked lines of text anchored near `pos`, offset by `(dx, dy)` in
/// "offset points" the way matplotlib's `annotate(..., textcoords="offset
/// points")` is (`dy` positive is *up* the page, so it flips sign against
/// canvas pixels, which grow downward).
fn two_line_label(
    scene: &mut Scene,
    pos: Point2D,
    (dx, dy): (f64, f64),
    align: TextAlign,
    color: Color,
    line1: &str,
    line2: &str,
) {
    let anchor = [pos[0] + dx, pos[1] - dy];
    let line_h = 11.0;
    scene.add(SceneElement::Text {
        text: line1.to_owned(),
        pos: anchor,
        font_size: 9.5,
        color,
        align,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: line2.to_owned(),
        pos: [anchor[0], anchor[1] + line_h],
        font_size: 9.5,
        color,
        align,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::render_svg;

    fn sample_vn() -> VnDiagramData {
        VnDiagramData {
            v_kt: vec![0.0, 100.0, 200.0, 250.0, 300.0, 340.0, 391.0],
            n_stall_pos: vec![0.0, 0.8, 2.5, 2.5, 2.5, 2.5, 2.5],
            n_stall_neg: vec![0.0, -0.3, -1.0, -1.0, -1.0, -1.0, -1.0],
            n_lim_pos: 2.5,
            far25_positive_limit_load_factor_min: Some(2.5),
            far25_positive_load_factor_status:
                alas_perf::performance::Far25PositiveLoadFactorStatus::MeetsMinimum,
            n_lim_neg: -1.0,
            n_ult_pos: 3.75,
            n_ult_neg: -1.5,
            v_s_kt: 110.0,
            v_a_kt: 180.0,
            v_c_kt: 280.0,
            v_d_kt: 340.0,
            v_cruise_op_kt: 250.0,
        }
    }

    #[test]
    fn the_diagram_fills_every_named_colour_band_and_the_hatch() {
        let scene = figure_vn_diagram(&sample_vn(), None);
        let svg = render_svg(&scene);
        for hex in ["#ff4d4d", "#ff9933", "#ffeb3b", "#66cc66"] {
            assert!(svg.contains(hex), "missing band colour {hex}");
        }
        // The stall-limited hatch and the VD boundary both render as lines.
        assert!(svg.contains("<line"));
        assert!(svg.contains("VA"));
        assert!(svg.contains("Cruise"));
    }

    #[test]
    fn the_never_exceed_band_reaches_past_vd_to_the_axis_edge() {
        // A case where the last knot sits well beyond VD: the axvspan
        // rectangle must still cover that whole tail.
        let vn = sample_vn();
        let scene = figure_vn_diagram(&vn, None);
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        assert!(rects >= 1, "expected the beyond-VD rectangle to be drawn");
    }

    #[test]
    fn envelope_fills_are_clipped_to_the_plot_bounds() {
        let mut vn = sample_vn();
        vn.n_stall_pos[2] = 99.0;
        vn.n_stall_neg[2] = -99.0;
        let scene = figure_vn_diagram(&vn, None);
        for element in scene.elements {
            if let SceneElement::Polygon { points, .. } = element {
                for [x, y] in points {
                    assert!((70.0..=570.0).contains(&x));
                    assert!((40.0..=380.0).contains(&y));
                }
            }
        }
    }

    #[test]
    fn an_empty_velocity_axis_does_not_panic() {
        let mut vn = sample_vn();
        vn.v_kt.clear();
        vn.n_stall_pos.clear();
        vn.n_stall_neg.clear();
        let scene = figure_vn_diagram(&vn, Some("dark"));
        assert!(!scene.elements.is_empty());
    }
}
