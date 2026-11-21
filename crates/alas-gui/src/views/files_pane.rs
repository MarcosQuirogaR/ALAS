// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The right-hand preview pane of the Files explorer: for the selected item a
//! thumbnail, its name, type, size and date, a few facts from the file header
//! and the main actions; with nothing selected, a summary of the folder.

use egui::{RichText, Ui};

use super::files_menu::run_action;
use super::files_model::{
    autosave_slot, default_action, format_modified, format_size, Action, Entry,
};
use super::files_state::PreviewSlot;
use super::{tr, tr_fields, AppState};

/// Height of the preview tile when the item has a picture, points.
const THUMB_HEIGHT: f32 = 190.0;
/// Height of the tile that only holds a file-type icon, points.
const ICON_TILE_HEIGHT: f32 = 84.0;

fn ensure_preview(state: &mut AppState, ctx: &egui::Context, entry: &Entry) {
    let lib = &mut state.shell.library;
    if lib
        .preview
        .as_ref()
        .is_some_and(|slot| slot.path == entry.path)
    {
        return;
    }
    lib.preview = Some(PreviewSlot {
        path: entry.path.clone(),
        ..PreviewSlot::default()
    });
    let theme = state.theme.figure_theme_name().to_owned();
    state
        .shell
        .library
        .jobs
        .preview(ctx, entry.path.clone(), theme);
}

fn thumbnail(state: &mut AppState, ui: &mut Ui, entry: &Entry) {
    let width = ui.available_width();
    let has_picture = state
        .shell
        .library
        .preview
        .as_ref()
        .filter(|slot| slot.path == entry.path)
        .is_some_and(|slot| {
            slot.texture.is_some() || slot.preview.as_ref().is_some_and(|p| p.scene.is_some())
        });
    let bounds = egui::vec2(
        width,
        if has_picture {
            THUMB_HEIGHT
        } else {
            ICON_TILE_HEIGHT
        },
    );
    let (rect, _) = ui.allocate_exact_size(bounds, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::Rounding::same(8.0),
        ui.visuals().extreme_bg_color,
    );
    let inner = rect.shrink(8.0);
    let lib = &state.shell.library;
    let slot = lib.preview.as_ref().filter(|slot| slot.path == entry.path);
    let texture = slot.and_then(|slot| slot.texture.clone());
    let scene = slot
        .and_then(|slot| slot.preview.as_ref())
        .and_then(|preview| preview.scene.clone());
    let loading = slot.is_some_and(|slot| slot.preview.is_none());
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let scale = (inner.width() / size.x).min(inner.height() / size.y);
        let fitted = egui::Rect::from_center_size(inner.center(), size * scale);
        ui.painter().image(
            texture.id(),
            fitted,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else if let Some(scene) = scene {
        let revision = state.shell.library.thumb_revision;
        let builder =
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::centered_and_justified(
                    egui::Direction::TopDown,
                ));
        ui.allocate_new_ui(builder, |ui| {
            crate::views::files_thumbnail::show_fitted(
                state,
                ui,
                &entry.path,
                &scene,
                inner.size(),
                revision,
            );
        });
    } else {
        let side = (inner.height() * 0.6).min(96.0);
        let icon = egui::Rect::from_center_size(inner.center(), egui::vec2(side, side));
        super::files_icons::paint(ui.painter(), icon, entry.kind);
        if loading {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn fact_row(ui: &mut Ui, label: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    ui.horizontal_top(|ui| {
        ui.label(RichText::new(tr(label)).weak());
        ui.add(egui::Label::new(value).wrap());
    });
}

fn primary_action(entry: &Entry, data: &std::path::Path) -> Action {
    let in_autosave = autosave_slot(data, &entry.path).is_some();
    default_action(super::files_model::ItemCtx {
        kind: entry.kind,
        in_autosave,
        protected: false,
    })
}

fn show_item(state: &mut AppState, ui: &mut Ui, entry: &Entry) {
    ensure_preview(state, ui.ctx(), entry);
    let data = state.data_dir();
    thumbnail(state, ui, entry);
    ui.add_space(8.0);
    let preview = state
        .shell
        .library
        .preview
        .as_ref()
        .and_then(|slot| slot.preview.as_ref());
    let title = preview
        .filter(|preview| !preview.title.is_empty())
        .map_or_else(|| entry.name.clone(), |preview| preview.title.clone());
    let facts = preview
        .map(|preview| preview.facts.clone())
        .unwrap_or_default();
    let text = preview
        .map(|preview| preview.text.clone())
        .unwrap_or_default();
    ui.add(egui::Label::new(RichText::new(title).strong().size(16.0)).wrap());
    ui.label(RichText::new(tr(entry.kind.label())).weak());
    ui.add_space(6.0);
    if !entry.is_folder() {
        fact_row(ui, "Size", &format_size(entry.size));
    }
    fact_row(ui, "Modified", &format_modified(entry.modified));
    for (label, value) in &facts {
        let shown = if label == "Status" {
            tr(value)
        } else {
            value.clone()
        };
        fact_row(ui, label, &shown);
    }
    ui.add_space(8.0);
    let primary = primary_action(entry, &data);
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        let (label, action) = match primary {
            Action::LoadIntoAlas => ("Load into ALAS", Action::LoadIntoAlas),
            Action::Restore => ("Restore\u{2026}", Action::Restore),
            _ => ("Open", Action::Open),
        };
        if ui.add(crate::theme::primary_button(tr(label))).clicked() {
            chosen = Some(action);
        }
        if ui.button(tr("Show in Explorer")).clicked() {
            chosen = Some(Action::ShowInExplorer);
        }
    });
    if let Some(action) = chosen {
        run_action(state, ui.ctx(), action, std::slice::from_ref(entry));
    }
    if !text.is_empty() {
        ui.add_space(8.0);
        snippet(ui, &text);
    }
}

/// The first lines of the file that fit the room left. A line longer than the
/// pane wraps (its tail is never cut off); when more follows, a faded ellipsis
/// ends the text.
fn snippet(ui: &mut Ui, text: &str) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let font = egui::FontId::monospace(font.size);
    let line_height = ui.fonts(|fonts| fonts.row_height(&font)) + ui.spacing().item_spacing.y;
    let lines: Vec<&str> = text.lines().collect();
    let mut shown = 0;
    for (index, line) in lines.iter().enumerate() {
        // Keep one row free for the ellipsis while lines remain.
        let needed = if index + 1 < lines.len() { 2.0 } else { 1.0 };
        if shown > 0 && ui.available_height() < line_height * needed {
            break;
        }
        ui.add(egui::Label::new(RichText::new(*line).font(font.clone())).wrap());
        shown += 1;
    }
    if shown < lines.len() {
        ui.label(
            RichText::new("\u{2026}")
                .font(font)
                .color(ui.visuals().weak_text_color()),
        );
    }
}

fn show_folder(state: &mut AppState, ui: &mut Ui, entries: &[Entry]) {
    let current = state.shell.library.current();
    let name = current.file_name().map_or_else(
        || "ALAS".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let folders = entries.iter().filter(|entry| entry.is_folder()).count();
    let files = entries.len() - folders;
    let bytes: u64 = entries.iter().map(|entry| entry.size).sum();
    ui.add(egui::Label::new(RichText::new(name).strong().size(16.0)).wrap());
    ui.label(RichText::new(tr("File folder")).weak());
    ui.add_space(6.0);
    fact_row(ui, "Folders", &folders.to_string());
    fact_row(ui, "Files", &files.to_string());
    // Subfolders are listed, not measured: the sum is the files' alone.
    fact_row(ui, "Size of files", &format_size(bytes));
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(crate::library_window::platform_path(&current))
                .monospace()
                .small(),
        )
        .wrap(),
    );
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("New folder")).clicked() {
            super::files_menu::new_folder(state);
        }
        if ui.button(tr("Show in Explorer")).clicked() {
            super::files_menu::reveal(state, &current, true);
        }
    });
}

/// The pane's content.
pub(super) fn show_preview_pane(state: &mut AppState, ui: &mut Ui) {
    let (entries, order) = state.shell.library.view();
    let mut chosen: Vec<Entry> = state
        .shell
        .library
        .selected(&entries, &order)
        .into_iter()
        .cloned()
        .collect();
    // The preview follows what the list highlights: with nothing selected it
    // is the row the keyboard cursor stands on.
    if chosen.is_empty() {
        let cursor = state.shell.library.sel.cursor.clone();
        chosen.extend(
            order
                .iter()
                .map(|&index| &entries[index])
                .find(|entry| cursor.as_ref() == Some(&entry.path))
                .cloned(),
        );
    }
    ui.add_space(4.0);
    match chosen.as_slice() {
        [] => {
            state.shell.library.preview = None;
            let folder: Vec<Entry> = if state.shell.library.searching_subfolders() {
                Vec::new()
            } else {
                entries.as_ref().clone()
            };
            show_folder(state, ui, &folder);
        }
        [entry] => show_item(state, ui, entry),
        many => {
            let bytes: u64 = many.iter().map(|entry| entry.size).sum();
            ui.label(
                RichText::new(tr_fields(
                    "{count} items selected",
                    &[("count", many.len().to_string())],
                ))
                .strong()
                .size(16.0),
            );
            fact_row(ui, "Size of files", &format_size(bytes));
        }
    }
}
