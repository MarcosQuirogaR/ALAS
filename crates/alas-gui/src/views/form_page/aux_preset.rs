// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Auxiliary preset selector shared by schema-driven Advanced Settings pages.

use alas_config::{fidelity_presets, performance_presets, solver_presets};
use egui::{ComboBox, RichText, Ui};

use crate::nav::PresetKind;
use crate::state::AppState;
use crate::views::tr;

pub(super) fn show_aux_preset_picker(state: &mut AppState, ui: &mut Ui, kind: PresetKind) {
    let names: Vec<(String, String)> = match kind {
        PresetKind::Fidelity => fidelity_presets::display_names(),
        PresetKind::Solver => solver_presets::display_names(),
        PresetKind::Performance => performance_presets::display_names(),
    }
    .into_iter()
    .map(|(name, display)| (name.to_owned(), display.to_owned()))
    .collect();
    if names.is_empty() {
        return;
    }

    let selected = state
        .selected_aux_preset
        .get(kind.code())
        .cloned()
        .unwrap_or_default();
    let display = names
        .iter()
        .find(|(name, _)| *name == selected)
        .map(|(_, display)| display.clone())
        .unwrap_or_else(|| tr("Choose a preset..."));

    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Preset:")).strong());
        let mut apply = None;
        ComboBox::from_id_salt(format!("aux_preset::{}", kind.code()))
            .selected_text(display)
            .show_ui(ui, |ui| {
                for (name, display) in &names {
                    if ui.selectable_label(*name == selected, display).clicked() {
                        apply = Some(name.clone());
                    }
                }
            });
        if let Some(name) = apply {
            state.apply_aux_preset(kind, &name);
        }
    });
}
