// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The collapsible card the Aircraft and Mission pages share, and how it takes
//! part in a search jump: a card that holds the searched control opens, so the
//! control is drawn (and can scroll into view) even when the user closed it.

use egui::{CollapsingHeader, Ui};

use crate::views::tr_label;

/// A titled card. While the search highlights one of `open_for` (dotted
/// configuration paths, or their parents) the card is forced open.
pub(crate) fn card_for(
    ui: &mut Ui,
    title: &str,
    open_for: &[&str],
    body: impl FnOnce(&mut Ui),
) -> egui::Response {
    let forced = crate::search::focus_wants_any(ui.ctx(), open_for);
    card_with(ui, egui::Id::new(title), title, true, forced, body)
}

/// A titled card with its own id and starting state: `forced` opens it (a
/// search jump into it), whatever the user did.
pub(crate) fn card_with(
    ui: &mut Ui,
    id: egui::Id,
    title: &str,
    default_open: bool,
    forced: bool,
    body: impl FnOnce(&mut Ui),
) -> egui::Response {
    crate::theme::card_frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut header = CollapsingHeader::new(crate::theme::card_title(ui, tr_label(title)))
                .id_salt(id)
                .default_open(default_open)
                .open(crate::perf_tour::force_open());
            if forced {
                header = header.open(Some(true));
            }
            crate::theme::show_header(ui, header, body);
        })
        .response
}

/// The heading of a group of cards or rows that is not itself a card: the
/// header style one level below the card it stands in (on the page, a card's).
pub(crate) fn group_heading(ui: &mut Ui, title: &str) {
    let title = crate::theme::section_title(ui, tr_label(title));
    crate::theme::header_label(ui, title);
}

/// Pulse the control drawn between `first` and `last` (the responses of its
/// label and editor) when the search targets `path`.
pub(crate) fn anchor_span(ui: &mut Ui, path: &str, first: egui::Rect, last: egui::Rect) {
    crate::search::focus_anchor(ui, path, first.union(last));
}
