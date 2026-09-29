// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Side panel of the load-and-trim sheet: limit key, worked loading case
//! as an additive index table, and method notes.

use super::render::{tag, text, Ink};
use super::{LoadTrimSheetData, SHEET_W};
use crate::scene::{Fill, Scene, SceneElement, Stroke, TextAlign};

const ROW: f64 = 17.0;

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

fn boxed(scene: &mut Scene, ink: &Ink, x: f64, y: f64, w: f64, h: f64, title: &str) {
    scene.add(SceneElement::Rect {
        x,
        y,
        width: w,
        height: h,
        rx: 3.0,
        fill: Some(Fill::new(ink.paper)),
        stroke: Some(Stroke::new(ink.text, 1.0)),
    });
    text(
        scene,
        title,
        [x + 10.0, y + 13.0],
        11.5,
        ink.text,
        TextAlign::Left,
        true,
    );
}

/// Word-wrap `s` to roughly `chars` characters per line.
fn wrap(s: &str, chars: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in s.split_whitespace() {
        let current = lines.last_mut().map_or(0, |l| l.len());
        if current > 0 && current + 1 + word.len() > chars {
            lines.push(String::new());
        }
        if let Some(last) = lines.last_mut() {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }
    lines
}

/// Draw the panel with its top-left corner at (`x`, `y`).
pub(super) fn draw_panel(scene: &mut Scene, data: &LoadTrimSheetData, ink: &Ink, x: f64, y: f64) {
    let w = SHEET_W - x - 16.0;
    let mut y = y;

    // Limit key.
    let key: [(&str, String); 3] = [
        (
            "A",
            format!(
                "Forward limit, takeoff/landing and zero fuel ({})",
                data.governance[0]
            ),
        ),
        (
            "B",
            format!("Aft limit, takeoff/landing ({})", data.governance[1]),
        ),
        (
            "C",
            "Aft limit, zero fuel: fuelling from any ZFW aft of C would cross B".to_owned(),
        ),
    ];
    let mut lines = Vec::new();
    for (letter, label) in &key {
        for (k, l) in wrap(label, 50).into_iter().enumerate() {
            lines.push((if k == 0 { Some(*letter) } else { None }, l));
        }
    }
    let key_h = 30.0 + ROW * (lines.len() as f64 + 2.4);
    boxed(scene, ink, x, y, w, key_h, "LIMIT DEFINITIONS");
    let mut ly = y + 34.0;
    for (letter, l) in &lines {
        if let Some(letter) = letter {
            tag(scene, [x + 20.0, ly], letter, ink.paper, ink.text, ink.text);
        }
        text(
            scene,
            l.clone(),
            [x + 36.0, ly],
            10.5,
            ink.text,
            TextAlign::Left,
            false,
        );
        ly += ROW;
    }
    scene.add(SceneElement::Rect {
        x: x + 12.0,
        y: ly - 7.0,
        width: 16.0,
        height: 12.0,
        rx: 0.0,
        fill: Some(Fill::new(ink.shade)),
        stroke: Some(Stroke::new(ink.text, 0.6)),
    });
    text(
        scene,
        "Do not operate (outside limits)",
        [x + 36.0, ly],
        10.5,
        ink.text,
        TextAlign::Left,
        false,
    );
    ly += ROW;
    let mut env = ink.envelope;
    env.a = 60;
    scene.add(SceneElement::Rect {
        x: x + 12.0,
        y: ly - 7.0,
        width: 16.0,
        height: 12.0,
        rx: 0.0,
        fill: Some(Fill::new(env)),
        stroke: Some(Stroke::dashed(ink.envelope, 1.0, 3.0, 2.0)),
    });
    text(
        scene,
        "Loading envelope: every boarding/hold order",
        [x + 36.0, ly],
        10.5,
        ink.text,
        TextAlign::Left,
        false,
    );
    y += key_h + 14.0;

    // Worked case as an additive index table.
    let rows = data.steps.len();
    let table_h = 40.0 + ROW * (rows as f64 + 1.0) + 64.0;
    boxed(scene, ink, x, y, w, table_h, "WORKED LOADING CASE");
    // Left edge of "#"/"Item", right edges of the numeric columns.
    let (c_num, c_item) = (x + 20.0, x + 36.0);
    let right = [x + 168.0, x + 212.0, x + 276.0, x + 316.0, x + w - 10.0];
    let hy = y + 34.0;
    text(
        scene,
        "#",
        [c_num, hy],
        10.0,
        ink.muted,
        TextAlign::Center,
        true,
    );
    text(
        scene,
        "Item",
        [c_item, hy],
        10.0,
        ink.muted,
        TextAlign::Left,
        true,
    );
    for (rx, h) in right.iter().zip(["dW kg", "dI", "W kg", "I", "%MAC"]) {
        text(scene, h, [*rx, hy], 10.0, ink.muted, TextAlign::Right, true);
    }
    let mut prev: Option<(f64, f64)> = None;
    let mut ry = hy + ROW;
    for (k, s) in data.steps.iter().enumerate() {
        let i = data.index_at(s.mass_kg, s.pct_mac);
        let fill = if s.item.to_ascii_lowercase().contains("fuel") {
            ink.fuel
        } else {
            ink.path
        };
        let (dw, di) = prev.map_or((String::new(), String::new()), |(pm, pi)| {
            (format!("{:+.0}", s.mass_kg - pm), format!("{:+.1}", i - pi))
        });
        tag(
            scene,
            [c_num, ry],
            &format!("{}", k + 1),
            fill,
            ink.paper,
            ink.paper,
        );
        text(
            scene,
            format!("{} ({})", s.item, s.state),
            [c_item, ry],
            10.0,
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
        for (rx, cell) in right.iter().zip(cells) {
            text(
                scene,
                cell,
                [*rx, ry],
                10.0,
                ink.text,
                TextAlign::Right,
                false,
            );
        }
        prev = Some((s.mass_kg, i));
        ry += ROW;
    }
    let explain = "Each item adds dI = dW (x_item - x_ref) / C whatever the weight, so the index column is a running sum; the CG is read on the slanted %MAC line through each point. Fuel follows the dashed curve (tank order), burn retraces it.";
    let mut ey = ry + 6.0;
    for l in wrap(explain, 60) {
        text(
            scene,
            l,
            [x + 12.0, ey],
            9.5,
            ink.muted,
            TextAlign::Left,
            false,
        );
        ey += 13.0;
    }
    y += table_h + 14.0;

    // Notes.
    let definition = format!(
        "Index I = W (x - x_ref)/C + K: x_ref = 25 %MAC = {:.3} m aft of nose, C = {} kg m, K = {:.0}; MAC {:.3} m, LEMAC {:.3} m.",
        data.index.x_ref_m,
        kg(data.index.c_kg_m),
        data.index.k,
        data.mac_m,
        data.x_lemac_m
    );
    let mut ny = y + 4.0;
    for note in std::iter::once(definition).chain(data.notes.iter().cloned()) {
        for l in wrap(&note, 62) {
            if ny > super::SHEET_H - 12.0 {
                return;
            }
            text(scene, l, [x, ny], 9.5, ink.muted, TextAlign::Left, false);
            ny += 13.0;
        }
        ny += 3.0;
    }
}
