// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cards: the shared elevated surface, one header style per nesting depth and
//! the one gap between a header and the first field under it.
//!
//! The depth of a card is read from the `Ui` stack (how many card frames
//! enclose the header), never from a flag of the caller, so every card at one
//! depth carries the same title whichever page or form draws it.

use egui::{
    CollapsingHeader, CollapsingResponse, Frame, Margin, Response, RichText, Rounding, Ui, UiKind,
};

use super::space::{CARD_PADDING, HEADER_TO_BODY};

/// Horizontal inner margin of a card, pt (the [`CARD_PADDING`] token).
pub const CARD_INNER_MARGIN_X: f32 = CARD_PADDING.x;

/// Corner radius of a card. With the padding it tells a card frame apart from
/// the other frames in the `Ui` stack.
const CARD_ROUNDING: f32 = 12.0;

fn card_margin() -> Margin {
    Margin::symmetric(CARD_PADDING.x, CARD_PADDING.y)
}

/// Frame used for grouped content cards, filled with the window color.
///
/// A distinct fill plus a single ordered corner radius makes grouping visible
/// without relying on the indentation rule that competes with form content.
pub fn card_frame(ui: &Ui) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .inner_margin(card_margin())
        .rounding(Rounding::same(CARD_ROUNDING))
}

/// Return the content width that produces the requested outer card width.
///
/// A frame's horizontal inner margin belongs to the painted card, not to the
/// child UI that receives a requested width. Accounting for it here keeps a
/// row of cards inside its measured gallery width instead of growing each card
/// by the frame padding.
pub fn card_content_width(outer_width: f32) -> f32 {
    (outer_width - 2.0 * CARD_INNER_MARGIN_X).max(1.0)
}

/// How many cards enclose `ui`: 1 inside a card on the page, 2 inside a card
/// in a card, 0 outside every card.
pub fn card_depth(ui: &Ui) -> usize {
    let card = card_margin();
    ui.stack()
        .iter()
        .filter(|node| {
            node.kind() == Some(UiKind::Frame)
                && node.frame().rounding == Rounding::same(CARD_ROUNDING)
                && node.frame().inner_margin == card
        })
        .count()
}

/// The header style of nesting `depth`: a card on the page (depth 1, or a
/// heading outside every card) has the accent title, every deeper card or
/// section the strong body text. One style per depth, nothing else decides it.
pub fn title_at_depth(ui: &Ui, text: impl Into<String>, depth: usize) -> RichText {
    let title = RichText::new(text.into()).strong();
    if depth <= 1 {
        title.size(16.0).color(ui.visuals().hyperlink_color)
    } else {
        title
    }
}

/// Title of the card `ui` is drawn in, styled by the card's depth.
pub fn card_title(ui: &Ui, text: impl Into<String>) -> RichText {
    title_at_depth(ui, text, card_depth(ui))
}

/// Title of a section inside the card `ui` is drawn in: one level below the
/// card. Outside every card it is the heading of a group of cards, styled as
/// a card on the page.
pub fn section_title(ui: &Ui, text: impl Into<String>) -> RichText {
    title_at_depth(ui, text, card_depth(ui) + 1)
}

/// A card or section title drawn as a label, [`HEADER_TO_BODY`] above the
/// first field under it.
pub fn header_label(ui: &mut Ui, title: impl Into<egui::WidgetText>) -> Response {
    header_row(ui, |ui| ui.label(title))
}

/// Show a collapsible card or section `header` and its `body`
/// [`HEADER_TO_BODY`] apart. Inside the body, and from the card to whatever
/// follows it, `ui`'s own spacing applies.
pub fn show_header<R>(
    ui: &mut Ui,
    header: CollapsingHeader,
    body: impl FnOnce(&mut Ui) -> R,
) -> CollapsingResponse<R> {
    header.show(ui, |ui| {
        // egui puts the body one item spacing under the header: move its
        // first line up to the header-to-body gap. The spacing after the
        // whole card is the one it was allocated with, the ordinary one.
        if ui.layout().main_dir() == egui::Direction::TopDown {
            ui.add_space(HEADER_TO_BODY - ui.spacing().item_spacing.y);
        }
        body(ui)
    })
}

/// A header row built by `add` (a title with controls beside it), with
/// [`HEADER_TO_BODY`] as the gap to what follows it.
pub fn header_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let gap = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = HEADER_TO_BODY;
    let out = add(ui);
    ui.spacing_mut().item_spacing.y = gap;
    out
}

/// Columns of cards that end at one bottom: the last card of every column of
/// the column set `set` is padded at the end of its content down to where the
/// longest column ended in the last frame. A column calls
/// [`column_padding`] for the padding of its last card and, once the column
/// is drawn, [`record_column_end`] with its bottom.
pub fn column_padding(ui: &Ui, set: egui::Id, column: usize) -> f32 {
    let ends: Vec<f32> = ui
        .ctx()
        .data(|data| data.get_temp::<Vec<f32>>(set))
        .unwrap_or_default();
    let longest = ends.iter().copied().fold(f32::MIN, f32::max);
    ends.get(column).map_or(0.0, |end| (longest - end).max(0.0))
}

/// Record how tall `column` of the column set `set` was this frame, from the
/// columns' common top to its last card's bottom, and the padding
/// [`column_padding`] gave that card.
pub fn record_column_end(ui: &Ui, set: egui::Id, column: usize, height: f32, padding: f32) {
    let natural = height - padding;
    let changed = ui.ctx().data_mut(|data| {
        let ends = data.get_temp_mut_or_default::<Vec<f32>>(set);
        let before = ends.get(column).copied();
        if ends.len() <= column {
            ends.resize(column + 1, natural);
        }
        ends[column] = natural;
        before.is_none_or(|before| (before - natural).abs() > 0.5)
    });
    if changed {
        // The other columns pad to this end on the next frame.
        ui.ctx().request_repaint();
    }
}
