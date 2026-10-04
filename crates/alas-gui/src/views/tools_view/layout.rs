// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout primitives of the External Tools page: tool cards, paired cards of
//! equal height, discipline headings, and full-width path rows.

use egui::{Align, Layout, RichText, TextEdit, Ui};

use crate::path_picker::ToolPathTarget;
use crate::state::AppState;
use crate::views::tr;

use super::open_in_file_explorer;

/// Page width from which two cards share a row.
pub(super) const TWO_COLUMN_WIDTH: f32 = 920.0;
/// Card width from which a card splits its own body into two columns.
pub(super) const SPLIT_CARD_WIDTH: f32 = 760.0;

/// Equal-height bookkeeping for the two cards of one [`card_row`].
#[derive(Clone, Copy, Default)]
struct CardPairing {
    /// Inner height every card of the row is padded to (last frame's tallest).
    target: f32,
    /// Tallest natural inner height measured this frame.
    measured: f32,
}

fn pairing_id() -> egui::Id {
    egui::Id::new("alas_tools_card_pairing")
}

/// One tool card: the title links to the publisher's documentation, the help
/// paragraphs are its tooltip.
pub(super) fn card(
    ui: &mut Ui,
    title: &str,
    link: &str,
    help: &[&str],
    body: impl FnOnce(&mut Ui),
) {
    let pairing = ui
        .ctx()
        .data(|data| data.get_temp::<CardPairing>(pairing_id()));
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let tooltip = help
            .iter()
            .map(|paragraph| tr(paragraph))
            .chain(std::iter::once(tr("Click to open documentation.")))
            .collect::<Vec<_>>()
            .join("\n\n");
        ui.hyperlink_to(RichText::new(tr(title)).strong().size(16.0), link)
            .on_hover_text(tooltip);
        ui.add_space(4.0);
        body(ui);
        if let Some(mut pairing) = pairing {
            pairing.measured = pairing.measured.max(ui.min_rect().height());
            ui.ctx()
                .data_mut(|data| data.insert_temp(pairing_id(), pairing));
            ui.set_min_height(pairing.target);
        }
    });
}

/// Two cards side by side with one common height, so a short card does not
/// leave a ragged gap beside a tall one. Narrow pages stack them instead.
pub(super) fn card_row<T>(
    ui: &mut Ui,
    key: &str,
    shared: &mut T,
    left: impl FnOnce(&mut T, &mut Ui),
    right: impl FnOnce(&mut T, &mut Ui),
) {
    if ui.available_width() < TWO_COLUMN_WIDTH {
        left(shared, ui);
        ui.add_space(8.0);
        right(shared, ui);
        return;
    }
    let slot = ui.id().with(("alas_tools_card_row", key));
    let target = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(slot))
        .unwrap_or(0.0);
    let pairing = CardPairing {
        target,
        measured: 0.0,
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(pairing_id(), pairing));
    ui.columns(2, |columns| {
        let (first, second) = columns.split_at_mut(1);
        left(shared, &mut first[0]);
        right(shared, &mut second[0]);
    });
    let measured = ui
        .ctx()
        .data_mut(|data| data.remove_temp::<CardPairing>(pairing_id()))
        .map_or(0.0, |pairing| pairing.measured);
    if (measured - target).abs() > 0.5 {
        ui.ctx().data_mut(|data| data.insert_temp(slot, measured));
        ui.ctx().request_repaint();
    }
}

/// Render `left` and `right` in two columns when the card is wide enough,
/// otherwise one after the other.
pub(super) fn split_body<T>(
    ui: &mut Ui,
    shared: &mut T,
    left: impl FnOnce(&mut T, &mut Ui),
    right: impl FnOnce(&mut T, &mut Ui),
) {
    if ui.available_width() >= SPLIT_CARD_WIDTH {
        ui.columns(2, |columns| {
            let (first, second) = columns.split_at_mut(1);
            left(shared, &mut first[0]);
            right(shared, &mut second[0]);
        });
    } else {
        left(shared, ui);
        ui.add_space(8.0);
        right(shared, ui);
    }
}

/// A discipline heading between groups of cards.
pub(super) fn section_heading(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(title))
            .strong()
            .size(16.0)
            .color(ui.visuals().hyperlink_color),
    );
    ui.add_space(4.0);
}

/// A sub-heading inside a card.
pub(super) fn sub_heading(ui: &mut Ui, title: &str) -> egui::Response {
    ui.label(RichText::new(tr(title)).strong())
}

/// One editable path or value: the label on its own line, then a text box
/// that fills the card with Browse and Open folder aligned to its right.
pub(super) fn text_row(
    state: &mut AppState,
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    directory: bool,
    picker_target: Option<ToolPathTarget>,
) -> bool {
    let label_response = ui.label(RichText::new(tr(label)).weak());
    if let Some(help) = text_row_help(label) {
        label_response.on_hover_text(tr(help));
    }
    let mut changed = false;
    // The outer horizontal bounds the row to one line; a bare right-to-left
    // layout in a vertical parent would claim the remaining card height.
    ui.horizontal(|ui| ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if let Some(target) = picker_target {
            if ui
                .add_enabled(!value.trim().is_empty(), egui::Button::new(tr("Open folder")))
                .on_hover_text(tr("Show this exact location in the system file explorer."))
                .clicked()
            {
                if let Err(error) = open_in_file_explorer(value, directory) {
                    state.log(error, crate::state::LogKind::Warn);
                }
            }
            let pending = state.path_picker_pending(target);
            if ui
                .add_enabled(!pending, egui::Button::new(tr("Browse...")))
                .on_hover_text(tr(
                    "Choose the directory or executable with the native file picker. The selected location is saved in this user's preferences.",
                ))
                .clicked()
            {
                state.begin_path_picker(target, directory);
            }
        }
        let margin = egui::Margin::symmetric(4.0, 2.0);
        let width = (ui.available_width() - margin.sum().x).max(120.0);
        let response = ui.add(TextEdit::singleline(value).margin(margin).desired_width(width));
        if !value.is_empty() {
            // A long path is clipped in the box; the tooltip shows all of it.
            response.clone().on_hover_text(value.as_str());
        }
        changed = response.changed();
    }));
    ui.add_space(2.0);
    changed
}

fn text_row_help(label: &str) -> Option<&'static str> {
    match label {
        "MSC solver override" => Some(
            "For the split MSC Student Edition, select Patran/.../analysis.exe here. Leave blank when the launcher finds its own solver.",
        ),
        "Short RF staging directory" => Some(
            "Set these three paths once to run local SOL 101/SOL 103 beside MSC in every desktop launch. The RF directory must be an absolute path shorter than 38 bytes (for example C:/nas-rf).",
        ),
        "Open-core words (OCMEM)" => Some(
            "Leave OCMEM blank to use the capacity compiled into nastran.exe. This local build records its limit beside the executable; rebuild it with a larger open-core array only if the full mesh exceeds that recorded allocation.",
        ),
        "WSL environment launcher" => Some(
            "The launcher receives each utility and its arguments inside WSL2, for example openfoam2306 from the openfoam.com Ubuntu packages. Without it, WSL runs the utilities without the OpenFOAM environment and a bin directory alone does not load their libraries.",
        ),
        _ => None,
    }
}
