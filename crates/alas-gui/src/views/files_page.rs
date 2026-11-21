// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Files page: a file explorer for the ALAS data folder. A navigation bar
//! (back, forward, up, address, search), a left pane of quick-access places and
//! a folder tree, a details or large-icons list, a preview pane and a status
//! bar. Folders are listed, previews built and files deleted on worker threads
//! and cached; a frame never waits for the disk.

use std::path::PathBuf;

use egui::{Key, Ui};

use crate::file_dialog::{spec, DialogKind, DialogTarget};
use crate::file_library::ObjectKind;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields, tr_label};

#[path = "files_grid.rs"]
mod files_grid;
#[path = "files_icons.rs"]
mod files_icons;
#[path = "files_interact.rs"]
mod files_interact;
#[path = "files_jobs.rs"]
mod files_jobs;
#[path = "files_label.rs"]
mod files_label;
#[path = "files_list.rs"]
mod files_list;
#[path = "files_menu.rs"]
mod files_menu;
#[path = "files_model.rs"]
mod files_model;
#[path = "files_nav.rs"]
mod files_nav;
#[path = "files_pane.rs"]
mod files_pane;
#[path = "files_popup.rs"]
mod files_popup;
#[path = "files_preview.rs"]
mod files_preview;
#[path = "files_side.rs"]
mod files_side;
#[path = "files_state.rs"]
mod files_state;
#[path = "files_status.rs"]
mod files_status;
#[path = "files_toolbar.rs"]
mod files_toolbar;

pub use files_state::LibraryUi;

use files_list::{Background, Events};
use files_model::{actions_for, Action, Entry, History};
use files_state::{ViewMode, POLL_SECONDS};

fn json_filters() -> Vec<(String, Vec<String>)> {
    vec![
        ("ALAS files".to_owned(), vec!["json".to_owned()]),
        (
            "ALAS configuration".to_owned(),
            vec!["yaml".to_owned(), "yml".to_owned()],
        ),
    ]
}

/// First-frame setup, worker answers and the periodic re-read of the folder.
fn prepare(state: &mut AppState, ctx: &egui::Context, now: f64) {
    let data = state.data_dir();
    let roots = files_side::allowed_roots(state);
    let lib = &mut state.shell.library;
    lib.roots = roots;
    if lib.history.is_none() {
        lib.history = Some(History::new(data.clone()));
        lib.expanded.insert(data.clone());
    }
    let messages: Vec<_> = lib.jobs.rx.try_iter().collect();
    let mut finished = Vec::new();
    for message in messages {
        finished.extend(lib.apply(message, ctx));
    }
    for done in finished {
        files_menu::finish_op(state, done);
    }
    let lib = &mut state.shell.library;
    let focused = ctx.input(|input| input.viewport().focused.unwrap_or(true));
    let regained = focused && !lib.was_focused;
    lib.was_focused = focused;
    if regained || now - lib.poll_at > POLL_SECONDS {
        lib.poll_at = now;
        for listing in lib.listings.values_mut() {
            listing.loaded = false;
        }
        lib.autosaves_at = None;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(POLL_SECONDS));
    let current = lib.current();
    let listing = lib.listings.entry(current.clone()).or_default();
    if !listing.loaded && !listing.requested {
        listing.requested = true;
        lib.jobs.list(ctx, current.clone());
    }
    refresh_autosaves(state, now);
    start_search(state, ctx);
    reveal_pending(state);
}

/// Which autosave slots exist (a handful of `exists` checks every few seconds).
fn refresh_autosaves(state: &mut AppState, now: f64) {
    if state.shell.library.autosaves_at.is_some() {
        return;
    }
    let data = state.data_dir();
    let mut slots: Vec<(String, String)> = Vec::new();
    if crate::file_library::sandbox_autosave_path(&data).exists() {
        slots.push(("sandbox".to_owned(), tr_label("Sandbox")));
    }
    for kind in [
        ObjectKind::Aircraft,
        ObjectKind::MissionPlan,
        ObjectKind::PayloadLayout,
    ] {
        if crate::file_library::autosave_path(&data, kind).exists() {
            slots.push((kind.id().to_owned(), tr_label(kind.title())));
        }
    }
    let lib = &mut state.shell.library;
    if lib.autosaves.len() != slots.len() {
        lib.notice_dismissed = false;
    }
    lib.autosaves = slots;
    lib.autosaves_at = Some(now);
}

fn start_search(state: &mut AppState, ctx: &egui::Context) {
    let lib = &mut state.shell.library;
    if !lib.searching_subfolders() {
        lib.search.for_query = None;
        lib.search.results = None;
        return;
    }
    let wanted = (lib.current(), lib.query.trim().to_lowercase());
    if lib.search.for_query.as_ref() == Some(&wanted) {
        return;
    }
    lib.search.id += 1;
    lib.search.results = None;
    lib.jobs
        .search(ctx, lib.search.id, wanted.0.clone(), wanted.1.clone());
    lib.search.for_query = Some(wanted);
}

/// Select (and maybe start renaming) an item that was just created or typed.
fn reveal_pending(state: &mut AppState) {
    let lib = &mut state.shell.library;
    let Some((path, rename)) = lib.reveal.clone() else {
        return;
    };
    let Some(parent) = path.parent().map(std::path::Path::to_path_buf) else {
        lib.reveal = None;
        return;
    };
    if parent != lib.current() {
        if files_model::allowed(&parent, &lib.roots) {
            lib.go(parent);
        } else {
            lib.reveal = None;
        }
        return;
    }
    let Some(listing) = lib.listings.get(&parent).filter(|listing| listing.loaded) else {
        return;
    };
    let found = listing
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .cloned();
    lib.reveal = None;
    if let Some(entry) = found {
        lib.query.clear();
        lib.sel.only(&entry.path);
        lib.follow_cursor = true;
        if rename {
            lib.rename = Some(files_state::RenameState {
                path: entry.path.clone(),
                original: entry.name.clone(),
                text: entry.name,
                fresh: true,
            });
        }
    }
}

/// Render the Files page.
pub fn show_files_page(state: &mut AppState, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|input| input.time);
    let typing = ctx.wants_keyboard_input();
    prepare(state, &ctx, now);
    files_toolbar::show_title_row(state, ui);
    ui.add_space(2.0);
    files_nav::show_nav_bar(state, ui);
    files_toolbar::show_autosave_notice(state, ui);
    ui.add_space(4.0);
    let (entries, order) = state.shell.library.view();
    handle_keys(state, &ctx, typing, &entries, &order);
    egui::TopBottomPanel::bottom("files_status")
        .show_separator_line(false)
        .frame(egui::Frame::none())
        .show_inside(ui, |ui| {
            files_status::show_status(state, ui, &entries, &order)
        });
    let total = ui.available_width();
    egui::SidePanel::left("files_nav_panel")
        .resizable(true)
        .default_width(files_side::preferred_width(state, ui, 264.0).min(total * 0.4))
        .width_range(170.0..=(total * 0.4).max(190.0))
        .frame(egui::Frame::none())
        .show_inside(ui, |ui| files_side::show_side(state, ui));
    let preview = state.shell.library.show_preview && total > 820.0;
    if preview {
        egui::SidePanel::right("files_preview_panel")
            .resizable(true)
            .default_width(290.0)
            .width_range(220.0..=(total * 0.45).max(240.0))
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(10.0, 0.0)))
            .show_inside(ui, |ui| files_pane::show_preview_pane(state, ui));
    }
    let events = egui::CentralPanel::default()
        .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(6.0, 0.0)))
        .show_inside(ui, |ui| files_list::show_list(state, ui))
        .inner;
    apply_events(state, &ctx, &entries, &order, events);
    files_toolbar::show_save_window(state, &ctx);
    files_menu::show_dialogs(state, &ctx);
    crate::files_storage::show_window(state, &ctx);
}

/// Select by a click, with Ctrl toggling and Shift extending a range.
fn select_click(
    state: &mut AppState,
    entries: &[Entry],
    order: &[usize],
    position: usize,
    modifiers: egui::Modifiers,
) {
    let lib = &mut state.shell.library;
    let path = entries[order[position]].path.clone();
    if modifiers.shift {
        let anchor = lib
            .sel
            .anchor
            .clone()
            .and_then(|anchor| order.iter().position(|&i| entries[i].path == anchor))
            .unwrap_or(position);
        if !modifiers.command {
            lib.sel.items.clear();
        }
        let (low, high) = (anchor.min(position), anchor.max(position));
        for &index in &order[low..=high] {
            lib.sel.items.insert(entries[index].path.clone());
        }
        lib.sel.cursor = Some(path);
    } else if modifiers.command {
        if !lib.sel.items.remove(&path) {
            lib.sel.items.insert(path.clone());
        }
        lib.sel.anchor = Some(path.clone());
        lib.sel.cursor = Some(path);
    } else {
        lib.sel.only(&path);
    }
}

fn apply_events(
    state: &mut AppState,
    ctx: &egui::Context,
    entries: &[Entry],
    order: &[usize],
    events: Events,
) {
    let lib = &mut state.shell.library;
    if let Some(key) = events.sort {
        lib.sort = lib.sort.clicked(key);
    }
    if events.clear {
        lib.sel.clear();
    }
    if let Some((position, modifiers)) = events.click {
        select_click(state, entries, order, position, modifiers);
    }
    if let Some(position) = events.open {
        let entry = entries[order[position]].clone();
        let data = state.data_dir();
        state.shell.library.sel.only(&entry.path);
        let action = state.shell.library.default_for(&entry, &data);
        files_menu::run_action(state, ctx, action, &[entry]);
    }
    if let Some(action) = events.menu {
        let targets: Vec<Entry> = state
            .shell
            .library
            .selected(entries, order)
            .into_iter()
            .cloned()
            .collect();
        files_menu::run_action(state, ctx, action, &targets);
    }
    match events.background {
        Some(Background::NewFolder) => files_menu::new_folder(state),
        Some(Background::Refresh) => state.shell.library.invalidate(),
        Some(Background::SaveCurrent) => files_toolbar::open_save_window(state),
        Some(Background::ShowFolder) => {
            let current = state.shell.library.current();
            files_menu::reveal(state, &current, true);
        }
        None => {}
    }
    if let Some(text) = events.rename {
        files_menu::finish_rename(state, text);
    }
}

/// Move the cursor to the item at `target` (extending the selection with Shift).
fn move_cursor(
    state: &mut AppState,
    entries: &[Entry],
    order: &[usize],
    target: usize,
    extend: bool,
) {
    if order.is_empty() {
        return;
    }
    let target = target.min(order.len() - 1);
    let modifiers = egui::Modifiers {
        shift: extend,
        ..egui::Modifiers::NONE
    };
    select_click(state, entries, order, target, modifiers);
    state.shell.library.follow_cursor = true;
}

fn handle_keys(
    state: &mut AppState,
    ctx: &egui::Context,
    typing: bool,
    entries: &[Entry],
    order: &[usize],
) {
    let lib = &state.shell.library;
    if typing
        || lib.rename.is_some()
        || lib.address.is_some()
        || lib.delete_confirm.is_some()
        || lib.restore_confirm.is_some()
    {
        return;
    }
    let (pressed, modifiers) = ctx.input(|input| {
        let keys: Vec<Key> = [
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::Enter,
            Key::Backspace,
            Key::F2,
            Key::F5,
            Key::Delete,
            Key::A,
            Key::L,
            Key::F,
            Key::C,
            Key::Escape,
        ]
        .into_iter()
        .filter(|key| input.key_pressed(*key))
        .collect();
        (keys, input.modifiers)
    });
    if pressed.is_empty() {
        return;
    }
    let icons = lib.mode == ViewMode::Icons;
    let columns = if icons { lib.grid_columns.max(1) } else { 1 };
    let page = if icons {
        columns * ((lib.list_height / files_grid::CELL_HEIGHT) as usize).max(1)
    } else {
        ((lib.list_height / 26.0) as usize).max(1)
    };
    let current = lib
        .sel
        .cursor
        .as_ref()
        .and_then(|cursor| order.iter().position(|&i| entries[i].path == *cursor));
    let step = |forward: bool, by: usize| -> usize {
        match (current, forward) {
            (None, true) => 0,
            (None, false) => order.len().saturating_sub(1),
            (Some(at), true) => at + by,
            (Some(at), false) => at.saturating_sub(by),
        }
    };
    let extend = modifiers.shift;
    let command = modifiers.command;
    for key in pressed {
        match key {
            Key::ArrowDown => move_cursor(state, entries, order, step(true, columns), extend),
            Key::ArrowUp => move_cursor(state, entries, order, step(false, columns), extend),
            Key::ArrowRight if modifiers.alt => state.shell.library.step(true),
            Key::ArrowLeft if modifiers.alt => state.shell.library.step(false),
            Key::ArrowRight if icons => move_cursor(state, entries, order, step(true, 1), extend),
            Key::ArrowLeft if icons => move_cursor(state, entries, order, step(false, 1), extend),
            Key::Home => move_cursor(state, entries, order, 0, extend),
            Key::End => move_cursor(state, entries, order, order.len().saturating_sub(1), extend),
            Key::PageDown => move_cursor(state, entries, order, step(true, page), extend),
            Key::PageUp => move_cursor(state, entries, order, step(false, page), extend),
            Key::Backspace => {
                let lib = &mut state.shell.library;
                if let Some(parent) = files_model::up_target(&lib.current(), &lib.roots) {
                    lib.go(parent);
                }
            }
            Key::F5 => state.shell.library.invalidate(),
            Key::A if command => {
                let lib = &mut state.shell.library;
                lib.sel.items = order.iter().map(|&i| entries[i].path.clone()).collect();
            }
            Key::L if command => {
                let text = state.shell.library.current().display().to_string();
                state.shell.library.address = Some(files_state::AddressEdit {
                    text,
                    fresh: true,
                    refused: false,
                });
            }
            Key::F if command => {
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("files_search_box")));
            }
            Key::Escape => {
                let lib = &mut state.shell.library;
                lib.sel.clear();
                lib.query.clear();
            }
            Key::Enter | Key::F2 | Key::Delete | Key::C => {
                key_action(state, ctx, key, command, entries, order)
            }
            _ => {}
        }
    }
}

fn key_action(
    state: &mut AppState,
    ctx: &egui::Context,
    key: Key,
    command: bool,
    entries: &[Entry],
    order: &[usize],
) {
    let data = state.data_dir();
    let lib = &state.shell.library;
    let targets: Vec<Entry> = lib.selected(entries, order).into_iter().cloned().collect();
    if targets.is_empty() || (key == Key::C && !command) {
        return;
    }
    let items: Vec<_> = targets
        .iter()
        .map(|entry| lib.item_ctx(entry, &data))
        .collect();
    let allowed = actions_for(&items);
    let action = match key {
        Key::Enter if targets.len() == 1 => lib.default_for(&targets[0], &data),
        Key::F2 => Action::Rename,
        Key::Delete => Action::Delete,
        Key::C => Action::CopyPath,
        _ => return,
    };
    if allowed.contains(&action) {
        files_menu::run_action(state, ctx, action, &targets);
    }
}

#[cfg(test)]
#[path = "files_page_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "files_layout_tests.rs"]
mod layout_tests;
