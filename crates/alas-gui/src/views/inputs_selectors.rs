// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aircraft and engine selectors of the Inputs page. Both lists are
//! searchable, and the aircraft list ends with the user's saved "My aircraft".

use egui::{ComboBox, Ui};
use serde_json::Value;

use crate::sandbox::StartingDesign;
use crate::state::AppState;
use crate::views::tr;

/// The preset selector: `Preset:` and its combo box.
pub(super) fn show_preset_selector(state: &mut AppState, ui: &mut Ui) {
    ui.label(tr("Preset:"));
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
    let preset_mode = state.starting_design() == StartingDesign::PresetAircraft;
    ui.add_enabled_ui(preset_mode, |ui| {
        let combo = ComboBox::from_id_salt("inputs_preset_combo")
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
    });
    if let Some(name) = chosen {
        state.load_preset(&name);
    }
    if let Some(path) = chosen_user {
        state.load_user_aircraft(&path);
    }
}

/// The engine selector: `Engine:` and its combo box.
pub(super) fn show_engine_selector(state: &mut AppState, ui: &mut Ui) {
    ui.label(tr("Engine:"));
    let current_engine = state
        .config_values
        .get("geometry")
        .and_then(|g| g.get("engine"))
        .and_then(|e| e.get("engine_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let chosen_engine = crate::selector_search::named_list_combo(
        ui,
        "inputs_engine_combo",
        None,
        &current_engine,
        &state.engine_names,
        &current_engine,
    );
    if let Some(name) = chosen_engine {
        state.set_engine(&name);
    }
}
