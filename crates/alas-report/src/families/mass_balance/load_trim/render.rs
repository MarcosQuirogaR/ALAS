// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chart area of the load-and-trim sheet (the side panel is `panel`).
//!
//! The chart is a manufacturer-style weight-and-balance envelope in physical
//! coordinates: CG in %MAC across, gross weight up. Layering, bottom to top:
//! grid, the takeoff, landing and zero-fuel envelopes as light filled
//! polygons with thin edges (red where an envelope has no feasible CG
//! range), the dotted en-route-above-MLW outline, the certified comparison
//! lines, the loading potato and its
//! curves, the fuel curve, the loading states. Every text stands on the chart
//! on its own backing clear of the others: labels on the limit lines, the
//! weights at the top of each envelope, the states beside their points.

use super::geom::{text_height, text_width, Geom};
use super::numbers::kg;
use super::panel::{
    draw_panel, gate_failed, panel_height, AFT_LIMIT, AXIS_CG, AXIS_WEIGHT, CERTIFIED, FWD_LIMIT,
};
use super::{frame_in, weight_step_kg, Envelope, EnvelopeKind, Frame, LoadTrimSheetData};
use crate::scene::{Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;

/// Theme-dependent colours of the sheet.
pub(super) struct Ink {
    pub(super) text: Color,
    pub(super) muted: Color,
    pub(super) paper: Color,
    pub(super) panel: Color,
    pub(super) grid: Color,
    pub(super) minor: Color,
    pub(super) alert: Color,
    pub(super) fuel: Color,
    pub(super) certified: Color,
    pub(super) state: Color,
    takeoff: (Color, Color),
    landing: (Color, Color),
    zero_fuel: (Color, Color),
    above_mlw: Color,
}

impl Ink {
    pub(super) fn for_palette(pal: &Palette) -> Self {
        let dark = pal.name.starts_with("dark") || pal.name.starts_with("grey");
        let hex =
            |light: &str, dark_hex: &str| Color::from_hex(if dark { dark_hex } else { light });
        let tint = |light: &str, dark_hex: &str, alpha: u8| {
            let c = hex(light, dark_hex);
            Color::rgba(c.r, c.g, c.b, alpha)
        };
        Self {
            text: Color::from_hex(pal.title),
            muted: hex("#5f6672", "#a7aeb9"),
            paper: Color::from_hex(pal.bg),
            panel: hex("#fafbfc", "#1f2329"),
            grid: hex("#d3d7dd", "#3a3f47"),
            minor: hex("#eceff2", "#2a2e35"),
            alert: hex("#c62828", "#ff6b6b"),
            fuel: hex("#e07b00", "#f0a24a"),
            certified: hex("#7b1fa2", "#d39cf0"),
            state: hex("#14213d", "#e8edf5"),
            takeoff: (tint("#4a78d4", "#6aa2ff", 38), hex("#3a61ae", "#7fb0ff")),
            landing: (tint("#3d9a5a", "#5fcf86", 34), hex("#2e7d46", "#6fd899")),
            zero_fuel: (tint("#e5a02a", "#f2c94c", 40), hex("#b57a10", "#f2c94c")),
            above_mlw: hex("#00838f", "#4dd0e1"),
        }
    }

    /// The en-route-above-MLW outline: dotted, in its own colour, so it reads
    /// apart from the envelopes it rises above.
    pub(super) fn above_mlw_stroke(&self) -> Stroke {
        Stroke::dashed(self.above_mlw, 1.6, 2.0, 2.5)
    }

    /// Fill and edge colours of an envelope.
    pub(super) fn envelope(&self, kind: EnvelopeKind) -> (Color, Color) {
        match kind {
            EnvelopeKind::Takeoff => self.takeoff,
            EnvelopeKind::Landing => self.landing,
            EnvelopeKind::ZeroFuel => self.zero_fuel,
        }
    }

    /// The shading of a band with no feasible CG range.
    pub(super) fn crossed(&self) -> Color {
        Color::rgba(self.alert.r, self.alert.g, self.alert.b, 80)
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

/// The state marker, and the alert ring around it when the state failed.
pub(super) fn marker(scene: &mut Scene, ink: &Ink, center: Point2D, failed: bool) {
    if failed {
        scene.add(SceneElement::Circle {
            center,
            radius: 9.5,
            fill: None,
            stroke: Some(Stroke::new(ink.alert, 2.0)),
        });
    }
    scene.add(SceneElement::Circle {
        center,
        radius: 4.4,
        fill: Some(Fill::new(ink.state)),
        stroke: Some(Stroke::new(ink.paper, 1.2)),
    });
}

/// Filled arrowhead at `tip`, pointing from `from`.
fn arrowhead(scene: &mut Scene, from: Point2D, tip: Point2D, color: Color) {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = dx.hypot(dy);
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (l, w) = (7.0, 2.8);
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

/// An axis-aligned box, px.
#[derive(Debug, Clone, Copy)]
struct Boxed {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
}

impl Boxed {
    fn centered(c: Point2D, w: f64, h: f64) -> Self {
        Self {
            l: c[0] - w / 2.0,
            t: c[1] - h / 2.0,
            r: c[0] + w / 2.0,
            b: c[1] + h / 2.0,
        }
    }

    fn grown(self, d: f64) -> Self {
        Self {
            l: self.l - d,
            t: self.t - d,
            r: self.r + d,
            b: self.b + d,
        }
    }

    fn overlaps(self, o: Self) -> bool {
        self.l < o.r && o.l < self.r && self.t < o.b && o.t < self.b
    }

    fn contains(self, p: Point2D) -> bool {
        p[0] >= self.l && p[0] <= self.r && p[1] >= self.t && p[1] <= self.b
    }

    fn inside(self, o: Self) -> bool {
        self.l >= o.l && self.r <= o.r && self.t >= o.t && self.b <= o.b
    }
}

/// What a label must stay clear of: boxes already placed, the state markers,
/// and the drawn lines.
struct Clearance {
    frame: Boxed,
    boxes: Vec<Boxed>,
    segments: Vec<(Point2D, Point2D)>,
}

impl Clearance {
    /// The cost of putting a label in `at`: infinite outside the frame or
    /// over another label, a little for every line it covers.
    fn cost(&self, at: Boxed) -> f64 {
        if !at.inside(self.frame) {
            return f64::INFINITY;
        }
        let probe = at.grown(1.5);
        if self.boxes.iter().any(|b| b.overlaps(probe)) {
            return f64::INFINITY;
        }
        let mut hits = 0.0;
        for (a, b) in &self.segments {
            let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / 3.0).ceil().max(1.0) as usize;
            if (0..=n).any(|k| {
                let t = k as f64 / n as f64;
                probe.contains([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])])
            }) {
                hits += 1.0;
            }
        }
        hits
    }

    fn take(&mut self, at: Boxed) {
        self.boxes.push(at);
    }
}

/// The CG axis below the frame and the weight axis left of it: a tick and
/// label at every major step, a faint gridline at every whole %MAC when there
/// is room.
fn draw_axes(scene: &mut Scene, fr: &Frame, ink: &Ink, geom: &Geom) {
    let (p0, p1) = fr.x_range_pct;
    let px_per_pct = fr.width / (p1 - p0);
    let label_step = if 5.0 * px_per_pct >= 38.0 { 5.0 } else { 10.0 };
    let (w0, w1) = fr.w_range_kg;
    let w_step = weight_step_kg(w1 - w0);
    if px_per_pct >= 7.0 {
        let mut p = p0.ceil();
        while p <= p1 + 1e-9 {
            let x = fr.map(p, w0)[0];
            line(
                scene,
                [x, fr.top],
                [x, fr.bottom()],
                Stroke::new(ink.minor, 0.6),
            );
            p += 1.0;
        }
    }
    let mut w = (w0 / (w_step / 2.0)).ceil() * (w_step / 2.0);
    while w <= w1 + 1e-6 {
        let y = fr.map(p0, w)[1];
        line(
            scene,
            [fr.left, y],
            [fr.right(), y],
            Stroke::new(ink.minor, 0.6),
        );
        w += w_step / 2.0;
    }
    let tick_font = geom.font(10.5);
    let mut p = (p0 / label_step).ceil() * label_step;
    while p <= p1 + 1e-9 {
        let x = fr.map(p, w0)[0];
        line(
            scene,
            [x, fr.top],
            [x, fr.bottom()],
            Stroke::new(ink.grid, 0.8),
        );
        line(
            scene,
            [x, fr.bottom()],
            [x, fr.bottom() + 5.0],
            Stroke::new(ink.text, 1.0),
        );
        text(
            scene,
            format!("{p:.0}"),
            [x, fr.bottom() + 16.0],
            tick_font,
            ink.text,
            TextAlign::Center,
            false,
        );
        p += label_step;
    }
    let mut w = (w0 / w_step).ceil() * w_step;
    while w <= w1 + 1e-6 {
        let y = fr.map(p0, w)[1];
        line(
            scene,
            [fr.left, y],
            [fr.right(), y],
            Stroke::new(ink.grid, 0.8),
        );
        line(
            scene,
            [fr.left - 5.0, y],
            [fr.left, y],
            Stroke::new(ink.text, 1.0),
        );
        text(
            scene,
            kg(w),
            [fr.left - 9.0, y],
            tick_font,
            ink.text,
            TextAlign::Right,
            false,
        );
        w += w_step;
    }
    text(
        scene,
        AXIS_CG,
        [fr.left + fr.width / 2.0, fr.bottom() + 40.0],
        geom.font(12.0),
        ink.text,
        TextAlign::Center,
        true,
    );
    scene.add(SceneElement::Text {
        text: AXIS_WEIGHT.to_owned(),
        pos: [22.0, fr.top + fr.height / 2.0],
        font_size: geom.font(12.0),
        color: ink.text,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: true,
    });
}

/// An envelope's outline in pixels.
fn pixels(fr: &Frame, envelope: &Envelope) -> Vec<Point2D> {
    envelope
        .outline()
        .into_iter()
        .map(|(pct, mass)| fr.map(pct, mass))
        .collect()
}

/// The envelopes as light filled polygons with thin edges, then a red band
/// wherever one has its forward limit aft of its aft limit.
fn draw_envelopes(scene: &mut Scene, envelopes: &[Envelope], fr: &Frame, ink: &Ink) {
    // Only the takeoff envelope is filled; landing and zero fuel are
    // outlines, so overlapping bands never mix their colours and every edge
    // reads on its own. Drawn widest first, so the narrower outlines stay on top.
    for envelope in envelopes {
        let (fill, edge) = ink.envelope(envelope.kind);
        let filled = envelope.kind == EnvelopeKind::Takeoff;
        let stroke = match envelope.kind {
            EnvelopeKind::ZeroFuel => Stroke::dashed(edge, 1.4, 6.0, 3.0),
            EnvelopeKind::Landing => Stroke::new(edge, 1.4),
            EnvelopeKind::Takeoff => Stroke::new(edge, 1.2),
        };
        scene.add(SceneElement::Polygon {
            points: pixels(fr, envelope),
            fill: filled.then(|| Fill::new(fill)),
            stroke: Some(stroke),
        });
    }
    for envelope in envelopes {
        for (lo, hi) in envelope.crossed_bands() {
            let mut inside: Vec<_> = envelope
                .vertices
                .iter()
                .filter(|v| v.mass_kg > lo && v.mass_kg < hi)
                .copied()
                .collect();
            let at = |m: f64| (envelope.limit_at(m, true), envelope.limit_at(m, false), m);
            let (a, b) = (at(lo), at(hi));
            let mut fwd = vec![fr.map(a.0, a.2)];
            let mut aft = vec![fr.map(a.1, a.2)];
            for v in inside.drain(..) {
                fwd.push(fr.map(v.fwd_pct_mac, v.mass_kg));
                aft.push(fr.map(v.aft_pct_mac, v.mass_kg));
            }
            fwd.push(fr.map(b.0, b.2));
            aft.push(fr.map(b.1, b.2));
            fwd.extend(aft.into_iter().rev());
            scene.add(SceneElement::Polygon {
                points: fwd,
                fill: Some(Fill::new(ink.crossed())),
                stroke: None,
            });
        }
    }
}

impl Envelope {
    /// The forward (`forward`) or aft limit at `mass_kg`, held at the ends.
    fn limit_at(&self, mass_kg: f64, forward: bool) -> f64 {
        super::limit_at(&self.vertices, mass_kg, forward)
    }
}

/// The en-route limits above the MLW as an unfilled outline in their own
/// style; returns the outline drawn (empty when there is none).
fn draw_above_mlw(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    fr: &Frame,
    ink: &Ink,
) -> Vec<Point2D> {
    let Some(set) = data.en_route_above_mlw.as_ref() else {
        return Vec::new();
    };
    let edge = |v: &super::LimitVertex, pct: f64| fr.map(pct, v.mass_kg);
    let mut points: Vec<Point2D> = set
        .vertices
        .iter()
        .map(|v| edge(v, v.fwd_pct_mac))
        .collect();
    points.extend(set.vertices.iter().rev().map(|v| edge(v, v.aft_pct_mac)));
    scene.add(SceneElement::Polygon {
        points: points.clone(),
        fill: None,
        stroke: Some(ink.above_mlw_stroke()),
    });
    points
}

/// The certified comparison lines: thin and dashed in their own colour, cut
/// at the frame's weight range.
fn draw_certified(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    fr: &Frame,
    ink: &Ink,
) -> Vec<Vec<Point2D>> {
    let (w0, w1) = fr.w_range_kg;
    let mut drawn = Vec::new();
    for published in &data.lines.published {
        let corners: Vec<(f64, f64)> = published
            .points
            .iter()
            .copied()
            .filter(|(m, pct)| m.is_finite() && pct.is_finite())
            .collect();
        let (Some(first), Some(last)) = (corners.first(), corners.last()) else {
            continue;
        };
        let (lo, hi) = (first.0.max(w0), last.0.min(w1));
        if hi <= lo {
            continue;
        }
        let at = |m: f64| -> f64 {
            corners
                .windows(2)
                .find(|w| m >= w[0].0 && m <= w[1].0)
                .map_or(last.1, |w| {
                    let t = if w[1].0 > w[0].0 {
                        (m - w[0].0) / (w[1].0 - w[0].0)
                    } else {
                        0.0
                    };
                    w[0].1 + t * (w[1].1 - w[0].1)
                })
        };
        let mut masses = vec![lo, hi];
        masses.extend(corners.iter().map(|c| c.0).filter(|m| *m > lo && *m < hi));
        masses.sort_by(f64::total_cmp);
        masses.dedup();
        let points: Vec<Point2D> = masses.into_iter().map(|m| fr.map(at(m), m)).collect();
        scene.add(SceneElement::Polyline {
            points: points.clone(),
            stroke: Stroke::dashed(ink.certified, 1.4, 6.0, 3.5),
        });
        drawn.push(points);
    }
    drawn
}

/// The registered MTOW as a dashed line across the chart, when it lies above
/// the top of the takeoff envelope (the tanks cannot fill the aircraft to it).
fn draw_registered_mtow(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    envelopes: &[Envelope],
    fr: &Frame,
    ink: &Ink,
    geom: &Geom,
) {
    let Some(mtow) = data.weight(super::MassRole::RegisteredMtow) else {
        return;
    };
    let top = envelopes.iter().map(|e| e.top_kg).fold(f64::NEG_INFINITY, f64::max);
    if !(mtow > top * 1.005) {
        return;
    }
    let y = fr.map(fr.x_range_pct.0, mtow)[1];
    scene.add(SceneElement::Polyline {
        points: vec![[fr.left, y], [fr.right(), y]],
        stroke: Stroke::dashed(ink.muted, 1.2, 6.0, 3.5),
    });
    text(
        scene,
        format!("MTOW {} kg", kg(mtow)),
        [fr.right() - 6.0, y - 8.0],
        geom.font(10.0),
        ink.text,
        TextAlign::Right,
        false,
    );
}

/// The fuel curve from ZFW to TOW along the tank sequence, with an arrowhead
/// at the takeoff end.
fn draw_fuel(scene: &mut Scene, data: &LoadTrimSheetData, fr: &Frame, ink: &Ink) -> Vec<Point2D> {
    let points: Vec<Point2D> = data.fuel_curve.iter().map(|&(m, p)| fr.map(p, m)).collect();
    if points.len() >= 2 {
        scene.add(SceneElement::Polyline {
            points: points.clone(),
            stroke: Stroke::new(ink.fuel, 1.3),
        });
        if let [.., from, tip] = points.as_slice() {
            arrowhead(scene, *from, *tip, ink.fuel);
        }
    }
    points
}

/// Where each envelope's top weight is labelled and where the limit lines
/// are, so the other labels keep clear.
struct LabelPlan {
    obstacles: Clearance,
}

/// The leftmost forward and rightmost aft edge of the envelopes at `mass_kg`
/// (those that span it), %MAC.
fn outer_edges(envelopes: &[Envelope], mass_kg: f64) -> Option<(f64, f64)> {
    let spanning: Vec<&Envelope> = envelopes
        .iter()
        .filter(|e| mass_kg >= e.bottom_kg - 1e-6 && mass_kg <= e.top_kg + 1e-6)
        .collect();
    if spanning.is_empty() {
        return None;
    }
    Some((
        spanning
            .iter()
            .map(|e| e.limit_at(mass_kg, true))
            .fold(f64::INFINITY, f64::min),
        spanning
            .iter()
            .map(|e| e.limit_at(mass_kg, false))
            .fold(f64::NEG_INFINITY, f64::max),
    ))
}

/// Label the forward and aft limit lines, the certified lines and the
/// envelopes' top weights.
#[allow(clippy::too_many_arguments)]
fn draw_line_labels(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    envelopes: &[Envelope],
    fr: &Frame,
    ink: &Ink,
    geom: &Geom,
    plan: &mut LabelPlan,
) {
    let font = geom.font(10.0);
    let (th, fwd_w, aft_w) = (
        text_height(font),
        text_width(FWD_LIMIT, font),
        text_width(AFT_LIMIT, font),
    );
    let Some(bottom) = envelopes.iter().map(|e| e.bottom_kg).reduce(f64::min) else {
        return;
    };
    let top = envelopes
        .iter()
        .map(|e| e.top_kg)
        .fold(f64::NEG_INFINITY, f64::max);

    // Top weights first: they sit on the horizontal edges.
    for envelope in envelopes {
        let label = format!("{} {} kg", envelope.label, kg(envelope.top_kg));
        let w = text_width(&label, font) + 4.0;
        let y = fr.map(0.0, envelope.top_kg)[1];
        let fwd = fr.map(envelope.limit_at(envelope.top_kg, true), envelope.top_kg)[0];
        let aft = fr.map(envelope.limit_at(envelope.top_kg, false), envelope.top_kg)[0];
        // Inside the polygon just under its top edge: right end, left end,
        // then centred; or just above the edge.
        let candidates = [
            ([aft - 5.0 - w / 2.0, y + th * 0.75], false),
            ([fwd + 5.0 + w / 2.0, y + th * 0.75], false),
            ([(fwd + aft) / 2.0, y + th * 0.75], false),
            ([aft - 5.0 - w / 2.0, y - th * 0.75], true),
            ([fwd + 5.0 + w / 2.0, y - th * 0.75], true),
        ];
        let mut best: Option<(f64, Boxed)> = None;
        for (c, _) in candidates {
            let b = Boxed::centered(c, w, th);
            let cost = plan.obstacles.cost(b);
            if best.is_none_or(|(bc, _)| cost < bc) {
                best = Some((cost, b));
            }
            if cost == 0.0 {
                break;
            }
        }
        if let Some((cost, b)) = best.filter(|(c, _)| c.is_finite()) {
            let _ = cost;
            text(
                scene,
                label,
                [(b.l + b.r) / 2.0, (b.t + b.b) / 2.0],
                font,
                ink.text,
                TextAlign::Center,
                false,
            );
            plan.obstacles.take(b);
        }
    }

    // The forward and aft limit lines, labelled outside the outermost
    // envelope edge along their length; inside when the outside is taken.
    for (forward, caption, len) in [(true, FWD_LIMIT, fwd_w), (false, AFT_LIMIT, aft_w)] {
        let mut best: Option<(f64, Boxed, [f64; 2], f64)> = None;
        'search: for frac in [0.62, 0.78, 0.9, 0.5, 0.35, 0.22] {
            let mass = bottom + frac * (top - bottom);
            let Some((fwd, aft)) = outer_edges(envelopes, mass) else {
                continue;
            };
            // The edge's direction at this mass, so the label runs along it.
            let step = 0.03 * (top - bottom);
            let edge = |m: f64| {
                let (f, a) = outer_edges(envelopes, m).unwrap_or((fwd, aft));
                fr.map(if forward { f } else { a }, m)
            };
            let (p0, p1) = (edge(mass - step), edge(mass + step));
            let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
            let norm = dx.hypot(dy).max(1e-9);
            let (ux, uy) = (dx / norm, dy / norm);
            let (ux, uy) = if uy > 0.0 { (-ux, -uy) } else { (ux, uy) };
            let angle = uy.atan2(ux).to_degrees();
            let here = edge(mass);
            for outside in [true, false] {
                let side = if outside == forward { 1.0 } else { -1.0 };
                let off = (th / 2.0 + 5.0) * side;
                let c = [here[0] + uy * off, here[1] - ux * off];
                let (w, h) = (
                    ux.abs() * (len + 4.0) + uy.abs() * th,
                    uy.abs() * (len + 4.0) + ux.abs() * th,
                );
                let b = Boxed::centered(c, w, h);
                let cost = plan.obstacles.cost(b) + if outside { 0.0 } else { 0.5 };
                if best.is_none_or(|(bc, _, _, _)| cost < bc) {
                    best = Some((cost, b, c, angle));
                }
                if cost == 0.0 {
                    break 'search;
                }
            }
        }
        if let Some((cost, b, c, angle)) = best {
            if cost.is_finite() {
                scene.add(SceneElement::Text {
                    text: caption.to_owned(),
                    pos: c,
                    font_size: font,
                    color: ink.text,
                    align: TextAlign::Center,
                    baseline: TextBaseline::Middle,
                    angle_deg: angle,
                    bold: true,
                });
                plan.obstacles.take(b);
            }
        }
    }

    // The certified lines: "Certified (source)" along each, on the side away
    // from the model edge.
    let mut done: Vec<&str> = Vec::new();
    for published in &data.lines.published {
        let key = published.source.as_str();
        let sides = [false, true];
        let corners: Vec<(f64, f64)> = published.points.clone();
        let (Some(first), Some(last)) = (corners.first(), corners.last()) else {
            continue;
        };
        let (lo, hi) = (first.0.max(fr.w_range_kg.0), last.0.min(fr.w_range_kg.1));
        if hi <= lo {
            continue;
        }
        let pct_at = |m: f64| -> f64 {
            corners
                .windows(2)
                .find(|w| m >= w[0].0 && m <= w[1].0)
                .map_or(last.1, |w| {
                    let t = if w[1].0 > w[0].0 {
                        (m - w[0].0) / (w[1].0 - w[0].0)
                    } else {
                        0.0
                    };
                    w[0].1 + t * (w[1].1 - w[0].1)
                })
        };
        let caption = format!("{CERTIFIED} ({key})");
        let len = text_width(&caption, font) + 4.0;
        let mut best: Option<(f64, Boxed, f64)> = None;
        'find: for frac in [0.55, 0.7, 0.4, 0.82, 0.28] {
            let mass = lo + frac * (hi - lo);
            let x = fr.map(pct_at(mass), mass)[0];
            let y = fr.map(0.0, mass)[1];
            for outward in sides {
                let sign = if outward == published.forward {
                    -1.0
                } else {
                    1.0
                };
                let cx = x + sign * (th / 2.0 + 4.0);
                let b = Boxed::centered([cx, y], th, len);
                let cost = plan.obstacles.cost(b);
                if best.is_none_or(|(bc, _, _)| cost < bc) {
                    best = Some((cost, b, cx));
                }
                if cost == 0.0 {
                    break 'find;
                }
            }
        }
        let _ = &mut done;
        if let Some((cost, b, cx)) = best {
            if cost.is_finite() {
                scene.add(SceneElement::Text {
                    text: caption,
                    pos: [cx, (b.t + b.b) / 2.0],
                    font_size: font,
                    color: ink.certified,
                    align: TextAlign::Center,
                    baseline: TextBaseline::Middle,
                    angle_deg: -90.0,
                    bold: false,
                });
                plan.obstacles.take(b);
            }
        }
        done.push(key);
    }
}

/// Where each state label stands: beside its point, clear of every other
/// label, marker and drawn line, inside the frame. Candidates run round the
/// point at growing distance; the first clear one is taken, the least
/// crowded otherwise.
fn draw_state_labels(
    scene: &mut Scene,
    points: &[(Point2D, String, bool)],
    ink: &Ink,
    geom: &Geom,
    plan: &mut LabelPlan,
) {
    let font = geom.font(11.0);
    let th = text_height(font);
    // The markers themselves are obstacles.
    for (p, _, _) in points {
        plan.obstacles.take(Boxed::centered(*p, 12.0, 12.0));
    }
    const DIRECTIONS_DEG: [f64; 8] = [0.0, 180.0, 45.0, 135.0, 315.0, 225.0, 90.0, 270.0];
    for (p, label, failed) in points {
        let w = text_width(label, font) + 3.0;
        let mut best: Option<(f64, Boxed, TextAlign, Point2D)> = None;
        'search: for distance in [11.0, 17.0, 26.0, 38.0] {
            for deg in DIRECTIONS_DEG {
                let a = f64::to_radians(deg);
                let (ux, uy) = (a.cos(), -a.sin());
                let d = distance + if *failed { 6.0 } else { 0.0 };
                let anchor = [p[0] + ux * d, p[1] + uy * d];
                let (align, cx) = if ux > 0.3 {
                    (TextAlign::Left, anchor[0] + w / 2.0)
                } else if ux < -0.3 {
                    (TextAlign::Right, anchor[0] - w / 2.0)
                } else {
                    (TextAlign::Center, anchor[0])
                };
                let cy = anchor[1]
                    + if ux.abs() <= 0.3 {
                        uy * th / 2.0
                    } else {
                        uy * th * 0.25
                    };
                let b = Boxed::centered([cx, cy], w, th);
                let cost = plan.obstacles.cost(b);
                if best.is_none_or(|(bc, _, _, _)| cost < bc) {
                    best = Some((cost, b, align, anchor));
                }
                if cost == 0.0 {
                    break 'search;
                }
            }
        }
        let Some((_, b, align, anchor)) = best else {
            continue;
        };
        let x = match align {
            TextAlign::Left => anchor[0],
            TextAlign::Right => anchor[0],
            _ => (b.l + b.r) / 2.0,
        };
        text(
            scene,
            label.clone(),
            [x, (b.t + b.b) / 2.0],
            font,
            ink.state,
            align,
            true,
        );
        plan.obstacles.take(b);
    }
}

/// Render the balance chart and its side panel on the report's canvas.
pub fn figure_load_trim_sheet(data: &LoadTrimSheetData, pal: &Palette) -> Scene {
    let geom = Geom::standard();
    draw_sheet(data, pal, geom)
}

/// The sheet drawn for a desktop card `width` points wide: scene units are
/// points, so the text is legible at any card width; a narrow card stacks the
/// panel below the chart.
pub fn figure_load_trim_sheet_sized(data: &LoadTrimSheetData, pal: &Palette, width: f64) -> Scene {
    let probe = Geom::sized(width, 0.0);
    let height = panel_height(data, &probe);
    draw_sheet(data, pal, Geom::sized(width, height))
}

fn draw_sheet(data: &LoadTrimSheetData, pal: &Palette, geom: Geom) -> Scene {
    let ink = Ink::for_palette(pal);
    let fr = frame_in(data, geom.chart);
    let envelopes = data.envelopes();
    let mut scene = Scene::new(
        geom.width,
        geom.height
            .max(geom.panel.1 + panel_height(data, &geom) + 14.0),
        Some(ink.paper),
    );

    draw_axes(&mut scene, &fr, &ink, &geom);
    draw_envelopes(&mut scene, &envelopes, &fr, &ink);
    let above_mlw = draw_above_mlw(&mut scene, data, &fr, &ink);
    let certified = draw_certified(&mut scene, data, &fr, &ink);
    let fuel = draw_fuel(&mut scene, data, &fr, &ink);
    draw_registered_mtow(&mut scene, data, &envelopes, &fr, &ink, &geom);

    let step_px: Vec<Point2D> = data
        .steps
        .iter()
        .map(|s| fr.map(s.pct_mac, s.mass_kg))
        .collect();

    // Everything drawn so far is an obstacle for the labels.
    let mut segments: Vec<(Point2D, Point2D)> = Vec::new();
    for path in certified.iter().chain([&fuel, &above_mlw]) {
        segments.extend(path.windows(2).map(|w| (w[0], w[1])));
    }
    for envelope in &envelopes {
        let outline = pixels(&fr, envelope);
        segments.extend(outline.windows(2).map(|w| (w[0], w[1])));
    }
    let frame_box = Boxed {
        l: fr.left + 2.0,
        t: fr.top + 2.0,
        r: fr.right() - 2.0,
        b: fr.bottom() - 2.0,
    };
    let mut plan = LabelPlan {
        obstacles: Clearance {
            frame: frame_box,
            boxes: Vec::new(),
            segments,
        },
    };
    draw_line_labels(&mut scene, data, &envelopes, &fr, &ink, &geom, &mut plan);

    // States: the +hold step is a small hollow marker, the four states are
    // labelled.
    let mut labelled: Vec<(Point2D, String, bool)> = Vec::new();
    for (step, p) in data.steps.iter().zip(&step_px) {
        if step.state != "+hold" {
            marker(&mut scene, &ink, *p, gate_failed(step));
            labelled.push((
                *p,
                format!("{} {:.1}", step.state, step.pct_mac),
                gate_failed(step),
            ));
        }
    }
    draw_state_labels(&mut scene, &labelled, &ink, &geom, &mut plan);

    scene.add(SceneElement::Rect {
        x: fr.left,
        y: fr.top,
        width: fr.width,
        height: fr.height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(ink.text, 1.2)),
    });
    text(
        &mut scene,
        data.title.clone(),
        [fr.left, 20.0],
        geom.font(16.0),
        ink.text,
        TextAlign::Left,
        true,
    );
    text(
        &mut scene,
        {
            let basis = super::panel::basis_line(data);
            if basis.is_empty() {
                format!("MAC {:.3} m   LEMAC {:.2} m", data.mac_m, data.x_lemac_m)
            } else {
                format!(
                    "MAC {:.3} m   LEMAC {:.2} m   |   {basis}",
                    data.mac_m, data.x_lemac_m
                )
            }
        },
        [fr.left, 42.0],
        geom.font(10.5),
        ink.muted,
        TextAlign::Left,
        false,
    );
    draw_panel(&mut scene, data, &ink, &geom);
    super::plot_data::record_sheet(&mut scene, data);
    scene
}
