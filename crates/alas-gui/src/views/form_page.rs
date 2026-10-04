// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One Advanced Settings page: title, description, an optional "How this
//! works" deep-dive, an optional aux-preset picker, the schema-driven form for
//! the page's configuration group, and an optional full-width live preview.

mod aux_preset;
mod cabin_editor;
mod engine;
mod mission_form;
mod optimizer_reset;
pub(crate) mod placement;
mod preview;
mod sections;
#[cfg(test)]
mod tests;

use egui::{RichText, Ui};

use crate::nav::{Page, Surface};
use crate::state::AppState;
use crate::views::form::dynamic_form;
use crate::views::tr;

use aux_preset::show_aux_preset_picker;
use engine::{
    engine_editor_model, render_engine_designer_form, render_engine_physics_summary,
    EngineEditorModel,
};
use mission_form::render_mission_form;
use preview::render_preview;
use sections::{page_sections, render_sectioned_form};

/// The notice a page carries while a registered preset protects its geometry.
pub const PRESET_LOCK_NOTICE: &str =
    "Preset geometry is protected from manual edits here as well; open the sandbox for geometry experiments.";

/// Render one Advanced Settings form page.
pub fn show_form_page(state: &mut AppState, ui: &mut Ui, page: &Page) {
    show_form_page_locked(state, ui, page, false);
}

/// Render one Advanced Settings form page, with its editors optionally locked.
///
/// Locking a page is not done by wrapping it whole in `add_enabled_ui(false)`. egui's
/// disabled scope fades every painted colour toward the background, so the page
/// title, its description, every field *label* and the page actions all dropped
/// to the disabled token together (measured 4.28-4.68:1 against 12-15:1 on an
/// active page) and the page read as failed to load rather than as protected.
/// Only the editors are disabled here. A short lock status stays under the
/// title while its explanation is available on hover.
pub fn show_form_page_locked(state: &mut AppState, ui: &mut Ui, page: &Page, locked: bool) {
    let Some(group) = page.group else { return };
    placement::sync_preview_tab(state, ui.ctx(), page);

    let heading = ui.heading(alas_i18n::t(Some(page.title), None));
    if page.description.is_some() || !page.detail.is_empty() {
        heading.on_hover_ui(|ui| {
            ui.set_max_width(480.0);
            if let Some(desc) = page.description {
                ui.label(alas_i18n::t(Some(desc), None));
            }
            for (index, para) in page.detail.iter().enumerate() {
                if page.description.is_some() || index > 0 {
                    ui.add_space(4.0);
                }
                ui.label(alas_i18n::t(Some(para), None));
            }
        });
    }
    if locked {
        ui.add_space(2.0);
        ui.label(
            RichText::new(tr("Preset geometry locked"))
                .color(ui.visuals().warn_fg_color)
                .small(),
        )
        .on_hover_text(tr(PRESET_LOCK_NOTICE));
    }
    ui.add_space(4.0);

    if let Some(kind) = page.preset_kind {
        show_aux_preset_picker(state, ui, kind);
        ui.add_space(4.0);
    }

    ui.add_space(8.0);

    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|f| f.field_path.starts_with(group))
        .map(|f| {
            f.field_path
                .rsplit('.')
                .next()
                .unwrap_or(&f.field_path)
                .to_owned()
        })
        .collect();
    let lang = Some(state.language.code());

    let group_node = state.schema.field(group).and_then(|f| match &f.entry {
        alas_config::Entry::Node(n) => Some(n.fields.clone()),
        _ => None,
    });
    let Some(fields) = group_node else { return };

    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Parameter values")).strong());
        // A framed button, not frameless text: "Reset page" changes every
        // value on the page and must look like the action it is.
        if ui
            .add_enabled(!locked, egui::Button::new(tr("Reset page")).small())
            .on_hover_text(tr(
                "Restore this page's default values; other pages are unchanged.",
            ))
            .clicked()
        {
            reset_page_to_defaults(state, page, group);
        }
    });
    ui.add_space(4.0);

    let visible_fields = placement::visible_fields(page, group, &fields);
    ui.add_enabled_ui(!locked, |ui| {
        if page.id == "mass_advanced" {
            crate::views::inputs_mtow::show_mtow_controls(state, ui);
            ui.separator();
        }
        render_editor(
            state,
            ui,
            group,
            page.surface,
            &visible_fields,
            &error_fields,
            lang,
        );
        placement::render_extra_sections(state, ui, page, &error_fields, lang);
    });
    if group == "mission" && page.surface == Surface::Advanced {
        crate::views::mission_profile_inputs::show_mission_profile_advanced(state, ui);
    }
    render_preview(state, ui, page.preview, page.preview_title);
}

pub(super) fn reset_page_to_defaults(state: &mut AppState, page: &Page, group: &str) {
    if group == "optimizer" {
        optimizer_reset::reset(state, page);
        return;
    }
    if page.id == "mass_advanced" {
        reset_mtow_fields(state);
    }
    if page.id == "propulsion_advanced" {
        reset_relocated_propulsion_fields(state);
    }
    if group == "mass_model" {
        // Modeling Mass and Advanced Mass own disjoint top-level fields in
        // this schema. Preserve the fields hidden on this surface, including
        // the two mass factors edited on Propulsion.
        let Some(alas_config::Entry::Node(node)) =
            state.schema.field(group).map(|field| &field.entry)
        else {
            return;
        };
        let visible = placement::visible_fields(page, group, &node.fields);
        let visible_names: std::collections::HashSet<&str> =
            visible.iter().map(|field| field.name).collect();
        let preserve: Vec<&str> = node
            .fields
            .iter()
            .filter(|field| !visible_names.contains(field.name))
            .map(|field| field.name)
            .collect();
        state.reset_group_to_defaults_preserving(group, &preserve);
    } else {
        state.reset_group_to_defaults(group);
    }
}

/// Restore the MTOW controls to the registered aircraft, or generic defaults.
fn reset_mtow_fields(state: &mut AppState) {
    let preset = state
        .config_values
        .get("preset")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let defaults = if alas_config::presets::get(preset).is_ok() {
        alas_config::AlasConfig::from_value(&serde_json::json!({"preset": preset}))
            .unwrap_or_default()
    } else {
        alas_config::AlasConfig::default()
    };
    if let Some(requirements) = state
        .group_mut("requirements")
        .and_then(serde_json::Value::as_object_mut)
    {
        requirements.insert(
            "mtow_kg".to_owned(),
            serde_json::json!(defaults.requirements.mtow_kg),
        );
    }
    if let Some(optimizer) = state
        .group_mut("optimizer")
        .and_then(serde_json::Value::as_object_mut)
    {
        let Some(objective) = optimizer
            .entry("objective")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
        else {
            return;
        };
        for (field, value) in [
            (
                "mtow_sizing",
                serde_json::json!(defaults.optimizer.objective.mtow_sizing),
            ),
            (
                "mtow_target_kg",
                serde_json::json!(defaults.optimizer.objective.mtow_target_kg),
            ),
            (
                "mtow_band_fraction",
                serde_json::json!(defaults.optimizer.objective.mtow_band_fraction),
            ),
            (
                "design_range_nmi",
                serde_json::json!(defaults.optimizer.objective.design_range_nmi),
            ),
        ] {
            objective.insert(field.to_owned(), value);
        }
    }
}

/// Reset the mass-model inputs edited on Propulsion without touching Mass's
/// transport, structure, or high-lift settings.
fn reset_relocated_propulsion_fields(state: &mut AppState) {
    let Ok(defaults) = serde_json::to_value(alas_config::AlasConfig::default()) else {
        return;
    };
    let Some(mass) = state
        .config_values
        .pointer_mut("/mass_model")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for name in ["propulsion_twr_factor", "propulsion_installation_factor"] {
        let pointer = format!("/mass_model/{name}");
        if let Some(default) = defaults.pointer(&pointer) {
            mass.insert(name.to_owned(), default.clone());
        } else {
            // A missing default means the override must be removed.
            mass.remove(name);
        }
    }
}

fn render_editor(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    surface: Surface,
    fields: &[alas_config::Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
) {
    // The page-level scroll area in `route_page` owns both the editor and its
    // preview so the preview cannot disappear behind the resizable run log.
    ui.vertical(|ui| {
        let show_help = false;
        let edits = if group == "mission" && surface != Surface::Advanced {
            render_mission_form(state, ui, fields, error_fields, lang, show_help)
        } else if group == "propulsion_cycle" {
            placement::render_propulsion_editor(
                state,
                ui,
                surface,
                fields,
                error_fields,
                lang,
                show_help,
            )
        } else if group == "cabin" {
            let custom_preset = state
                .config_values
                .pointer("/requirements/cabin_preset")
                .and_then(serde_json::Value::as_str)
                == Some("Custom");
            if let Some(values) = state.group_mut(group) {
                cabin_editor::render_cabin_editor(
                    ui,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    custom_preset,
                )
            } else {
                Vec::new()
            }
        } else if let Some(values) = state.group_mut(group) {
            if let Some(sections) = page_sections(group, surface) {
                render_sectioned_form(
                    ui,
                    group,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    sections,
                )
            } else {
                dynamic_form(ui, fields, values, error_fields, lang, show_help)
            }
        } else {
            Vec::new()
        };
        if !edits.is_empty() {
            state.on_config_modified();
            for edit in edits {
                state.note_parameter_modified(edit.label, edit.value);
            }
        }
    });
}
