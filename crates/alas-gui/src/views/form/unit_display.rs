// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a schema unit is shown next to a value.

use std::cell::RefCell;
use std::rc::Rc;

use alas_units::notation::has_notation;
use egui::{vec2, Color32, DragValue, Layout, Response, Ui};

/// Render a schema unit the way the desktop displays it.
///
/// The schema states a unit per field, and four different spellings of a
/// dimensionless quantity had reached the screen at once: a literal hyphen
/// (`10.000 -`), the word `fraction` (`0.027 fraction`), a phrase
/// (`0.020 fraction of critical`) and nothing at all. This collapses them to
/// one convention: a dimensionless quantity is a bare number, and a unit that
/// names the reference a fraction is taken of keeps only that reference
/// (`0.080 of semi-span`). Physical units are untouched apart from typography:
/// `^2`/`^3` become real superscripts and the ASCII `.` that separates
/// multiplied unit symbols becomes a middle dot, so `kg/(kgf.hr)` reads
/// `kg/(kgf.h)` with the conventional separator instead of a full stop.
///
/// No value is converted here; this is presentation only.
pub(crate) fn display_unit(unit: &str) -> String {
    let unit = unit.trim();
    let dimensionless = [
        "-",
        "--",
        "1",
        "0-1",
        "fraction",
        "dimensionless",
        "ratio",
        "x/c",
        "chord fraction",
        "root chord ratio",
    ];
    if unit.is_empty() || dimensionless.contains(&unit) {
        return String::new();
    }
    // A percentage of the mean aerodynamic chord reads like its fraction
    // ("30 % of MAC", "0.1 of MAC"), not as a bare "% MAC".
    if unit == "% MAC" {
        return alas_i18n::t(Some("% of MAC"), None).into_owned();
    }
    // The main gear's position reads as a phrase, not as "of MAC aft of MAC LE".
    if unit == "fraction of MAC aft of MAC LE" {
        return alas_i18n::t(Some("MAC aft of the MAC leading edge"), None).into_owned();
    }
    // A fraction taken of a named reference keeps the reference only.
    for prefix in ["fraction of ", "0-1 of "] {
        if let Some(reference) = unit.strip_prefix(prefix) {
            let reference = match reference {
                "semispan" => "semi-span",
                other => other,
            };
            // The phrase is translated whole: Spanish needs an article that
            // depends on the reference (`de la MAC`, `de semienvergadura`).
            return alas_i18n::t(Some(format!("of {reference}").as_str()), None).into_owned();
        }
    }
    typeset_unit(unit)
}

/// Superscripts and the middle dot of a physical unit symbol.
fn typeset_unit(unit: &str) -> String {
    // The UAV pages spell squares and cubes without a caret. Map only these
    // exact symbols; a general "digit after a letter" rule would also rewrite
    // chemical and model names.
    let unit = match unit {
        "m2" => "m^2",
        "m3" => "m^3",
        "kg/m2" => "kg/m^2",
        "kg/m3" => "kg/m^3",
        "m/s2" => "m/s^2",
        other => other,
    };
    let mut text = unit.replace("^2", "\u{b2}").replace("^3", "\u{b3}");
    // `hr` is not the SI symbol for the hour and only ever appears as a
    // denominator token here.
    text = text.replace("kgf.hr", "kgf.h");
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, character) in bytes.iter().enumerate() {
        let multiplied = *character == '.'
            && index > 0
            && index + 1 < bytes.len()
            && bytes[index - 1].is_alphanumeric()
            && bytes[index + 1].is_alphabetic();
        out.push(if multiplied { '\u{b7}' } else { *character });
    }
    out
}

/// A displayed unit in the engineering notation of the form captions: the
/// lift or drag coefficient a unit is taken per (`1/CL²`) carries its
/// subscript, `1/C_L²`. Only a whole `CL` or `CD` token is marked.
fn unit_notation(unit: &str) -> String {
    // A neighbour that ends the token: none, or neither a letter nor a digit.
    let edge = |neighbour: Option<&char>| !neighbour.is_some_and(char::is_ascii_alphanumeric);
    let chars: Vec<char> = unit.chars().collect();
    let mut out = String::with_capacity(unit.len() + 2);
    for (index, &character) in chars.iter().enumerate() {
        out.push(character);
        let coefficient = character == 'C'
            && matches!(chars.get(index + 1), Some('L' | 'D'))
            && edge(index.checked_sub(1).and_then(|before| chars.get(before)))
            && edge(chars.get(index + 2));
        if coefficient {
            out.push('_');
        }
    }
    out
}

/// The value box of a numeric field: `drag` across the cell, its value
/// written by `format` and followed by `suffix` (a space and the unit).
///
/// egui draws a drag value's text plain, so when the unit has notation (a
/// subscript, as in `1/C_L²`) the box's own text is hidden and the value and
/// unit are drawn over it in the notation of the captions, in the box's font,
/// colour and centring. While the box is being edited it shows the bare
/// number, as every value box does.
pub(super) fn value_box<'a>(
    ui: &mut Ui,
    drag: DragValue<'a>,
    format: impl Fn(f64) -> String + 'a,
    suffix: String,
) -> Response {
    let size = vec2(ui.available_width(), ui.spacing().interact_size.y);
    let marked = unit_notation(&suffix);
    if !has_notation(&marked) {
        let drag = drag.custom_formatter(move |value, _| format(value));
        return ui.add_sized(size, drag.suffix(suffix));
    }
    // The text the box shows, as its formatter wrote it this frame.
    let shown = Rc::new(RefCell::new(String::new()));
    let sink = Rc::clone(&shown);
    let drag = drag
        .custom_formatter(move |value, _| {
            let text = format(value);
            *sink.borrow_mut() = text.clone();
            text
        })
        .suffix(suffix);
    let layout = Layout::centered_and_justified(ui.layout().main_dir());
    ui.allocate_ui_with_layout(size, layout, |ui| {
        // The drag value takes the next id, and is edited while it has focus.
        let id = ui.next_auto_id();
        if ui.memory(|memory| memory.has_focus(id)) {
            return ui.add(drag);
        }
        let outer = ui.visuals().override_text_color;
        ui.visuals_mut().override_text_color = Some(Color32::TRANSPARENT);
        let response = ui.add(drag);
        if ui.memory(|memory| memory.has_focus(id)) && !response.clicked() {
            // Focus arrived inside the drag value (the Tab key), which then
            // drew its edit box without text: draw the frame again.
            ui.ctx().request_repaint();
        } else if ui.is_rect_visible(response.rect) {
            let color = outer.unwrap_or_else(|| ui.style().interact(&response).text_color());
            let font = ui.style().drag_value_text_style.resolve(ui.style());
            let text = format!("{}{marked}", shown.borrow());
            let job = crate::notation::job(&text, &font, color);
            let galley = ui.fonts(|fonts| fonts.layout_job(job));
            let inner = response.rect.shrink2(ui.spacing().button_padding);
            let at = ui.layout().align_size_within_rect(galley.size(), inner).min;
            ui.painter().galley(at, galley, color);
        }
        response
    })
    .inner
}
