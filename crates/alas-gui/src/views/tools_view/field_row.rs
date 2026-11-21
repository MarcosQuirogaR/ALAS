// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The label/field grid of the External Tools page.
//!
//! Every option row of every card shares one label column (the widest label
//! of the page on one line when the card has the room, else wrapped at word
//! boundaries; never elided) and one field width, so field boxes start at the same x in every row and card of a
//! column. Long paths are middle-elided while the box is not being edited.

use egui::{vec2, Align, Layout, RichText, Ui};

use crate::views::tr;

/// Labels sit above their fields, like every other form of the app.
const INLINE_LABELS: bool = false;
/// Card width from which a row's label sits left of its field.
pub(super) const INLINE_LABEL_WIDTH: f32 = 520.0;
/// Widest a field box grows, pt.
pub(super) const MAX_FIELD_WIDTH: f32 = 4_000.0;
/// The label column never takes more than this share of the card width.
const MAX_LABEL_SHARE: f32 = 0.42;
/// Widest the label column grows in a card too narrow for one-line labels, pt:
/// a longer label wraps at word boundaries.
const MAX_LABEL_COLUMN: f32 = 200.0;
/// The narrowest field box, pt, a card must leave beside a one-line label
/// column and the Browse and Open folder buttons.
pub(super) const MIN_FIELD_WIDTH: f32 = 360.0;

/// Every label that sits in the grid, in English. The label column is as wide
/// as the widest of these once translated.
pub(super) const GRID_LABELS: &[&str] = &[
    "SimBrief username",
    "Fetch timeout",
    "Navigation-data directory",
    "Saved-routes directory",
    "Executable path",
    "MSC solver override",
    "Local solver directory",
    "Runtime DLL directory",
    "Short RF staging directory",
    "Open-core words (OCMEM)",
    "Install directory",
    "Backend",
    "Gmsh executable",
    "ParaView executable",
    "Native OpenFOAM bin directory",
    "Native OpenFOAM project directory",
    "WSL distribution",
    "WSL environment launcher",
    "WSL OpenFOAM bin directory",
];

/// Width of `text` in the label font, pt.
fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x
}

/// Width of the label column of a card `card_width` points across.
///
/// The column is as wide as the widest label of the page, so every label stays
/// on one line, whenever the field keeps at least [`MIN_FIELD_WIDTH`] beside
/// it and its buttons. A card without that much room wraps long labels at
/// word boundaries in a column of at most [`MAX_LABEL_COLUMN`]. The rule
/// depends on the card width only, so equally wide cards share one column.
pub(super) fn label_column(ui: &Ui, card_width: f32) -> f32 {
    let widest = GRID_LABELS
        .iter()
        .map(|label| text_width(ui, &tr(label)))
        .fold(0.0_f32, f32::max);
    let one_line = (widest + 6.0).ceil();
    let buttons = 2.0 * super::layout::picker_button_width(ui) + 2.0 * ui.spacing().item_spacing.x;
    if card_width - one_line - buttons >= MIN_FIELD_WIDTH {
        return one_line;
    }
    one_line
        .min(MAX_LABEL_COLUMN)
        .min(card_width * MAX_LABEL_SHARE)
        .ceil()
}

/// Whether a card `width` points across puts labels left of fields.
pub(super) fn is_inline(width: f32) -> bool {
    width >= INLINE_LABEL_WIDTH
}

/// One grid row: the label (and its tooltip) in the label column, then
/// `field` on the same line. On a narrow card the label sits above the field.
pub(super) fn grid_row<R>(
    ui: &mut Ui,
    label: &str,
    help: Option<&str>,
    field: impl FnOnce(&mut Ui) -> R,
) -> R {
    let height = crate::theme::control_height(ui);
    let text = crate::theme::field_label(&tr(label), true);
    let hover = tr(help.unwrap_or(label));
    let buttons = 2.0 * super::layout::picker_button_width(ui) + 2.0 * ui.spacing().item_spacing.x;
    let column = label_column(ui, ui.available_width());
    // Label left of the field only while the field keeps its minimum width.
    if INLINE_LABELS
        && is_inline(ui.available_width())
        && ui.available_width() - column - buttons >= MIN_FIELD_WIDTH
    {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                vec2(column, height),
                Layout::left_to_right(Align::Center),
                |ui| {
                    // A cell takes the room it used, not the room offered:
                    // the minimum size is what keeps the column one width.
                    ui.set_min_size(vec2(column, height));
                    ui.set_max_width(column);
                    ui.add(egui::Label::new(crate::notation::styled(ui, text)).wrap())
                },
            )
            .response
            .on_hover_text(hover);
            field(ui)
        })
        .inner
    } else {
        // The caption stands the forms' caption-to-box gap above its box.
        let gap = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = crate::theme::space::LABEL_TO_BOX;
        let response = ui.label(crate::notation::styled(ui, text));
        ui.spacing_mut().item_spacing.y = gap;
        if help.is_some() {
            response.on_hover_text(hover);
        }
        ui.horizontal(field).inner
    }
}

/// Width of the field box of a row whose trailing buttons take `buttons`.
///
/// Rows without buttons pass the same reservation as path rows, so every box
/// in a column has one width.
pub(super) fn field_width(ui: &Ui, buttons: f32, margin_x: f32) -> f32 {
    (ui.available_width() - buttons - margin_x).clamp(80.0, MAX_FIELD_WIDTH)
}

/// `text` shortened to the widest middle-elided form for which `fits` holds:
/// the start and (favoured) the end are kept, so a path keeps its root and its
/// file name. Text that already fits is returned unchanged.
pub(super) fn middle_elide(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let shown = |kept: usize| -> String {
        let head = kept / 3;
        let tail = kept - head;
        let mut out: String = chars[..head].iter().collect();
        out.push_str("...");
        out.extend(&chars[chars.len() - tail..]);
        out
    };
    let (mut low, mut high) = (0_usize, chars.len().saturating_sub(1));
    while low < high {
        let mid = (low + high).div_ceil(2);
        if fits(&shown(mid)) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    shown(low)
}

/// A path shortened to the widest form for which `fits` holds, eliding whole
/// middle segments: the root (drive) and the file name are always kept, so
/// `C:\a\b\c\nastran.exe` becomes `C:\a\...\nastran.exe`. Text that is not a
/// path, or whose root plus file name still do not fit, falls back to
/// [`middle_elide`].
pub(super) fn path_elide(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_owned();
    }
    let sep = if text.contains('\\') { '\\' } else { '/' };
    let parts: Vec<&str> = text.split(sep).collect();
    if parts.len() >= 3 {
        let build = |head: usize, tail: usize| {
            let mut out = parts[..head].join(&sep.to_string());
            out.push(sep);
            out.push_str("...");
            out.push(sep);
            out.push_str(&parts[parts.len() - tail..].join(&sep.to_string()));
            out
        };
        // Most segments first: grow the kept head and tail while it fits.
        let mut best = None;
        for total in (2..parts.len()).rev() {
            for head in (1..total).rev() {
                let candidate = build(head, total - head);
                if fits(&candidate) {
                    best = Some(candidate);
                    break;
                }
            }
            if best.is_some() {
                break;
            }
        }
        if let Some(best) = best {
            return best;
        }
    }
    middle_elide(text, fits)
}

/// A path as shown to the user: one slash style per platform.
pub(super) fn display_path(text: &str) -> String {
    if cfg!(windows) && (text.contains(":/") || text.contains(":\\")) {
        text.replace('/', "\\")
    } else {
        text.to_owned()
    }
}

/// A weak label showing `text` on one line, elided to the width left in the
/// row (a path in its middle, a message at its end), with the full text as its
/// tooltip.
pub(super) fn elided_weak_label(ui: &mut Ui, text: &str) {
    let normalised = display_path(text);
    let text = normalised.as_str();
    let width = ui.available_width();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().weak_text_color();
    // A message is elided at its end; egui shows the whole of it on hover.
    if !text.contains('\\') && !text.contains('/') {
        ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
        return;
    }
    // A trailing " (note)" stays whole beside the path while the path, elided
    // by whole folders, keeps its file name; else the note is left to the
    // hover text and the path is elided alone. The label truncates as a last
    // resort, so the text never passes the edge of its cell. The note is the
    // last parenthesis when it holds no folder, so "Program Files (x86)"
    // stays part of its path.
    let separators = ['\\', '/'];
    let (path, note) = match text.rfind(" (") {
        Some(at) if at > 0 && text.ends_with(')') && !text[at..].contains(separators) => {
            text.split_at(at)
        }
        _ => (text, ""),
    };
    let fits = |candidate: &str| {
        ui.painter()
            .layout_no_wrap(candidate.to_owned(), font.clone(), color)
            .size()
            .x
            <= width
    };
    let name = path.rsplit(separators).next().unwrap_or(path);
    let with_note = path_elide(path, |candidate| fits(&format!("{candidate}{note}")));
    let keep_note = !note.is_empty() && with_note.ends_with(name);
    let shown = if keep_note && fits(&format!("{with_note}{note}")) {
        format!("{with_note}{note}")
    } else {
        path_elide(path, fits)
    };
    let label = ui.add(egui::Label::new(RichText::new(shown).weak()).truncate());
    if !text.is_empty() {
        label.on_hover_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::middle_elide;

    #[test]
    fn a_path_that_fits_is_left_alone() {
        assert_eq!(
            middle_elide("C:/tools/gmsh", |s| s.len() <= 20),
            "C:/tools/gmsh"
        );
    }

    #[test]
    fn a_long_path_keeps_its_root_and_file_name() {
        let path = "C:/tools/OpenFOAM/bin/simpleFoam.exe";
        let shown = middle_elide(path, |s| s.chars().count() <= 30);
        assert!(shown.chars().count() <= 30, "{shown}");
        assert!(shown.starts_with("C:/"), "{shown}");
        assert!(shown.ends_with("simpleFoam.exe"), "{shown}");
        assert!(shown.contains("..."), "{shown}");
    }

    #[test]
    fn nothing_fits_degrades_to_the_bare_ellipsis() {
        assert_eq!(middle_elide("abcdefghij", |s| s.len() <= 3), "...");
    }
}
