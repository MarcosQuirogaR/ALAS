// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What one item or the empty part of the Files list does with the mouse:
//! clicks, the context menus and the rename box. Shared by both views.

use egui::{Key, Sense, Ui};

use super::files_list::{Background, Events};
use super::files_menu::menu_item;
use super::files_model::{actions_for, Entry};
use super::files_state::RenameState;
use super::{tr, AppState};

/// Edit the name of the item being renamed in `rect`.
pub(super) fn rename_box(
    ui: &mut Ui,
    rect: egui::Rect,
    rename: &mut RenameState,
    events: &mut Events,
) {
    let output = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::TextEdit::singleline(&mut rename.text)
                .desired_width(rect.width())
                .margin(egui::vec2(4.0, 2.0))
                .show(ui)
        })
        .inner;
    let response = output.response;
    if rename.fresh {
        rename.fresh = false;
        response.request_focus();
        let stem = std::path::Path::new(&rename.text)
            .file_stem()
            .map_or(rename.text.chars().count(), |stem| {
                stem.to_string_lossy().chars().count()
            });
        let mut text_state = output.state;
        text_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(stem),
            )));
        text_state.store(ui.ctx(), response.id);
        return;
    }
    let (enter, escape) = ui.input(|input| {
        (
            input.key_pressed(Key::Enter),
            input.key_pressed(Key::Escape),
        )
    });
    if escape {
        events.rename = Some(None);
    } else if response.lost_focus() {
        events.rename = Some(Some(rename.text.clone()));
        let _ = enter;
    }
}

/// The clicks and the context menu of one item, shared by both views.
pub(super) fn interact(
    state: &mut AppState,
    response: &egui::Response,
    entries: &[Entry],
    order: &[usize],
    position: usize,
    data: &std::path::Path,
    events: &mut Events,
) {
    if response.double_clicked() {
        events.open = Some(position);
    } else if response.clicked() {
        events.click = Some((position, response.ctx.input(|input| input.modifiers)));
    }
    let lib = &state.shell.library;
    super::files_popup::context_menu(response, |ui| {
        let mut chosen = false;
        let items: Vec<_> = lib
            .selected(entries, order)
            .iter()
            .map(|entry| lib.item_ctx(entry, data))
            .collect();
        let actions = actions_for(&items);
        let labels: Vec<String> = actions
            .iter()
            .map(|action| tr(super::files_menu::action_label(*action)))
            .collect();
        super::files_menu::fit_menu_width(ui, labels.iter().map(String::as_str));
        for action in actions {
            if menu_item(ui, action) {
                events.menu = Some(action);
                chosen = true;
            }
        }
        chosen
    });
}

/// The menu of the empty part of the list and the clear-on-click rule.
pub(super) fn background(ui: &mut Ui, rect: egui::Rect, events: &mut Events) {
    let response = ui.interact(rect, ui.id().with("files_list_background"), Sense::click());
    if response.clicked() {
        events.clear = true;
    }
    super::files_popup::context_menu(&response, |ui| {
        let mut chosen = false;
        let entries = [
            (Background::NewFolder, "New folder"),
            (Background::SaveCurrent, "Save current as\u{2026}"),
            (Background::Refresh, "Refresh"),
            (Background::ShowFolder, "Show folder in Windows Explorer"),
        ];
        let labels: Vec<String> = entries.iter().map(|(_, label)| tr(label)).collect();
        super::files_menu::fit_menu_width(ui, labels.iter().map(String::as_str));
        for (item, label) in entries {
            if ui.button(tr(label)).clicked() {
                events.background = Some(item);
                chosen = true;
            }
        }
        chosen
    });
}
