// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The navigation bar of the Files explorer: Back, Forward and Up, the
//! address bar (clickable breadcrumbs, or the path as editable text) and the
//! search box.

use egui::{Color32, Key, Rounding, Sense, Stroke, TextEdit, Ui};

use super::files_model::{breadcrumbs, resolve_typed, up_target, PathError};
use super::files_state::AddressEdit;
use super::{tr, AppState, PathBuf};

const BAR_HEIGHT: f32 = 28.0;
const SEARCH_WIDTH: f32 = 210.0;

#[derive(Clone, Copy)]
enum Arrow {
    Back,
    Forward,
    Up,
}

/// A square button with a drawn arrow.
fn arrow_button(ui: &mut Ui, arrow: Arrow, enabled: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(BAR_HEIGHT, BAR_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let visuals = ui.visuals();
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::same(6.0),
            visuals.widgets.hovered.weak_bg_fill,
        );
    }
    let color = if enabled {
        visuals.text_color()
    } else {
        visuals.weak_text_color().gamma_multiply(0.6)
    };
    let stroke = Stroke::new(1.8_f32, color);
    let c = rect.center();
    let r = 5.0;
    let points = match arrow {
        Arrow::Back => [
            c + egui::vec2(r, 0.0),
            c + egui::vec2(-r, 0.0),
            c + egui::vec2(-r * 0.2, -r * 0.8),
            c + egui::vec2(-r, 0.0),
            c + egui::vec2(-r * 0.2, r * 0.8),
        ],
        Arrow::Forward => [
            c + egui::vec2(-r, 0.0),
            c + egui::vec2(r, 0.0),
            c + egui::vec2(r * 0.2, -r * 0.8),
            c + egui::vec2(r, 0.0),
            c + egui::vec2(r * 0.2, r * 0.8),
        ],
        Arrow::Up => [
            c + egui::vec2(0.0, r),
            c + egui::vec2(0.0, -r),
            c + egui::vec2(-r * 0.8, -r * 0.2),
            c + egui::vec2(0.0, -r),
            c + egui::vec2(r * 0.8, -r * 0.2),
        ],
    };
    ui.painter()
        .add(egui::Shape::line(points[..2].to_vec(), stroke));
    ui.painter()
        .add(egui::Shape::line(points[2..].to_vec(), stroke));
    response.on_hover_text(tip).clicked() && enabled
}

fn chevron(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 20.0), Sense::hover());
    let c = rect.center();
    let stroke = Stroke::new(1.2_f32, ui.visuals().weak_text_color());
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(-2.0, -4.0),
            c + egui::vec2(2.0, 0.0),
            c + egui::vec2(-2.0, 4.0),
        ],
        stroke,
    ));
}

/// The whole bar.
pub(super) fn show_nav_bar(state: &mut AppState, ui: &mut Ui) {
    let (can_back, can_forward, current, roots) = {
        let lib = &state.shell.library;
        (
            lib.history.as_ref().is_some_and(|h| h.can_back()),
            lib.history.as_ref().is_some_and(|h| h.can_forward()),
            lib.current(),
            lib.roots.clone(),
        )
    };
    let up = up_target(&current, &roots);
    let total = ui.available_width();
    let address_width = (total - 3.0 * BAR_HEIGHT - SEARCH_WIDTH - 70.0 - 5.0 * 8.0).max(160.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if arrow_button(ui, Arrow::Back, can_back, &tr("Back (Alt+Left)")) {
            state.shell.library.step(false);
        }
        if arrow_button(ui, Arrow::Forward, can_forward, &tr("Forward (Alt+Right)")) {
            state.shell.library.step(true);
        }
        if arrow_button(ui, Arrow::Up, up.is_some(), &tr("Up one level (Backspace)")) {
            if let Some(parent) = up.clone() {
                state.shell.library.go(parent);
            }
        }
        ui.add_space(4.0);
        show_address(state, ui, address_width, &current, &roots);
        ui.add_space(8.0);
        show_search(state, ui, &current);
    });
}

fn show_address(
    state: &mut AppState,
    ui: &mut Ui,
    width: f32,
    current: &std::path::Path,
    roots: &[PathBuf],
) {
    let frame = egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(4.0, 2.0));
    frame.show(ui, |ui| {
        ui.set_width(width - 10.0);
        ui.set_height(BAR_HEIGHT - 6.0);
        if state.shell.library.address.is_some() {
            edit_address(state, ui, current, roots);
        } else {
            show_crumbs(state, ui, current, roots);
        }
    });
}

fn edit_address(state: &mut AppState, ui: &mut Ui, current: &std::path::Path, roots: &[PathBuf]) {
    let Some(edit) = state.shell.library.address.as_mut() else {
        return;
    };
    let mut text_color = ui.visuals().text_color();
    if edit.refused {
        text_color = Color32::from_rgb(255, 154, 147);
    }
    let output = TextEdit::singleline(&mut edit.text)
        .frame(false)
        .desired_width(f32::INFINITY)
        .text_color(text_color)
        .show(ui);
    let response = output.response;
    if edit.fresh {
        edit.fresh = false;
        response.request_focus();
        let end = edit.text.chars().count();
        let mut text_state = output.state;
        text_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
        text_state.store(ui.ctx(), response.id);
        return;
    }
    if response.changed() {
        edit.refused = false;
    }
    let enter = ui.input(|input| input.key_pressed(Key::Enter));
    let escape = ui.input(|input| input.key_pressed(Key::Escape));
    if response.lost_focus() && enter {
        let typed = edit.text.clone();
        match resolve_typed(&typed, current, roots) {
            Ok(path) => {
                state.shell.library.address = None;
                if path.is_dir() {
                    state.shell.library.go(path);
                } else if let Some(parent) = path.parent() {
                    state.shell.library.go(parent.to_path_buf());
                    state.shell.library.reveal = Some((path, false));
                }
            }
            Err(error) => {
                let message = match error {
                    PathError::Empty => "Type a folder path.",
                    PathError::Outside => {
                        "Only the ALAS data folder and the library folders can be browsed here."
                    }
                    PathError::Missing => "That folder does not exist.",
                };
                state.files.status = Some(tr(message));
                if let Some(edit) = state.shell.library.address.as_mut() {
                    edit.refused = true;
                }
                response.request_focus();
            }
        }
    } else if escape || response.lost_focus() {
        state.shell.library.address = None;
    }
}

fn show_crumbs(state: &mut AppState, ui: &mut Ui, current: &std::path::Path, roots: &[PathBuf]) {
    let crumbs = breadcrumbs(current, roots);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width_of = |ui: &Ui, text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            + 14.0
    };
    // Keep the deepest folders that fit; the rest sit behind a "..." menu.
    let room = ui.available_width() - 40.0;
    let mut used = 0.0;
    let mut first = crumbs.len();
    for (index, (label, _)) in crumbs.iter().enumerate().rev() {
        used += width_of(ui, label) + 10.0;
        if used > room && index + 1 < crumbs.len() {
            break;
        }
        first = index;
    }
    let mut jump: Option<PathBuf> = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if first > 0 {
            ui.menu_button("\u{2026}", |ui| {
                for (label, path) in &crumbs[..first] {
                    if ui.button(label).clicked() {
                        jump = Some(path.clone());
                        ui.close_menu();
                    }
                }
            });
            chevron(ui);
        }
        for (index, (label, path)) in crumbs.iter().enumerate().skip(first) {
            if index > first {
                chevron(ui);
            }
            let last = index + 1 == crumbs.len();
            let text = if last {
                egui::RichText::new(label).strong()
            } else {
                egui::RichText::new(label)
            };
            let button = egui::Button::new(text)
                .frame(false)
                .min_size(egui::vec2(0.0, 20.0));
            if ui
                .add(button)
                .on_hover_text(path.display().to_string())
                .clicked()
            {
                jump = Some(path.clone());
            }
        }
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(6.0), 20.0),
            Sense::click(),
        );
        let _ = rect;
        if response.on_hover_text(tr("Click to type a path")).clicked() {
            state.shell.library.address = Some(AddressEdit {
                text: current.display().to_string(),
                fresh: true,
                refused: false,
            });
        }
    });
    if let Some(path) = jump {
        state.shell.library.go(path);
    }
}

fn show_search(state: &mut AppState, ui: &mut Ui, current: &std::path::Path) {
    let folder = current
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hint = crate::views::tr_fields("Search {folder}", &[("folder", folder)]);
    let lib = &mut state.shell.library;
    let id = egui::Id::new("files_search_box");
    ui.add(
        TextEdit::singleline(&mut lib.query)
            .id(id)
            .hint_text(hint)
            .desired_width(SEARCH_WIDTH - 70.0)
            .margin(egui::vec2(6.0, 5.0)),
    );
    let label = tr("Subfolders");
    if ui
        .add(crate::theme::selectable_button(label, lib.search_sub))
        .on_hover_text(tr("Also search the folders inside this one."))
        .clicked()
    {
        lib.search_sub = !lib.search_sub;
    }
}
