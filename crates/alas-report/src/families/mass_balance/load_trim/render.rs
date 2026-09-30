// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chart area of the load-and-trim sheet (the side panel is `panel`).
//!
//! Layering, bottom to top: shaded "do not operate" field, the clear region
//! inside the ground limits, weight gridlines and the %MAC fan, hatching
//! where a zero-fuel CG is not permitted, the boarding potato with its
//! named orders, the four phase limit sets, structural weight lines, the
//! worked case (the highlighted path).

use super::layers::{draw_limit_sets, draw_potato};
use super::panel::{draw_panel, kg};
use super::{
    envelope_outline, fan_segment, frame_for, limit_at, weight_step_kg, Frame, LoadTrimSheetData,
    SHEET_H, SHEET_W,
};
use crate::scene::{Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;

/// Theme-dependent colours of the sheet.
pub(super) struct Ink {
    pub(super) text: Color,
    pub(super) muted: Color,
    pub(super) paper: Color,
    pub(super) shade: Color,
    pub(super) grid: Color,
    pub(super) fan: Color,
    pub(super) path: Color,
    pub(super) fuel: Color,
    pub(super) envelope: Color,
}

impl Ink {
    fn for_palette(pal: &Palette) -> Self {
        let dark = pal.name.starts_with("dark") || pal.name.starts_with("grey");
        let hex =
            |light: &str, dark_hex: &str| Color::from_hex(if dark { dark_hex } else { light });
        Self {
            text: Color::from_hex(pal.title),
            muted: Color::from_hex(pal.tick),
            paper: Color::from_hex(pal.bg),
            shade: hex("#cfcfcf", "#3b3f47"),
            grid: hex("#dedede", "#30343b"),
            fan: hex("#9a9a9a", "#6a707a"),
            path: hex("#1f5bd6", "#6aa2ff"),
            fuel: hex("#d9770a", "#f0a24a"),
            envelope: hex("#1f5bd6", "#6aa2ff"),
        }
    }
}

pub(super) fn text(
    scene: &mut Scene,
    s: impl Into<String>,
    pos: Point2D,
    size: f64,
    color: Color,
    align: TextAlign,
    bold: bool,
) {
    scene.add(SceneElement::Text {
        text: s.into(),
        pos,
        font_size: size,
        color,
        align,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold,
    });
}

fn line(scene: &mut Scene, p1: Point2D, p2: Point2D, stroke: Stroke) {
    scene.add(SceneElement::Line { p1, p2, stroke });
}

/// Filled arrowhead at `tip`, pointing from `from`.
fn arrowhead(scene: &mut Scene, from: Point2D, tip: Point2D, color: Color) {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = dx.hypot(dy);
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (l, w) = (11.0, 4.5);
    let base = [tip[0] - ux * l, tip[1] - uy * l];
    scene.add(SceneElement::Polygon {
        points: vec![
            tip,
            [base[0] - uy * w, base[1] + ux * w],
            [base[0] + uy * w, base[1] - ux * w],
        ],
        fill: Some(Fill::new(color)),
        stroke: None,
    });
}

/// Circled tag (limit letter or step number).
pub(super) fn tag(
    scene: &mut Scene,
    center: Point2D,
    label: &str,
    fill: Color,
    text_color: Color,
    edge: Color,
) {
    scene.add(SceneElement::Circle {
        center,
        radius: 9.0,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(edge, 1.3)),
    });
    text(
        scene,
        label,
        center,
        10.5,
        text_color,
        TextAlign::Center,
        true,
    );
}

/// Diagonal hatching over the pixels where `inside(index, kg)` holds.
fn hatch(scene: &mut Scene, fr: &Frame, inside: impl Fn(f64, f64) -> bool, color: Color) {
    let spacing = 9.0;
    let span = fr.width + fr.height;
    let mut c = -fr.height;
    while c < fr.width {
        let (mut run_start, mut last): (Option<Point2D>, Point2D) = (None, [0.0, 0.0]);
        let steps = (span / 2.0) as usize;
        for k in 0..=steps {
            let t = k as f64 * 2.0;
            let p = [fr.left + c + t, fr.bottom() - t];
            let ok = p[0] >= fr.left && p[0] <= fr.right() && p[1] >= fr.top && {
                let (i, m) = fr.unmap(p);
                inside(i, m)
            };
            match (ok, run_start) {
                (true, None) => run_start = Some(p),
                (false, Some(s)) => {
                    line(scene, s, last, Stroke::new(color, 0.9));
                    run_start = None;
                }
                _ => {}
            }
            last = p;
        }
        if let Some(s) = run_start {
            line(scene, s, last, Stroke::new(color, 0.9));
        }
        c += spacing;
    }
}

/// Render the balance chart and its side panel.
pub fn figure_load_trim_sheet(data: &LoadTrimSheetData, pal: &Palette) -> Scene {
    let ink = Ink::for_palette(pal);
    let mut scene = Scene::new(SHEET_W, SHEET_H, Some(ink.paper));
    let fr = frame_for(data);
    let px = |i: f64, m: f64| fr.map(i, m);
    let (w0, w1) = fr.w_range_kg;
    let top_kg = data
        .weight("MTOW")
        .unwrap_or_else(|| data.ground_limits.last().map_or(w1, |v| v.mass_kg));
    let mzfw = data.weight("MZFW");
    let fwd_i = |m: f64| data.index_at(m, limit_at(&data.ground_limits, m, true));
    let aft_i = |m: f64| data.index_at(m, limit_at(&data.ground_limits, m, false));

    // 1. Shaded field, clear region inside the ground limits.
    scene.add(SceneElement::Rect {
        x: fr.left,
        y: fr.top,
        width: fr.width,
        height: fr.height,
        rx: 0.0,
        fill: Some(Fill::new(ink.shade)),
        stroke: None,
    });
    let outline: Vec<Point2D> = envelope_outline(data, &data.ground_limits, w0, top_kg)
        .iter()
        .map(|&(i, m)| px(i, m))
        .collect();
    scene.add(SceneElement::Polygon {
        points: outline.clone(),
        fill: Some(Fill::new(ink.paper)),
        stroke: None,
    });

    // 2. Weight gridlines and the %MAC fan (thin, every 2 %).
    let step = weight_step_kg(w1 - w0);
    let mut w = (w0 / step).ceil() * step;
    while w <= w1 + 1e-6 {
        let y = px(fr.i_range.0, w)[1];
        line(
            &mut scene,
            [fr.left, y],
            [fr.right(), y],
            Stroke::new(ink.grid, 0.7),
        );
        w += step;
    }
    let pct_lo = data
        .pct_at(fr.i_range.0, w1)
        .min(data.pct_at(fr.i_range.0, w0))
        .ceil() as i64;
    let pct_hi = data
        .pct_at(fr.i_range.1, w1)
        .max(data.pct_at(fr.i_range.1, w0))
        .floor() as i64;
    for p in pct_lo..=pct_hi {
        if p % 2 != 0 {
            continue;
        }
        if let Some((a, b)) = fan_segment(data, &fr, p as f64) {
            line(
                &mut scene,
                px(a.0, a.1),
                px(b.0, b.1),
                Stroke::new(ink.fan, if p % 10 == 0 { 1.1 } else { 0.6 }),
            );
        }
    }

    // 3. Hatching: inside the takeoff envelope, at or below MZFW, but
    //    outside the zero-fuel limits (a ZFW CG there cannot be fuelled).
    if let (Some(mzfw), false) = (mzfw, data.zfw_limits.is_empty()) {
        let zfw = &data.zfw_limits;
        hatch(
            &mut scene,
            &fr,
            |i, m| {
                m <= mzfw
                    && i >= fwd_i(m)
                    && i <= aft_i(m)
                    && (i < data.index_at(m, limit_at(zfw, m, true))
                        || i > data.index_at(m, limit_at(zfw, m, false)))
            },
            ink.fan,
        );
    }

    // 4. Boarding potato and every composed loading order.
    draw_potato(&mut scene, data, &fr, &ink);

    // 5. Limit sets by phase, then the zero-fuel CG limit outline.
    draw_limit_sets(&mut scene, data, &fr, &ink);
    if let (Some(mzfw), Some(first)) = (mzfw, data.zfw_limits.first()) {
        let bottom = first.mass_kg.max(w0);
        let z: Vec<Point2D> = envelope_outline(data, &data.zfw_limits, bottom, mzfw)
            .iter()
            .map(|&(i, m)| px(i, m))
            .collect();
        scene.add(SceneElement::Polygon {
            points: z,
            fill: None,
            stroke: Some(Stroke::new(ink.muted, 1.2)),
        });
    }

    // 6. Structural weight lines across the envelope.
    for wl in &data.weight_lines {
        if wl.mass_kg < w0 || wl.mass_kg > w1 {
            continue;
        }
        let m = wl.mass_kg.min(top_kg);
        let (a, b) = (px(fwd_i(m), wl.mass_kg), px(aft_i(m), wl.mass_kg));
        line(&mut scene, a, b, Stroke::new(ink.text, 2.0));
        let label = format!("{} {} KG", wl.label, kg(wl.mass_kg));
        // MTOW caps the envelope, so its label goes inside, below the line.
        let below = wl.label == "MTOW";
        scene.add(SceneElement::Text {
            text: label,
            pos: [a[0] + 14.0, a[1] + if below { 4.0 } else { -4.0 }],
            font_size: 11.5,
            color: ink.text,
            align: TextAlign::Left,
            baseline: if below {
                TextBaseline::Top
            } else {
                TextBaseline::Bottom
            },
            angle_deg: 0.0,
            bold: true,
        });
    }

    // 7. %MAC labels in boxes along one row, as on manufacturer sheets.
    // %MAC row in the band above MTOW if it fits, else near the chart top.
    let room_px = px(fr.i_range.0, top_kg)[1] - fr.top;
    let row_kg = if room_px > 24.0 {
        0.5 * (top_kg + w1)
    } else {
        w0 + 0.9 * (w1 - w0)
    };
    let row_y = px(fr.i_range.0, row_kg)[1];
    let mut last_x = f64::NEG_INFINITY;
    text(
        &mut scene,
        "%MAC",
        [fr.left + 24.0, row_y],
        10.0,
        ink.text,
        TextAlign::Center,
        true,
    );
    for p in pct_lo..=pct_hi {
        if p % 2 != 0 {
            continue;
        }
        let x = px(data.index_at(row_kg, p as f64), row_kg)[0];
        if x < fr.left + 56.0 || x > fr.right() - 16.0 || x - last_x < 34.0 {
            continue;
        }
        last_x = x;
        scene.add(SceneElement::Rect {
            x: x - 13.0,
            y: row_y - 8.0,
            width: 26.0,
            height: 16.0,
            rx: 2.0,
            fill: Some(Fill::new(ink.paper)),
            stroke: Some(Stroke::new(ink.fan, 0.8)),
        });
        text(
            &mut scene,
            format!("{p}"),
            [x, row_y],
            10.0,
            ink.text,
            TextAlign::Center,
            false,
        );
    }

    // 8. Worked case: straight loading vectors, fuel curve, numbered states.
    let pts: Vec<Point2D> = data
        .steps
        .iter()
        .map(|s| px(data.index_at(s.mass_kg, s.pct_mac), s.mass_kg))
        .collect();
    let fuel_pts: Vec<Point2D> = data
        .fuel_curve
        .iter()
        .map(|&(m, p)| px(data.index_at(m, p), m))
        .collect();
    if fuel_pts.len() >= 2 {
        scene.add(SceneElement::Polyline {
            points: fuel_pts.clone(),
            stroke: Stroke::dashed(ink.fuel, 2.4, 8.0, 4.0),
        });
        for p in fuel_pts.iter().step_by((fuel_pts.len() / 6).max(1)) {
            scene.add(SceneElement::Circle {
                center: *p,
                radius: 3.0,
                fill: Some(Fill::new(ink.fuel)),
                stroke: None,
            });
        }
    }
    for (k, pair) in pts.windows(2).enumerate() {
        let is_fuel = data
            .steps
            .get(k + 1)
            .is_some_and(|s| s.item.to_ascii_lowercase().contains("fuel"));
        if !is_fuel {
            line(&mut scene, pair[0], pair[1], Stroke::new(ink.path, 2.4));
            arrowhead(&mut scene, pair[0], pair[1], ink.path);
        }
    }
    for (k, p) in pts.iter().enumerate() {
        let fill = if data.steps[k].item.to_ascii_lowercase().contains("fuel") {
            ink.fuel
        } else {
            ink.path
        };
        tag(
            &mut scene,
            *p,
            &format!("{}", k + 1),
            fill,
            ink.paper,
            ink.paper,
        );
    }

    // 9. Frame, index axes top and bottom, weight axis.
    scene.add(SceneElement::Rect {
        x: fr.left,
        y: fr.top,
        width: fr.width,
        height: fr.height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(ink.text, 1.4)),
    });
    let mut i = (fr.i_range.0 / 10.0).ceil() * 10.0;
    while i <= fr.i_range.1 + 1e-9 {
        let x = px(i, w0)[0];
        for (y, dy) in [(fr.bottom(), 5.0), (fr.top, -5.0)] {
            line(&mut scene, [x, y], [x, y + dy], Stroke::new(ink.text, 1.0));
            text(
                &mut scene,
                format!("{i:.0}"),
                [x, y + 3.0 * dy],
                11.0,
                ink.text,
                TextAlign::Center,
                false,
            );
        }
        i += 10.0;
    }
    let mut w = (w0 / step).ceil() * step;
    while w <= w1 + 1e-6 {
        let y = px(fr.i_range.0, w)[1];
        line(
            &mut scene,
            [fr.left - 5.0, y],
            [fr.left, y],
            Stroke::new(ink.text, 1.0),
        );
        text(
            &mut scene,
            kg(w),
            [fr.left - 8.0, y],
            10.5,
            ink.text,
            TextAlign::Right,
            false,
        );
        w += step;
    }
    text(
        &mut scene,
        "INDEX",
        [fr.left + fr.width / 2.0, fr.bottom() + 36.0],
        13.0,
        ink.text,
        TextAlign::Center,
        true,
    );
    scene.add(SceneElement::Text {
        text: "AIRPLANE GROSS WEIGHT - KILOGRAMS".to_owned(),
        pos: [24.0, fr.top + fr.height / 2.0],
        font_size: 12.0,
        color: ink.text,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: true,
    });
    text(
        &mut scene,
        data.title.clone(),
        [fr.left, 22.0],
        15.0,
        ink.text,
        TextAlign::Left,
        true,
    );

    draw_panel(&mut scene, data, &ink, fr.right() + 30.0, fr.top);
    scene
}
