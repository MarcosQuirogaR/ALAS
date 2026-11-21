// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aircraft and engine selectors of the Inputs page. Both lists are
//! searchable, and the aircraft list ends with the user's saved "My aircraft".

use egui::{ComboBox, Id, Ui};
use serde_json::Value;

use crate::file_library::{self, ObjectKind};
use crate::sandbox::StartingDesign;
use crate::shell_confirm::Pending;
use crate::state::AppState;
use crate::views::{tr, tr_fields, tr_label};

/// The list a preset choice is waiting on while the user confirms leaving a
/// clean-sheet aircraft.
fn leave_clean_sheet_id() -> Id {
    Id::new("preset_leave_clean_sheet")
}

/// The preset field: a label above a combo box filling its column.
pub(super) fn show_preset_selector(state: &mut AppState, ui: &mut Ui) {
    crate::views::form::label_row(
        ui,
        crate::theme::field_label(&tr_label("Preset"), true),
        |_| {},
    );
    let names = state.preset_names.clone();
    let current_display = names
        .iter()
        .find(|(n, _)| *n == state.active_preset)
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| tr("Choose a preset..."));
    // A saved "My aircraft" variant is what is loaded until a built-in
    // preset is chosen again.
    let current_display = state
        .files
        .active_aircraft
        .clone()
        .map_or(current_display, |name| {
            format!("{} {name}", tr("My aircraft:"))
        });
    let mut chosen = None;
    let mut chosen_user = None;
    // Always enabled: after a clean sheet it is the way back to a reference
    // aircraft, and choosing one asks first (below).
    let clean_sheet = state.starting_design() != StartingDesign::PresetAircraft;
    {
        let combo = ComboBox::from_id_salt("inputs_preset_combo")
            .icon(crate::combo_icon::down_arrow)
            .width(ui.available_width())
            .truncate()
            .height(crate::selector_search::FIXED_SEARCH_COMBO_HEIGHT)
            .selected_text(current_display)
            .show_ui(ui, |ui| {
                let query = crate::selector_search::search_field(ui, "inputs_preset_combo");
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for (name, display) in &names {
                            if !crate::selector_search::matches(&query, &[name, display]) {
                                continue;
                            }
                            let selected = *name == state.active_preset
                                && state.files.active_aircraft.is_none();
                            if ui.selectable_label(selected, display).clicked() {
                                chosen = Some(name.clone());
                            }
                        }
                        chosen_user = crate::user_files_ui::my_aircraft_group(state, ui, &query);
                    });
            });
        crate::selector_search::reset_when_closed(ui, "inputs_preset_combo", &combo.inner);
    }
    let action = chosen
        .map(Pending::Preset)
        .or_else(|| chosen_user.map(Pending::UserAircraft));
    match action {
        // Leaving a clean sheet replaces a design that has no reference to
        // return to: confirm before it goes.
        Some(action) if clean_sheet && !state.sandbox.active() => {
            ui.ctx()
                .data_mut(|data| data.insert_temp(leave_clean_sheet_id(), Some(action)));
        }
        Some(action) => state.guarded(action),
        None => {}
    }
    if clean_sheet && !state.sandbox.active() {
        show_leave_clean_sheet_confirm(state, ui.ctx());
    }
}

/// The confirmation shown when a reference aircraft is chosen over a clean
/// sheet. Confirming loads it, which ends clean-sheet mode.
fn show_leave_clean_sheet_confirm(state: &mut AppState, ctx: &egui::Context) {
    let Some(Some(action)) =
        ctx.data(|data| data.get_temp::<Option<Pending>>(leave_clean_sheet_id()))
    else {
        return;
    };
    let name = match &action {
        Pending::Preset(name) => state
            .preset_names
            .iter()
            .find(|(key, _)| key == name)
            .map_or_else(|| name.clone(), |(_, display)| display.clone()),
        _ => tr("the saved aircraft"),
    };
    let body = tr_fields(
        "Load {name}? The clean-sheet aircraft is replaced by this reference aircraft.",
        &[("name", name)],
    );
    let decision = crate::shell_confirm::confirm_dialog(
        ctx,
        "leave_clean_sheet_window",
        &tr("Leave the clean sheet?"),
        Some(&body),
        &[crate::shell_confirm::ConfirmAction::primary(tr(
            "Load aircraft",
        ))],
    )
    .map(|answer| matches!(answer, crate::shell_confirm::ConfirmAnswer::Action(0)));
    match decision {
        Some(true) => {
            ctx.data_mut(|data| data.insert_temp(leave_clean_sheet_id(), None::<Pending>));
            state.perform(action);
        }
        Some(false) => {
            ctx.data_mut(|data| data.insert_temp(leave_clean_sheet_id(), None::<Pending>));
        }
        None => {}
    }
}

/// The user's saved engines, read from disk at most every two seconds.
fn my_engines(state: &AppState, ui: &Ui) -> Vec<file_library::LibraryEntry> {
    let id = egui::Id::new("my_engines_cache");
    let now = ui.input(|input| input.time);
    if let Some((read_at, entries)) = ui
        .ctx()
        .data(|data| data.get_temp::<(f64, Vec<file_library::LibraryEntry>)>(id))
    {
        if now - read_at < 2.0 {
            return entries;
        }
    }
    let entries = file_library::list(&state.data_dir(), ObjectKind::Engine);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, (now, entries.clone())));
    entries
}

/// The engine field: a label above a combo box filling its column.
pub(super) fn show_engine_selector(state: &mut AppState, ui: &mut Ui) {
    crate::views::form::label_row(
        ui,
        crate::theme::field_label(&tr_label("Engine"), true),
        |_| {},
    );
    let current_engine = state
        .config_values
        .get("geometry")
        .and_then(|g| g.get("engine"))
        .and_then(|e| e.get("engine_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    // The user's saved engines follow the catalogue, marked as theirs.
    let mine = my_engines(state, ui);
    let suffix = format!(" ({})", tr("My engine"));
    let mut names = state.engine_names.clone();
    names.extend(mine.iter().map(|entry| format!("{}{suffix}", entry.name)));
    let chosen_engine = crate::selector_search::named_list_combo(
        ui,
        "inputs_engine_combo",
        Some(ui.available_width()),
        &current_engine,
        &names,
        &current_engine,
    );
    if let Some(name) = chosen_engine {
        match name.strip_suffix(suffix.as_str()) {
            Some(own) => {
                if let Some(entry) = mine.iter().find(|entry| entry.name == own) {
                    state.guarded(crate::shell_confirm::Pending::Library(entry.clone()));
                }
            }
            None => state.set_engine(&name),
        }
    }
}

/// The name asked for by "Save as my engine..." while its window is open.
fn save_engine_id() -> Id {
    Id::new("save_my_engine_name")
}

/// "Save as my engine...": stores the current engine type in the user's
/// `library/my-engines` folder, where the selector above then lists it.
pub(super) fn show_save_engine_button(state: &mut AppState, ui: &mut Ui) {
    if ui
        .button(tr_label("Save as my engine\u{2026}"))
        .on_hover_text(tr(
            "Store the current engine under a name of your choice. It then appears in the Engine list, marked as yours.",
        ))
        .clicked()
    {
        let name = state.library_default_name(ObjectKind::Engine);
        ui.ctx()
            .data_mut(|data| data.insert_temp(save_engine_id(), Some(name)));
    }
    show_save_engine_window(state, ui.ctx());
}

fn show_save_engine_window(state: &mut AppState, ctx: &egui::Context) {
    let Some(Some(mut name)) = ctx.data(|data| data.get_temp::<Option<String>>(save_engine_id()))
    else {
        return;
    };
    let mut done = false;
    egui::Window::new(tr("Save as my engine"))
        .id(Id::new("save_my_engine_window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(tr("Name"));
            ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Cancel")).clicked() {
                    done = true;
                }
                let can_save = !name.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(tr("Save")))
                    .clicked()
                {
                    state.library_save(ObjectKind::Engine, name.trim(), "");
                    // The selector re-reads the folder at once.
                    ctx.data_mut(|data| {
                        data.remove::<(f64, Vec<file_library::LibraryEntry>)>(Id::new(
                            "my_engines_cache",
                        ));
                    });
                    done = true;
                }
            });
        });
    ctx.data_mut(|data| {
        data.insert_temp(save_engine_id(), if done { None } else { Some(name) });
    });
}

#[cfg(test)]
mod popup_layer_tests {
    use super::*;

    fn run_frame(
        ctx: &egui::Context,
        state: &mut AppState,
        events: Vec<egui::Event>,
    ) -> Vec<egui::epaint::ClippedShape> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::views::show_inputs_view(state, ui);
            });
        })
        .shapes
    }

    fn text_rect(text: &egui::epaint::TextShape) -> egui::Rect {
        egui::Rect::from_min_size(text.pos, text.galley.size())
    }

    /// Every filled rectangle of `shape`, looking into frames that carry a
    /// shadow (a `Shape::Vec`).
    fn rects(shape: &egui::Shape, out: &mut Vec<egui::epaint::RectShape>) {
        match shape {
            egui::Shape::Rect(rect) => out.push(rect.clone()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| rects(s, out)),
            _ => {}
        }
    }

    /// The Aircraft page with the preset list opened: from the very first
    /// frame the list is visible, an opaque frame sits under its entries, so
    /// the form beneath never shows through (egui fades a new popup in over
    /// `animation_time`, which showed the "Cabin preset" label through it).
    #[test]
    fn the_preset_list_is_opaque_from_its_first_visible_frame() {
        for theme in [crate::theme::AppTheme::Light, crate::theme::AppTheme::Dark] {
            let mut state = AppState::default();
            let ctx = egui::Context::default();
            crate::theme::apply_theme(theme, &ctx);
            let shapes = run_frame(&ctx, &mut state, vec![]);
            let shown = crate::views::tr(
                &state
                    .preset_names
                    .iter()
                    .find(|(key, _)| *key == state.active_preset)
                    .map(|(_, display)| display.clone())
                    .expect("the default case is a registered preset"),
            );
            let button = shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == shown => Some(text_rect(t)),
                    _ => None,
                })
                .expect("the selector shows the active preset");
            let at = button.center();
            let click = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let _ = run_frame(&ctx, &mut state, vec![egui::Event::PointerMoved(at)]);
            let _ = run_frame(&ctx, &mut state, vec![click(true)]);
            let mut frame = run_frame(&ctx, &mut state, vec![click(false)]);
            let mut first_visible = None;
            for _ in 0..6 {
                let entry = frame.iter().position(|s| {
                    matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.contains("A380"))
                });
                if let Some(index) = entry {
                    first_visible = Some((frame.clone(), index));
                    break;
                }
                frame = run_frame(&ctx, &mut state, vec![]);
            }
            let (shapes, entry_index) = first_visible.expect("the open list draws its entries");
            let entry_rect = match &shapes[entry_index].shape {
                egui::Shape::Text(t) => text_rect(t),
                _ => unreachable!(),
            };
            let mut candidates = Vec::new();
            for s in &shapes[..entry_index] {
                rects(&s.shape, &mut candidates);
            }
            let list_frame = candidates
                .iter()
                .filter(|r| r.rect.contains_rect(entry_rect) && r.rect.width() < 600.0)
                .last()
                .unwrap_or_else(|| panic!("{theme:?}: no frame under the list entries"));
            assert_eq!(
                list_frame.fill.a(),
                255,
                "{theme:?}: the list frame is translucent on its first visible frame"
            );
        }
    }
}
