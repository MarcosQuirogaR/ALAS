// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drawing the Add-ons page.

use alas_addons::{AddonKind, InstalledAddon};
use egui::{Color32, Grid, RichText, ScrollArea, Ui};

use crate::state::AppState;
use crate::views::tr;

const WARNING: Color32 = Color32::from_rgb(214, 90, 49);

/// Render the Add-ons page.
pub(crate) fn show_addons_view(state: &mut AppState, ui: &mut Ui) {
    state.ensure_addons_loaded();
    if state.addons.is_running() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    ScrollArea::vertical()
        .id_salt("addons_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading(tr("Add-ons"));
            ui.label(tr(
                "Small calculators you can add or edit: a Rhai script (sandboxed) or an external program. They read a summary of the current design and may write a result back, only when you click Apply to design.",
            ));
            ui.add_space(6.0);
            toolbar(state, ui);
            ui.add_space(6.0);
            for (path, problem) in &state.addons.discovery.problems {
                ui.colored_label(
                    WARNING,
                    format!("{}: {problem}", path.display()),
                );
            }
            let addons = state.addons.discovery.addons.clone();
            if addons.is_empty() {
                ui.label(tr("No add-ons found in the add-ons folder."));
            }
            for addon in &addons {
                let selected = state.addons.selected.as_deref() == Some(addon.id.as_str());
                let label = format!(
                    "{}  v{}  [{}]",
                    addon.manifest.name,
                    addon.manifest.version,
                    addon.manifest.kind.as_str()
                );
                if ui.selectable_label(selected, label).clicked() {
                    state.addons.selected = Some(addon.id.clone());
                    state.addons.preview = None;
                    state.addons.status = None;
                }
            }
            ui.separator();
            let selected = state
                .addons
                .selected
                .as_ref()
                .and_then(|id| addons.iter().find(|addon| &addon.id == id));
            if let Some(addon) = selected {
                detail(state, ui, addon);
            }
            applied_list(state, ui);
        });
}

fn toolbar(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("Open add-ons folder"))
            .on_hover_text(state.addons.dir.display().to_string())
            .clicked()
        {
            let _ = std::fs::create_dir_all(&state.addons.dir);
            let path = state.addons.dir.display().to_string();
            if let Err(error) = crate::views::tools_view::open_in_file_explorer(&path, true) {
                state.addons.status = Some(error);
            }
        }
        if ui.button(tr("Reload")).clicked() {
            state.reload_addons();
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Add-ons folder"));
        ui.monospace(state.addons.dir.display().to_string());
    });
    ui.horizontal(|ui| {
        ui.label(tr("Python interpreter for process add-ons"));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.addons.python_text)
                .hint_text("python")
                .desired_width(280.0),
        );
        if response.lost_focus() {
            let text = state.addons.python_text.trim().to_owned();
            state.addons.settings.python = (!text.is_empty()).then_some(text);
            state.save_addon_settings();
        }
    });
}

fn detail(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    let manifest = &addon.manifest;
    ui.label(RichText::new(&manifest.name).strong().size(16.0));
    ui.label(format!(
        "v{}   {}   {}",
        manifest.version,
        manifest.author,
        manifest.kind.as_str()
    ));
    ui.label(&manifest.description);
    ui.add_space(4.0);

    let process = manifest.kind == AddonKind::Process;
    if process {
        ui.colored_label(
            WARNING,
            tr("Warning: a process add-on runs an external program with your user rights and can read or change any file you can. Enable it only if you trust its author and have read its code."),
        );
    }
    let mut enabled = state
        .addons
        .settings
        .is_enabled(&addon.id, &manifest.version, manifest.kind);
    let label = if process {
        tr("I trust this add-on and enable it (this version)")
    } else {
        tr("Enabled")
    };
    if ui.checkbox(&mut enabled, label).changed() {
        state
            .addons
            .settings
            .set_enabled(&addon.id, &manifest.version, enabled);
        state.save_addon_settings();
    }
    ui.add_space(6.0);

    ui.label(RichText::new(tr("Inputs")).strong());
    Grid::new("addon_inputs").num_columns(3).show(ui, |ui| {
        for spec in &manifest.inputs {
            let key = format!("{}/{}", addon.id, spec.name);
            let text = state
                .addons
                .input_text
                .entry(key)
                .or_insert_with(|| format!("{}", spec.default));
            ui.label(&spec.name).on_hover_text(&spec.description);
            ui.add(egui::TextEdit::singleline(text).desired_width(110.0));
            ui.label(format!("{}  {}", spec.unit, spec.description));
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let can_run = enabled && !state.addons.is_running();
        if ui
            .add_enabled(can_run, egui::Button::new(tr("Run")))
            .clicked()
        {
            state.run_selected_addon();
        }
        if state.addons.is_running() {
            ui.spinner();
        }
    });
    if let Some(status) = state.addons.status.clone() {
        ui.label(status);
    }
    results(state, ui, addon);
}

fn results(state: &mut AppState, ui: &mut Ui, addon: &InstalledAddon) {
    let Some(last) = state
        .addons
        .last
        .as_ref()
        .filter(|last| last.id == addon.id)
    else {
        return;
    };
    ui.add_space(6.0);
    ui.label(RichText::new(tr("Outputs")).strong());
    let outcome = match &last.result {
        Ok(outcome) => outcome.clone(),
        Err(error) => {
            ui.colored_label(WARNING, error);
            return;
        }
    };
    let mut has_targets = false;
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
    });
    for message in &outcome.messages {
        ui.label(message);
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

fn applied_list(state: &AppState, ui: &mut Ui) {
    if state.addons.applied().is_empty() {
        return;
    }
    ui.separator();
    ui.label(RichText::new(tr("Values set by add-ons (recorded with the next run)")).strong());
    for record in state.addons.applied() {
        ui.label(record.provenance_line());
    }
}
