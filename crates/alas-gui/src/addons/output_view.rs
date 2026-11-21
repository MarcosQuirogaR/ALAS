// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The outputs of the last run: numbers, texts, result-file links, and the
//! preview-and-apply flow for targeted outputs and configuration overlays.

use alas_addons::InstalledAddon;
use egui::{Grid, RichText, Ui};

use super::files;
use super::view::WARNING;
use crate::state::AppState;
use crate::views::{tr, tr_fields};

pub(super) fn show(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    let Some(last) = state
        .addons
        .last
        .as_ref()
        .filter(|last| last.id == addon.id)
    else {
        return;
    };
    ui.add_space(6.0);
    let outcome = match &last.result {
        Ok(outcome) => outcome.clone(),
        Err(error) if error == "cancelled" => {
            ui.label(RichText::new(tr("The last run was cancelled.")).weak());
            return;
        }
        Err(error) => {
            ui.colored_label(WARNING, error);
            return;
        }
    };
    ui.label(RichText::new(tr("Outputs")).strong());
    let mut has_targets = outcome.config_overlay.is_some();
    Grid::new("addon_outputs").num_columns(4).show(ui, |ui| {
        for spec in &addon.manifest.outputs {
            ui.label(&spec.name).on_hover_text(&spec.description);
            let value = outcome.outputs.get(&spec.name).copied().unwrap_or(f64::NAN);
            ui.monospace(format!("{value:.6}"));
            ui.label(&spec.unit);
            match &spec.target {
                Some(target) => {
                    has_targets = true;
                    ui.label(format!("-> {target}"));
                }
                None => {
                    ui.label(tr("display only"));
                }
            }
            ui.end_row();
        }
        for (name, text) in &outcome.texts {
            ui.label(name);
            ui.label(text);
            ui.label("");
            ui.label(tr("display only"));
            ui.end_row();
        }
    });
    for message in &outcome.messages {
        ui.label(message);
    }
    if !outcome.files.is_empty() {
        ui.label(RichText::new(tr("Result files")).strong());
        for file in &outcome.files {
            if files::is_openable_result(&file.path) {
                if ui
                    .link(&file.label)
                    .on_hover_text(file.path.display().to_string())
                    .clicked()
                {
                    if let Err(error) = files::open_result(&file.path) {
                        state.addons.status = Some(error);
                    }
                }
            } else {
                ui.label(RichText::new(&file.label).weak())
                    .on_hover_text(tr_fields(
                        "{path} is missing or is a program, so it is not opened from here.",
                        &[("path", file.path.display().to_string())],
                    ));
            }
        }
    }
    if let Some(overlay) = &outcome.config_overlay {
        ui.label(tr_fields(
            "Configuration overlay returned: {file}",
            &[(
                "file",
                overlay.file_name().map_or_else(
                    || overlay.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                ),
            )],
        ));
    }
    if !outcome.log.trim().is_empty() {
        ui.collapsing(tr("Add-on log"), |ui| {
            ui.monospace(outcome.log.trim());
        });
    }
    if !has_targets {
        ui.label(tr(
            "None of this add-on's outputs targets a design field, so nothing can be applied.",
        ));
        return;
    }
    if ui.button(tr("Preview changes")).clicked() {
        state.preview_addon_changes();
    }
    let Some(rows) = state.addons.preview.clone() else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    Grid::new("addon_preview").num_columns(4).show(ui, |ui| {
        ui.label(RichText::new(tr("Field")).strong());
        ui.label(RichText::new(tr("Before")).strong());
        ui.label(RichText::new(tr("After")).strong());
        ui.label(RichText::new(tr("Unit")).strong());
        ui.end_row();
        for row in &rows {
            ui.label(&row.target);
            ui.monospace(
                row.before
                    .map_or_else(|| tr("unset"), |value| format!("{value}")),
            );
            ui.monospace(format!("{}", row.after));
            ui.label(&row.unit);
            ui.end_row();
        }
    });
    ui.label(tr(
        "The value is written as is, in the unit of the target field. The design is checked before the change is kept, and the run records it as set by this add-on.",
    ));
    if ui.button(tr("Apply to design")).clicked() {
        state.apply_addon_changes();
    }
}
