// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Side panel of the load-and-trim sheet: limit key, worked loading case as
//! an additive index table, and the run's CG-gate verdict for each state.
//!
//! The three boxes fill the chart's height exactly: the row pitch is solved
//! from the number of rows, so the column never ends in an empty band below
//! the tables whatever the number of loading states.

use super::layers::{limit_style, LimitKind};
use super::render::{tag, text, Ink};
use super::{GatePhase, LoadStep, LoadTrimSheetData};
use crate::scene::{Fill, Scene, SceneElement, Stroke, TextAlign, CSS_PIXELS_PER_POINT};

/// Key text of each limit set. Static strings so the desktop catalog can
/// translate them.
pub(super) const LIMIT_KEY: [(LimitKind, &str); 4] = [
    (LimitKind::Ground, "Ground limits"),
    (LimitKind::Takeoff, "Takeoff limits"),
    (LimitKind::Flight, "Flight limits"),
    (LimitKind::Landing, "Landing limits"),
];
/// Key text of the shaded field.
pub(super) const KEY_SHADE: &str = "Outside ground limits";
/// Key text of the ring around a state the run's CG gate failed.
pub(super) const KEY_OUTSIDE: &str = "State failing the CG gate";
/// Limit-column text of a loading step the CG gate does not evaluate.
const NOT_GATED: &str = "not gated";
const TITLE_KEY: &str = "LIMIT DEFINITIONS";
const TITLE_POINTS: &str = "LOADING POINTS";
const TITLE_CHECK: &str = "CG GATE (%MAC)";
const POINT_HEADERS: [&str; 5] = ["dW kg", "dI", "W kg", "I", "%MAC"];
const CHECK_HEADERS: [&str; 4] = ["Limit", "Fwd", "Aft", "Margin"];

/// Axis titles and the sheet name, shared with the chart and the data title.
pub(super) const AXIS_INDEX: &str = "INDEX";
pub(super) const AXIS_WEIGHT: &str = "AIRPLANE GROSS WEIGHT - KILOGRAMS";
pub(super) const SHEET_NAME: &str = "LOAD & TRIM SHEET (ALAS model)";

/// Every fixed string the sheet draws, for catalog coverage.
pub const SHEET_TEXT: &[&str] = &[
    AXIS_INDEX,
    AXIS_WEIGHT,
    SHEET_NAME,
    TITLE_KEY,
    TITLE_POINTS,
    TITLE_CHECK,
    "Ground limits",
    "Takeoff limits",
    "Flight limits",
    "Landing limits",
    KEY_SHADE,
    KEY_OUTSIDE,
    NOT_GATED,
    "State",
    "dW kg",
    "W kg",
    "Limit",
    "Fwd",
    "Aft",
    "Margin",
];

/// Rows above and below a box's table: the title band and the bottom pad.
const TITLE_ROWS: f64 = 1.6;
const PAD_ROWS: f64 = 0.5;
/// The row pitch is kept legible and not stretched past readable spacing;
/// the remainder of the column goes to the gaps between boxes.
const ROW_MIN: f64 = 16.0;
const ROW_MAX: f64 = 30.0;
const GAP_MIN: f64 = 12.0;
/// Column widths of the two tables in em: the state tag, the state name and
/// the numeric columns, each with its gap. Digits advance about 0.6 em.
const POINT_COLUMNS_EM: [f64; 7] = [1.8, 4.6, 4.4, 3.8, 5.0, 3.6, 3.8];
const CHECK_COLUMNS_EM: [f64; 6] = [1.8, 4.6, 3.4, 3.8, 3.8, 4.8];
const TABLE_INSET: f64 = 10.0;

/// Group thousands with a space: 78000 -> "78 000".
pub(super) fn kg(value: f64) -> String {
    let digits = format!("{:.0}", value.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    if value < 0.0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A signed value to one decimal, with no sign on a value that rounds to
/// zero ("-0.0" would read as outside a limit the state sits on).
pub(super) fn signed_tenths(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded == 0.0 {
        "0.0".to_owned()
    } else {
        format!("{rounded:+.1}")
    }
}

/// Geometry of the panel column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PanelLayout {
    pub(super) x: f64,
    pub(super) top: f64,
    pub(super) width: f64,
    /// Row pitch, px.
    pub(super) row: f64,
    /// Space between boxes, px.
    pub(super) gap: f64,
    /// Table text size, pt.
    pub(super) font: f64,
    /// Rows of each box: key, loading points, limit check.
    pub(super) rows: [usize; 3],
}

impl PanelLayout {
    /// Solve the row pitch that makes the three boxes span `top..bottom`.
    pub(super) fn solve(x: f64, top: f64, bottom: f64, width: f64, rows: [usize; 3]) -> Self {
        let fixed_rows = 3.0 * (TITLE_ROWS + PAD_ROWS) + rows.iter().sum::<usize>() as f64;
        let available = (bottom - top).max(0.0);
        let row = ((available - 2.0 * GAP_MIN) / fixed_rows).clamp(ROW_MIN, ROW_MAX);
        let gap = ((available - row * fixed_rows) / 2.0).max(GAP_MIN);
        let table_em = POINT_COLUMNS_EM
            .iter()
            .sum::<f64>()
            .max(CHECK_COLUMNS_EM.iter().sum::<f64>());
        let width_font = (width - 2.0 * TABLE_INSET) / (table_em * CSS_PIXELS_PER_POINT);
        let font = width_font.min(row / 2.0).clamp(9.0, 13.0);
        Self {
            x,
            top,
            width,
            row,
            gap,
            font,
            rows,
        }
    }

    fn box_height(&self, rows: usize) -> f64 {
        self.row * (TITLE_ROWS + PAD_ROWS + rows as f64)
    }

    /// Bottom edge of the last box.
    pub(super) fn bottom(&self) -> f64 {
        self.top + self.rows.iter().map(|&r| self.box_height(r)).sum::<f64>() + 2.0 * self.gap
    }

    /// Text size of box titles, pt.
    fn title_font(&self) -> f64 {
        self.font + 1.5
    }

    /// One em of table text, px.
    fn em(&self) -> f64 {
        self.font * CSS_PIXELS_PER_POINT
    }
}

/// Rows of each box for `data`: the key (four sets, the shaded field and,
/// when any state is outside its limits, the ring), and a header plus one
/// row per state in each table.
pub(super) fn panel_rows(data: &LoadTrimSheetData) -> [usize; 3] {
    let flagged = data.steps.iter().any(gate_failed);
    let states = data.steps.len();
    [5 + usize::from(flagged), states + 1, states + 1]
}

fn boxed(scene: &mut Scene, ink: &Ink, layout: &PanelLayout, y: f64, rows: usize, title: &str) {
    scene.add(SceneElement::Rect {
        x: layout.x,
        y,
        width: layout.width,
        height: layout.box_height(rows),
        rx: 3.0,
        fill: Some(Fill::new(ink.paper)),
        stroke: Some(Stroke::new(ink.text, 1.0)),
    });
    text(
        scene,
        title,
        [layout.x + TABLE_INSET, y + 0.85 * layout.row],
        layout.title_font(),
        ink.text,
        TextAlign::Left,
        true,
    );
}

/// Centre line of table row `index` (0 is the header) of a box at `y`.
fn row_y(layout: &PanelLayout, y: f64, index: usize) -> f64 {
    y + layout.row * (TITLE_ROWS + 0.5 + index as f64)
}

/// Right edges of a table's columns, px, from its em widths.
fn right_edges<const N: usize>(layout: &PanelLayout, columns: [f64; N]) -> [f64; N] {
    let mut edges = [0.0; N];
    let mut x = layout.x + TABLE_INSET;
    for (edge, width) in edges.iter_mut().zip(columns) {
        x += width * layout.em();
        *edge = x;
    }
    // The last column ends at the box's inner right edge.
    let shift = layout.x + layout.width - TABLE_INSET - edges[N - 1];
    for edge in edges.iter_mut().skip(2) {
        *edge += shift;
    }
    edges
}

fn state_fill(ink: &Ink, item: &str) -> crate::scene::Color {
    if item.to_ascii_lowercase().contains("fuel") {
        ink.fuel
    } else {
        ink.path
    }
}

fn state_tag(scene: &mut Scene, ink: &Ink, layout: &PanelLayout, y: f64, k: usize, item: &str) {
    tag(
        scene,
        [layout.x + TABLE_INSET + 0.9 * layout.em(), y],
        &format!("{}", k + 1),
        state_fill(ink, item),
        ink.paper,
        ink.paper,
    );
}

fn draw_key(scene: &mut Scene, ink: &Ink, layout: &PanelLayout, y: f64, flagged: bool) {
    boxed(scene, ink, layout, y, layout.rows[0], TITLE_KEY);
    let (x, font) = (layout.x, layout.font);
    let label_x = x + TABLE_INSET + 3.4 * layout.em();
    let mut ry = row_y(layout, y, 0);
    for (kind, label) in LIMIT_KEY {
        scene.add(SceneElement::Line {
            p1: [x + TABLE_INSET, ry],
            p2: [x + TABLE_INSET + 2.6 * layout.em(), ry],
            stroke: limit_style(kind, ink.text),
        });
        text(
            scene,
            label,
            [label_x, ry],
            font,
            ink.text,
            TextAlign::Left,
            false,
        );
        ry += layout.row;
    }
    scene.add(SceneElement::Rect {
        x: x + TABLE_INSET + 0.4 * layout.em(),
        y: ry - 0.35 * layout.em(),
        width: 1.8 * layout.em(),
        height: 0.7 * layout.em(),
        rx: 0.0,
        fill: Some(Fill::new(ink.shade)),
        stroke: Some(Stroke::new(ink.text, 0.6)),
    });
    text(
        scene,
        KEY_SHADE,
        [label_x, ry],
        font,
        ink.text,
        TextAlign::Left,
        false,
    );
    if flagged {
        ry += layout.row;
        alert_ring(scene, ink, [x + TABLE_INSET + 1.3 * layout.em(), ry]);
        text(
            scene,
            KEY_OUTSIDE,
            [label_x, ry],
            font,
            ink.text,
            TextAlign::Left,
            false,
        );
    }
}

/// The ring drawn around a state outside its limits, on the chart and in
/// the key.
pub(super) fn alert_ring(scene: &mut Scene, ink: &Ink, center: crate::scene::Point2D) {
    scene.add(SceneElement::Circle {
        center,
        radius: 13.0,
        fill: None,
        stroke: Some(Stroke::new(ink.alert, 2.6)),
    });
}

fn header(scene: &mut Scene, ink: &Ink, layout: &PanelLayout, y: f64, name_x: f64) {
    let font = layout.font;
    text(
        scene,
        "#",
        [layout.x + TABLE_INSET + 0.9 * layout.em(), y],
        font,
        ink.muted,
        TextAlign::Center,
        true,
    );
    text(
        scene,
        "State",
        [name_x, y],
        font,
        ink.muted,
        TextAlign::Left,
        true,
    );
}

fn draw_points(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    ink: &Ink,
    layout: &PanelLayout,
    y: f64,
) {
    boxed(scene, ink, layout, y, layout.rows[1], TITLE_POINTS);
    let right = right_edges(layout, POINT_COLUMNS_EM);
    let name_x = right[0] + 0.2 * layout.em();
    let hy = row_y(layout, y, 0);
    header(scene, ink, layout, hy, name_x);
    for (rx, h) in right[2..].iter().zip(POINT_HEADERS) {
        text(
            scene,
            h,
            [*rx, hy],
            layout.font,
            ink.muted,
            TextAlign::Right,
            true,
        );
    }
    let mut prev: Option<(f64, f64)> = None;
    for (k, s) in data.steps.iter().enumerate() {
        let ry = row_y(layout, y, k + 1);
        let i = data.index_at(s.mass_kg, s.pct_mac);
        let (dw, di) = prev.map_or((String::new(), String::new()), |(pm, pi)| {
            (format!("{:+.0}", s.mass_kg - pm), format!("{:+.1}", i - pi))
        });
        state_tag(scene, ink, layout, ry, k, &s.item);
        text(
            scene,
            s.state.clone(),
            [name_x, ry],
            layout.font,
            ink.text,
            TextAlign::Left,
            false,
        );
        let cells = [
            dw,
            di,
            kg(s.mass_kg),
            format!("{i:.1}"),
            format!("{:.1}", s.pct_mac),
        ];
        for (rx, cell) in right[2..].iter().zip(cells) {
            text(
                scene,
                cell,
                [*rx, ry],
                layout.font,
                ink.text,
                TextAlign::Right,
                false,
            );
        }
        prev = Some((s.mass_kg, i));
    }
}

/// The limit-set line style of a gate phase, as the chart draws that set.
fn phase_style(phase: GatePhase, ink: &Ink) -> Stroke {
    limit_style(
        match phase {
            GatePhase::Ground => LimitKind::Ground,
            GatePhase::Takeoff => LimitKind::Takeoff,
            GatePhase::Flight => LimitKind::Flight,
            GatePhase::Landing => LimitKind::Landing,
        },
        ink.text,
    )
}

/// Whether the run's CG gate failed the state of `step`.
pub(super) fn gate_failed(step: &LoadStep) -> bool {
    step.gate.as_ref().is_some_and(|gate| gate.violated)
}

fn draw_checks(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    ink: &Ink,
    layout: &PanelLayout,
    y: f64,
) {
    boxed(scene, ink, layout, y, layout.rows[2], TITLE_CHECK);
    let right = right_edges(layout, CHECK_COLUMNS_EM);
    let name_x = right[0] + 0.2 * layout.em();
    let hy = row_y(layout, y, 0);
    header(scene, ink, layout, hy, name_x);
    // The limit column holds the phase's line sample, centred under its header.
    let glyph = (right[1] + 0.4 * layout.em(), right[2] - 0.2 * layout.em());
    let font = layout.font;
    let head = |scene: &mut Scene, label: &str, x: f64, align| {
        text(scene, label, [x, hy], font, ink.muted, align, true);
    };
    head(
        scene,
        CHECK_HEADERS[0],
        0.5 * (glyph.0 + glyph.1),
        TextAlign::Center,
    );
    for (rx, h) in right[3..].iter().zip(&CHECK_HEADERS[1..]) {
        head(scene, h, *rx, TextAlign::Right);
    }
    for (k, s) in data.steps.iter().enumerate() {
        let ry = row_y(layout, y, k + 1);
        state_tag(scene, ink, layout, ry, k, &s.item);
        text(
            scene,
            s.state.clone(),
            [name_x, ry],
            font,
            ink.text,
            TextAlign::Left,
            false,
        );
        let Some(gate) = &s.gate else {
            // An intermediate loading step: the gate does not evaluate it,
            // so it has no verdict and no ring.
            text(
                scene,
                NOT_GATED,
                [glyph.0, ry],
                font,
                ink.muted,
                TextAlign::Left,
                false,
            );
            continue;
        };
        scene.add(SceneElement::Line {
            p1: [glyph.0, ry],
            p2: [glyph.1, ry],
            stroke: phase_style(gate.phase, ink),
        });
        let cells = [
            format!("{:.1}", gate.fwd_pct_mac),
            format!("{:.1}", gate.aft_pct_mac),
            signed_tenths(gate.margin_pct_mac()),
        ];
        for (index, (rx, cell)) in right[3..].iter().zip(cells).enumerate() {
            let alert = index == 2 && gate.violated;
            let color = if alert { ink.alert } else { ink.text };
            text(scene, cell, [*rx, ry], font, color, TextAlign::Right, alert);
        }
    }
}

/// Draw the panel in the column `layout` describes.
pub(super) fn draw_panel(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    ink: &Ink,
    layout: &PanelLayout,
) {
    let flagged = data.steps.iter().any(gate_failed);
    let mut y = layout.top;
    draw_key(scene, ink, layout, y, flagged);
    y += layout.box_height(layout.rows[0]) + layout.gap;
    draw_points(scene, data, ink, layout, y);
    y += layout.box_height(layout.rows[1]) + layout.gap;
    draw_checks(scene, data, ink, layout, y);
}
