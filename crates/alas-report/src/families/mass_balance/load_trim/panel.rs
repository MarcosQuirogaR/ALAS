// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Side panel of the load-and-trim sheet: a compact chart key, the weight and
//! balance table of the worked case with the run's CG-gate verdict, and the
//! limit mechanism that sets each phase's forward and aft limit.
//!
//! Every fixed text is its own scene text so the desktop catalog can
//! translate it after the sheet is drawn; only numbers are formatted in.

use super::gate::{table_notes, NOTE_ABOVE_MLW, TABLE_NOTE};
use super::geom::{text_height, text_width, Geom};
use super::numbers::{kg, signed_tenths};
use super::render::{marker, text, Ink};
use super::{EnvelopeKind, LoadStep, LoadTrimSheetData};
use crate::scene::{Fill, Scene, SceneElement, Stroke, TextAlign};

const TITLE_KEY: &str = "Chart key";
const TITLE_TABLE: &str = "Weight and balance";
const TITLE_LIMITS: &str = "Governing limits [%MAC]";
/// Key text of the fuel curve.
const KEY_FUEL: &str = "Fuel loading, ZFW to TOW (tank sequence)";
/// The fuel line of a sheet that runs it on to the maximum-fuel takeoff.
const KEY_FUEL_MAX: &str = "Fuel loading, ZFW to max-fuel TOW (tank sequence)";
/// Key text of the loading states.
const KEY_STATE: &str = "Loading state";
/// Key text of the red band.
pub(super) const KEY_CROSSED: &str = "No feasible CG range (forward limit aft of aft limit)";
/// Key text of the ring around a state the run's CG gate failed.
pub(super) const KEY_OUTSIDE: &str = "State failing the CG gate";
/// Key prefix of a published comparison envelope, followed by its source.
pub(super) const CERTIFIED: &str = "Published";
/// Key lead of the en-route-above-MLW outline, followed by the label of the
/// mechanism the gate found bounding it forward.
pub(super) const KEY_ABOVE_MLW: &str = "En route above MLW, fwd limit:";
/// Label of the forward and aft limit lines.
pub(super) const FWD_LIMIT: &str = "Fwd limit";
/// See [`FWD_LIMIT`].
pub(super) const AFT_LIMIT: &str = "Aft limit";
/// Headers of the weight and balance table.
const TABLE_HEADERS: [&str; 5] = ["W [kg]", "CG [%MAC]", "Fwd", "Aft", "Margin"];
/// Header of the state column.
const STATE_HEADER: &str = "State";
/// The takeoff and landing rows of a sheet that also carries the sizing
/// loading's maximum-fuel takeoff: they are the flown route's.
const TABLE_TOW_FLOWN: &str = "TOW (flown route)";
const TABLE_LW_FLOWN: &str = "LW (flown route)";

/// Axis titles and the sheet name, shared with the chart and the data title.
pub(super) const AXIS_CG: &str = "CG position [%MAC]";
pub(super) const AXIS_WEIGHT: &str = "Airplane gross weight [kg]";
pub(super) const SHEET_NAME: &str = "Load & trim sheet (ALAS model)";

/// Phase names of the governing-limit table.
const PHASE_TAKEOFF: &str = "Takeoff";
const PHASE_LANDING: &str = "Landing";
const PHASE_ZERO_FUEL: &str = "Zero fuel";

/// Every fixed string the sheet draws, for catalog coverage.
pub const SHEET_TEXT: &[&str] = &[
    AXIS_CG,
    AXIS_WEIGHT,
    SHEET_NAME,
    TITLE_KEY,
    TITLE_TABLE,
    TITLE_LIMITS,
    "Takeoff envelope",
    "Landing envelope",
    "Zero-fuel envelope",
    FWD_LIMIT,
    AFT_LIMIT,
    CERTIFIED,
    KEY_ABOVE_MLW,
    KEY_FUEL,
    KEY_FUEL_MAX,
    KEY_STATE,
    KEY_CROSSED,
    KEY_OUTSIDE,
    PHASE_TAKEOFF,
    PHASE_LANDING,
    PHASE_ZERO_FUEL,
    "nose-gear load",
    "rotation",
    "landing trim",
    "static margin",
    "min nose load",
    "tip-back",
    STATE_HEADER,
    TABLE_TOW_FLOWN,
    super::data::MAX_FUEL_STATE,
    TABLE_LW_FLOWN,
    TABLE_NOTE,
    NOTE_ABOVE_MLW,
    TABLE_HEADERS[0],
    TABLE_HEADERS[1],
    TABLE_HEADERS[2],
    TABLE_HEADERS[3],
    TABLE_HEADERS[4],
];

/// Whether the run's CG gate failed the state of `step`.
pub(super) fn gate_failed(step: &LoadStep) -> bool {
    step.gate.as_ref().is_some_and(|gate| gate.violated)
}

/// One row of the chart key.
enum KeyRow {
    Envelope(EnvelopeKind),
    Stroke(Stroke, &'static str),
    /// A line sample, a translatable lead and a text after it.
    Led(Stroke, &'static str, String),
    State,
    Crossed,
    Ring,
}

/// The key rows of `data`: each drawn envelope, the loading curves and fuel
/// curve when drawn, each certified source, and the ring and red band when
/// the state or an envelope failed.
fn key_rows(data: &LoadTrimSheetData, ink: &Ink) -> Vec<KeyRow> {
    let mut rows: Vec<KeyRow> = data
        .envelopes()
        .iter()
        .map(|e| KeyRow::Envelope(e.kind))
        .collect();
    if let Some(set) = &data.en_route_above_mlw {
        let bound = set.forward_bound.label().to_owned();
        rows.push(KeyRow::Led(ink.above_mlw_stroke(), KEY_ABOVE_MLW, bound));
    }
    if data
        .envelopes()
        .iter()
        .any(|e| !e.crossed_bands().is_empty())
    {
        rows.push(KeyRow::Crossed);
    }
    if data.fuel_curve.len() >= 2 {
        let label = if data.steps.iter().any(|s| s.state == super::data::MAX_FUEL_STATE) {
            KEY_FUEL_MAX
        } else {
            KEY_FUEL
        };
        rows.push(KeyRow::Stroke(Stroke::new(ink.fuel, 1.4), label));
    }
    for source in certified_sources(data) {
        let stroke = Stroke::dashed(ink.certified, 1.5, 6.0, 3.0);
        rows.push(KeyRow::Led(stroke, CERTIFIED, format!("({source})")));
    }
    rows.push(KeyRow::State);
    if data.steps.iter().any(gate_failed) {
        rows.push(KeyRow::Ring);
    }
    rows
}

/// The source of each certified comparison line, each once.
pub(super) fn certified_sources(data: &LoadTrimSheetData) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for line in &data.lines.published {
        if !sources.contains(&line.source) {
            sources.push(line.source.clone());
        }
    }
    sources
}

/// Text size of the panel, pt.
fn panel_font(geom: &Geom) -> f64 {
    geom.font(10.5)
}

/// Row pitch of the panel, px.
fn row_pitch(font_pt: f64) -> f64 {
    text_height(font_pt) * 1.32
}

const INSET: f64 = 12.0;
const TITLE_ROWS: f64 = 1.5;
const BOTTOM_PAD_ROWS: f64 = 0.4;
const BOX_GAP: f64 = 14.0;

/// The state name the table prints. When the sheet also carries the
/// maximum-fuel takeoff of the sizing loading, the takeoff and landing rows
/// are the flown route's and say so.
fn table_label(data: &LoadTrimSheetData, step: &LoadStep) -> String {
    let two_bases = data.steps.iter().any(|s| s.state == super::data::MAX_FUEL_STATE);
    match step.state.as_str() {
        "TOW" if two_bases => TABLE_TOW_FLOWN.to_owned(),
        "LW" if two_bases => TABLE_LW_FLOWN.to_owned(),
        _ => step.state.clone(),
    }
}

/// The mass basis of the sheet, one line: the flown route's takeoff and fuel
/// and, when the sheet carries it, the MTOW sizing basis beside it.
pub(super) fn basis_line(data: &LoadTrimSheetData) -> String {
    let mass = |state: &str| data.steps.iter().find(|s| s.state == state).map(|s| s.mass_kg);
    let (Some(tow), Some(zfw)) = (mass("TOW"), mass("ZFW")) else {
        return String::new();
    };
    let t = |kg: f64| format!("{:.1} t", kg / 1000.0);
    match mass(super::data::MAX_FUEL_STATE) {
        Some(max) => format!(
            "Flown route: TOW {}, fuel {}   |   MTOW sizing basis: {}, fuel {}",
            t(tow),
            t(tow - zfw),
            t(max),
            t(max - zfw)
        ),
        None => format!("TOW {}, fuel {}", t(tow), t(tow - zfw)),
    }
}

/// The states the table lists: those the gate evaluates.
fn table_steps(data: &LoadTrimSheetData) -> Vec<&LoadStep> {
    data.steps.iter().filter(|s| s.gate.is_some()).collect()
}

/// The phases of the governing-limit table with the state whose mass they
/// are read at.
fn limit_phases(data: &LoadTrimSheetData) -> Vec<(EnvelopeKind, &'static str, f64)> {
    let mass = |state: &str| {
        data.steps
            .iter()
            .find(|s| s.state == state)
            .map(|s| s.mass_kg)
    };
    [
        (EnvelopeKind::Takeoff, PHASE_TAKEOFF, "TOW"),
        (EnvelopeKind::Landing, PHASE_LANDING, "LW"),
        (EnvelopeKind::ZeroFuel, PHASE_ZERO_FUEL, "ZFW"),
    ]
    .into_iter()
    .filter(|_| !data.lines.mechanisms.is_empty())
    .filter_map(|(kind, name, state)| Some((kind, name, mass(state)?)))
    .collect()
}

/// Rows (in row pitches) of the three boxes.
fn box_rows(data: &LoadTrimSheetData, ink: &Ink) -> [f64; 3] {
    let key = key_rows(data, ink).len() as f64;
    let table = 1.0 + (table_steps(data).len() + table_notes(&data.steps).len()) as f64;
    let limits = 2.0 * limit_phases(data).len() as f64 + 0.2;
    [key, table, limits]
}

/// Total height of the panel for `data` at the geometry's font, px.
pub(super) fn panel_height(data: &LoadTrimSheetData, geom: &Geom) -> f64 {
    let pitch = row_pitch(panel_font(geom));
    let ink = Ink::for_palette(&crate::theme::PALETTE_LIGHT);
    let rows = box_rows(data, &ink);
    let boxes = rows.iter().filter(|r| **r > 0.3).count() as f64;
    rows.iter()
        .filter(|r| **r > 0.3)
        .map(|r| (r + TITLE_ROWS + BOTTOM_PAD_ROWS) * pitch)
        .sum::<f64>()
        + (boxes - 1.0).max(0.0) * BOX_GAP
}

fn boxed(
    scene: &mut Scene,
    ink: &Ink,
    (x, y, width, height): (f64, f64, f64, f64),
    title: &str,
    font: f64,
) {
    scene.add(SceneElement::Rect {
        x,
        y,
        width,
        height,
        rx: 4.0,
        fill: Some(Fill::new(ink.panel)),
        stroke: Some(Stroke::new(ink.grid, 1.0)),
    });
    text(
        scene,
        title,
        [x + INSET, y + 0.85 * row_pitch(font)],
        font + 0.8,
        ink.text,
        TextAlign::Left,
        true,
    );
}

/// Draw the panel at the geometry's panel origin.
pub(super) fn draw_panel(scene: &mut Scene, data: &LoadTrimSheetData, ink: &Ink, geom: &Geom) {
    let font = panel_font(geom);
    let pitch = row_pitch(font);
    let (x, mut y, width) = geom.panel;
    let rows = box_rows(data, ink);
    let height = |rows: f64| (rows + TITLE_ROWS + BOTTOM_PAD_ROWS) * pitch;

    // Key.
    let key = key_rows(data, ink);
    boxed(scene, ink, (x, y, width, height(rows[0])), TITLE_KEY, font);
    let sample_x = (x + INSET, x + INSET + 34.0);
    let label_x = sample_x.1 + 9.0;
    let room = x + width - INSET - label_x;
    for (k, row) in key.into_iter().enumerate() {
        let ry = y + (TITLE_ROWS + k as f64 + 0.35) * pitch;
        let mid = 0.5 * (sample_x.0 + sample_x.1);
        let label: String = match row {
            KeyRow::Envelope(kind) => {
                let (fill, edge) = ink.envelope(kind);
                scene.add(SceneElement::Rect {
                    x: sample_x.0,
                    y: ry - 5.0,
                    width: sample_x.1 - sample_x.0,
                    height: 10.0,
                    rx: 1.5,
                    fill: (kind == EnvelopeKind::Takeoff).then(|| Fill::new(fill)),
                    stroke: Some(if kind == EnvelopeKind::ZeroFuel {
                        Stroke::dashed(edge, 1.4, 5.0, 2.5)
                    } else {
                        Stroke::new(edge, 1.4)
                    }),
                });
                kind.key().to_owned()
            }
            KeyRow::Stroke(stroke, label) => {
                scene.add(SceneElement::Line {
                    p1: [sample_x.0, ry],
                    p2: [sample_x.1, ry],
                    stroke,
                });
                label.to_owned()
            }
            KeyRow::Led(stroke, lead, rest) => {
                scene.add(SceneElement::Line {
                    p1: [sample_x.0, ry],
                    p2: [sample_x.1, ry],
                    stroke,
                });
                // The lead is translatable; the rest (a source, a mechanism)
                // follows as its own text.
                let size = font;
                text(
                    scene,
                    lead,
                    [label_x, ry],
                    size,
                    ink.text,
                    TextAlign::Left,
                    false,
                );
                let offset = text_width(lead, size) + 6.0;
                let fit = ((room - offset) / text_width(&rest, size)).min(1.0);
                text(
                    scene,
                    rest,
                    [label_x + offset, ry],
                    (size * fit).max(size.min(9.5)),
                    ink.muted,
                    TextAlign::Left,
                    false,
                );
                continue;
            }
            KeyRow::State => {
                marker(scene, ink, [mid, ry], false);
                KEY_STATE.to_owned()
            }
            KeyRow::Crossed => {
                scene.add(SceneElement::Rect {
                    x: sample_x.0,
                    y: ry - 5.0,
                    width: sample_x.1 - sample_x.0,
                    height: 10.0,
                    rx: 0.0,
                    fill: Some(Fill::new(ink.crossed())),
                    stroke: Some(Stroke::new(ink.alert, 0.6)),
                });
                KEY_CROSSED.to_owned()
            }
            KeyRow::Ring => {
                marker(scene, ink, [mid, ry], true);
                KEY_OUTSIDE.to_owned()
            }
        };
        let fit = (room / text_width(&label, font)).min(1.0);
        text(
            scene,
            label,
            [label_x, ry],
            (font * fit).max(font.min(9.5)),
            ink.text,
            TextAlign::Left,
            false,
        );
    }
    y += height(rows[0]) + BOX_GAP;

    // Weight and balance table.
    let steps = table_steps(data);
    boxed(
        scene,
        ink,
        (x, y, width, height(rows[1])),
        TITLE_TABLE,
        font,
    );
    let inner_w = width - 2.0 * INSET;
    // Right edges of the five numeric columns as shares of the inner width.
    let shares = [0.44, 0.62, 0.755, 0.875, 1.0];
    let right: Vec<f64> = shares.iter().map(|s| x + INSET + s * inner_w).collect();
    let hy = y + (TITLE_ROWS + 0.35) * pitch;
    text(
        scene,
        STATE_HEADER,
        [x + INSET, hy],
        font,
        ink.muted,
        TextAlign::Left,
        true,
    );
    for (rx, h) in right.iter().zip(TABLE_HEADERS) {
        text(scene, h, [*rx, hy], font, ink.muted, TextAlign::Right, true);
    }
    scene.add(SceneElement::Line {
        p1: [x + INSET, hy + 0.55 * pitch],
        p2: [x + width - INSET, hy + 0.55 * pitch],
        stroke: Stroke::new(ink.grid, 0.8),
    });
    for (k, step) in steps.iter().enumerate() {
        let ry = hy + (k as f64 + 1.0) * pitch;
        let failed = gate_failed(step);
        marker(scene, ink, [x + INSET + 4.0, ry], failed);
        text(
            scene,
            table_label(data, step),
            [x + INSET + 16.0, ry],
            font,
            ink.text,
            TextAlign::Left,
            true,
        );
        let gate = step.gate.as_ref();
        let cells = [
            kg(step.mass_kg),
            format!("{:.1}", step.pct_mac),
            gate.map_or_else(String::new, |g| format!("{:.1}", g.fwd_pct_mac)),
            gate.map_or_else(String::new, |g| format!("{:.1}", g.aft_pct_mac)),
            gate.map_or_else(String::new, |g| signed_tenths(g.margin_pct_mac())),
        ];
        for (index, (rx, cell)) in right.iter().zip(cells).enumerate() {
            let alert = index == 4 && failed;
            text(
                scene,
                cell,
                [*rx, ry],
                font,
                if alert { ink.alert } else { ink.text },
                TextAlign::Right,
                alert,
            );
        }
    }
    for (k, (lead, bound)) in table_notes(&data.steps).into_iter().enumerate() {
        let ny = hy + (steps.len() + k + 1) as f64 * pitch;
        let at = |dx: f64| [x + INSET + dx, ny];
        text(
            scene,
            lead,
            at(0.0),
            font,
            ink.muted,
            TextAlign::Left,
            false,
        );
        if let Some(bound) = bound {
            let dx = text_width(lead, font) + 4.0;
            text(scene, bound, at(dx), font, ink.text, TextAlign::Left, true);
        }
    }
    y += height(rows[1]) + BOX_GAP;

    // Governing limits.
    let phases = limit_phases(data);
    if !phases.is_empty() {
        boxed(
            scene,
            ink,
            (x, y, width, height(rows[2])),
            TITLE_LIMITS,
            font,
        );
        let cols = (
            x + INSET,
            x + INSET + 0.30 * inner_w,
            x + INSET + 0.66 * inner_w,
        );
        for (k, (kind, name, mass)) in phases.into_iter().enumerate() {
            let ry = y + (TITLE_ROWS + 0.35 + 2.0 * k as f64) * pitch;
            text(
                scene,
                name,
                [cols.0, ry],
                font,
                ink.text,
                TextAlign::Left,
                true,
            );
            for (col, forward, caption) in [(cols.1, true, FWD_LIMIT), (cols.2, false, AFT_LIMIT)] {
                let value = data.envelope_limit_at(kind, mass, forward);
                text(
                    scene,
                    caption,
                    [col, ry],
                    font,
                    ink.muted,
                    TextAlign::Left,
                    false,
                );
                let cap_w = text_width(caption, font) + 6.0;
                text(
                    scene,
                    format!("{value:.1}"),
                    [col + cap_w, ry],
                    font,
                    ink.text,
                    TextAlign::Left,
                    true,
                );
                if let Some(mechanism) = data.governing_mechanism(kind, mass, forward) {
                    text(
                        scene,
                        mechanism.label(),
                        [col, ry + 0.95 * pitch],
                        font,
                        ink.muted,
                        TextAlign::Left,
                        false,
                    );
                }
            }
        }
    }
}
