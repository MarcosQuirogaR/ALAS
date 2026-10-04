// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The analyzed/design mass lines of the load-and-trim chart and their
//! labels.
//!
//! Each label sits on a paper backplate a fixed clearance away from its own
//! line, and is moved (to the other side, to the aft end, or further out)
//! until it crosses no other mass line, no earlier label, the %MAC row or the
//! frame, so the layout holds for any combination of masses.

use super::panel::kg;
use super::render::Ink;
use super::{limit_at, Frame, LoadTrimSheetData, MassRole};
use crate::scene::{Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::scene::{CONSERVATIVE_ADVANCE_EM, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM};

/// One analyzed/design mass line and the label it carries.
struct MassAnnotation {
    label: String,
    /// Forward and aft end of the line, px.
    a: Point2D,
    b: Point2D,
    /// Whether the label prefers the side below the line.
    below: bool,
    font_size: f64,
    /// Reserved text extent, px.
    width: f64,
    height: f64,
}

/// Clearance between a mass line and the label backplate beside it, px.
const LABEL_CLEARANCE: f64 = 6.0;
/// Backplate padding around the label text, px.
const LABEL_PAD_X: f64 = 3.0;
const LABEL_PAD_Y: f64 = 2.0;
/// Half the height of the %MAC row's boxes, px.
pub(super) const MAC_ROW_HALF_HEIGHT: f64 = 8.0;
/// How far, in label heights, a label may move away from its own line.
const MAX_LABEL_STEPS: usize = 6;

fn overlaps(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// The backplate `[x0, y0, x1, y1]` of a mass label and whether it lies
/// below its line. The label keeps [`LABEL_CLEARANCE`] from its own line and
/// must not cross another mass line, an earlier label, the %MAC row or the
/// frame. It tries its preferred side at the forward end of the line, then
/// the other side, then both sides at the aft end, each further out in turn;
/// when nothing is free it keeps the preferred placement.
fn place_mass_label(
    annotation: &MassAnnotation,
    all: &[MassAnnotation],
    placed: &[[f64; 4]],
    fr: &Frame,
    mac_row_y: f64,
) -> ([f64; 4], bool) {
    let plate_w = annotation.width + 2.0 * LABEL_PAD_X;
    let plate_h = annotation.height + 2.0 * LABEL_PAD_Y;
    let y = annotation.a[1];
    let x_min = fr.left + 3.0;
    let x_max = fr.right() - plate_w - 3.0;
    let forward = (annotation.a[0] + 11.0).clamp(x_min, x_max);
    let aft = (annotation.b[0] - 11.0 - plate_w).clamp(x_min, x_max);
    let plate = |x: f64, below: bool, step: usize| {
        let offset = LABEL_CLEARANCE + step as f64 * (plate_h + 2.0);
        let top = if below {
            y + offset
        } else {
            y - offset - plate_h
        };
        [x, top, x + plate_w, top + plate_h]
    };
    let mac_row = [
        fr.left,
        mac_row_y - MAC_ROW_HALF_HEIGHT - 2.0,
        fr.right(),
        mac_row_y + MAC_ROW_HALF_HEIGHT + 2.0,
    ];
    let free = |rect: [f64; 4]| {
        let inside = rect[1] >= fr.top + 1.0 && rect[3] <= fr.bottom() - 1.0;
        let crosses_line = all.iter().any(|other| {
            let band = [
                other.a[0],
                other.a[1] - LABEL_CLEARANCE + 1.0,
                other.b[0],
                other.a[1] + LABEL_CLEARANCE - 1.0,
            ];
            overlaps(rect, band)
        });
        inside
            && !overlaps(rect, mac_row)
            && !crosses_line
            && !placed.iter().any(|other| overlaps(rect, *other))
    };
    let preferred = annotation.below;
    (0..MAX_LABEL_STEPS)
        .flat_map(|step| {
            [
                (forward, preferred),
                (forward, !preferred),
                (aft, preferred),
                (aft, !preferred),
            ]
            .map(|(x, below)| (plate(x, below, step), below))
        })
        .find(|(rect, _)| free(*rect))
        .unwrap_or((plate(forward, preferred, 0), preferred))
}

/// Draw each distinct analyzed/design mass across the ground envelope (capped
/// at `top_kg`), then its label. `mac_row_y` is the %MAC row the labels keep
/// clear of.
pub(super) fn draw_mass_lines(
    scene: &mut Scene,
    data: &LoadTrimSheetData,
    fr: &Frame,
    ink: &Ink,
    top_kg: f64,
    mac_row_y: f64,
) {
    let (w0, w1) = fr.w_range_kg;
    let fwd_i = |m: f64| data.index_at(m, limit_at(&data.ground_limits, m, true));
    let aft_i = |m: f64| data.index_at(m, limit_at(&data.ground_limits, m, false));
    let mut annotations: Vec<MassAnnotation> = Vec::new();
    for (index, wl) in data.weight_lines.iter().enumerate() {
        if wl.mass_kg < w0
            || wl.mass_kg > w1
            || data.weight_lines[..index]
                .iter()
                .any(|line| line.mass_kg == wl.mass_kg)
        {
            continue;
        }
        let roles: Vec<MassRole> = data
            .weight_lines
            .iter()
            .filter(|line| line.mass_kg == wl.mass_kg)
            .map(|line| line.role)
            .collect();
        let m = wl.mass_kg.min(top_kg);
        let (a, b) = (fr.map(fwd_i(m), wl.mass_kg), fr.map(aft_i(m), wl.mass_kg));
        scene.add(SceneElement::Line {
            p1: a,
            p2: b,
            stroke: Stroke::new(ink.text, 2.0),
        });
        // Reserve the longer of the English and Spanish role labels: the
        // sheet is localized after it is drawn.
        let characters = roles
            .iter()
            .map(|role| role.reserved_chars())
            .sum::<usize>()
            + roles.len().saturating_sub(1) * 3
            + kg(wl.mass_kg).chars().count()
            + 4;
        let em = CSS_PIXELS_PER_POINT * CONSERVATIVE_ADVANCE_EM;
        let font_size = 11.5_f64.min((fr.width - 12.0) / (characters as f64 * em));
        let labels: Vec<&str> = roles.iter().map(|role| role.label()).collect();
        annotations.push(MassAnnotation {
            label: format!("{} {} kg", labels.join(" / "), kg(wl.mass_kg)),
            a,
            b,
            // Takeoff mass caps the envelope, so its label prefers the
            // inside, below the line.
            below: roles
                .iter()
                .any(|role| matches!(role, MassRole::SizedTakeoff | MassRole::AnalyzedTakeoff)),
            font_size,
            width: (characters as f64 * font_size * em).min(fr.width - 12.0),
            height: font_size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM,
        });
    }
    let mut placed: Vec<[f64; 4]> = Vec::new();
    for annotation in &annotations {
        let (plate, below) = place_mass_label(annotation, &annotations, &placed, fr, mac_row_y);
        placed.push(plate);
        scene.add(SceneElement::Rect {
            x: plate[0],
            y: plate[1],
            width: plate[2] - plate[0],
            height: plate[3] - plate[1],
            rx: 2.0,
            fill: Some(Fill::new(ink.paper)),
            stroke: None,
        });
        scene.add(SceneElement::Text {
            text: annotation.label.clone(),
            pos: if below {
                [plate[0] + LABEL_PAD_X, plate[1] + LABEL_PAD_Y]
            } else {
                [plate[0] + LABEL_PAD_X, plate[3] - LABEL_PAD_Y]
            },
            font_size: annotation.font_size,
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
}
