// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One Advanced Settings page: title, description, an optional "How this
//! works" deep-dive, an optional aux-preset picker, the schema-driven form for
//! the page's configuration group, and an optional full-width live preview.
//!
//! A direct port of the reference desktop app's `FormPage` + `pages.ts` data.

use egui::{vec2, RichText, Ui};

use crate::nav::Page;
use crate::state::AppState;
use crate::views::form::{dynamic_form, dynamic_form_with_open_root_nodes, FormEdit};
use crate::views::tr;

mod aux_preset;
#[path = "mission_form.rs"]
mod mission_form;
#[path = "mission_profile_preview.rs"]
mod mission_profile_preview;
#[path = "form_page_sections.rs"]
mod sections;

use aux_preset::show_aux_preset_picker;
use mission_form::render_mission_form;
use sections::{page_sections, render_sectioned_form};

/// Render one Advanced Settings form page.
pub fn show_form_page(state: &mut AppState, ui: &mut Ui, page: &Page) {
    let Some(group) = page.group else { return };

    let heading = ui.heading(alas_i18n::t(Some(page.title), None));
    if let Some(desc) = page.description {
        heading.on_hover_text(alas_i18n::t(Some(desc), None));
    }
    if state.help_verbose {
        if let Some(desc) = page.description {
            ui.label(RichText::new(alas_i18n::t(Some(desc), None)).weak());
        }
    }
    ui.add_space(4.0);

    if state.help_verbose && !page.detail.is_empty() {
        egui::CollapsingHeader::new(tr("How this works"))
            .default_open(false)
            .show(ui, |ui| {
                for para in page.detail {
                    ui.label(alas_i18n::t(Some(para), None));
                    ui.add_space(4.0);
                }
            });
        ui.add_space(4.0);
    }

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
        if ui
            .add(egui::Button::new(tr("Reset page")).small().frame(false))
            .on_hover_text(tr(
                "Restore this page's default values; other pages are unchanged.",
            ))
            .clicked()
        {
            state.reset_group_to_defaults(group);
        }
    });
    ui.add_space(4.0);

    let visible_fields: Vec<alas_config::Field> = fields
        .iter()
        .filter(|field| !is_external_tools_field(group, field.name))
        .cloned()
        .collect();
    render_editor(state, ui, group, &visible_fields, &error_fields, lang);
    render_preview(state, ui, page.preview, page.preview_title);
}

/// Keep machine- and user-specific locations out of the model form. Their
/// values remain in the mission configuration at the pipeline boundary, but
/// Setup > External Tools is the single human-facing place to manage them.
fn is_external_tools_field(group: &str, name: &str) -> bool {
    group == "mission" && matches!(name, "navdata_dir" | "texture_path" | "routes_dir")
}

fn render_editor(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    fields: &[alas_config::Field],
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
) {
    // The page-level scroll area in `route_page` owns both the editor and its
    // preview so the preview cannot disappear behind the resizable run log.
    ui.vertical(|ui| {
        let show_help = state.help_verbose;
        let edits = if group == "mission" {
            render_mission_form(state, ui, fields, error_fields, lang, show_help)
        } else if let Some(values) = state.group_mut(group) {
            if group == "propulsion_cycle" {
                render_engine_designer_form(ui, fields, values, error_fields, lang, show_help)
            } else if let Some(sections) = page_sections(group) {
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
            } else if group == "optimizer" {
                // These are the two first-class halves of one optimizer, not
                // nested detail menus. Their direct sections should be ready
                // to scan when the page first opens.
                dynamic_form_with_open_root_nodes(
                    ui,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    true,
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

/// Group the otherwise flat turbofan-cycle inputs by their physical subsystem.
/// The model schema remains the single source of field metadata; this only
/// supplies visual hierarchy for a form with no nested configuration nodes.
fn render_engine_designer_form(
    ui: &mut Ui,
    fields: &[alas_config::Field],
    values: &mut serde_json::Value,
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    const GROUPS: [(&str, &[&str], bool); 4] = [
        (
            "Intake & compressors",
            &[
                "inlet_pressure_recovery",
                "lpc_pressure_ratio_split",
                "fan_polytropic_efficiency",
                "lpc_polytropic_efficiency",
                "hpc_polytropic_efficiency",
            ],
            true,
        ),
        (
            "Combustor",
            &["combustor_pressure_ratio", "combustor_efficiency"],
            false,
        ),
        (
            "Turbines",
            &[
                "hpt_polytropic_efficiency",
                "lpt_polytropic_efficiency",
                "turbine_mechanical_efficiency",
            ],
            false,
        ),
        (
            "Nozzles",
            &[
                "core_nozzle_pressure_ratio",
                "fan_nozzle_pressure_ratio",
                "core_nozzle_efficiency",
                "fan_nozzle_efficiency",
            ],
            false,
        ),
    ];

    let mut edits = Vec::new();
    for (title, names, default_open) in GROUPS {
        let section: Vec<alas_config::Field> = fields
            .iter()
            .filter(|field| names.contains(&field.name))
            .cloned()
            .collect();
        if section.is_empty() {
            continue;
        }
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr(title)).strong())
                .id_salt(format!("propulsion_cycle::{title}"))
                .default_open(default_open)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &section,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
        ui.add_space(6.0);
    }

    let remaining: Vec<alas_config::Field> = fields
        .iter()
        .filter(|field| {
            !GROUPS
                .iter()
                .any(|(_, names, _)| names.contains(&field.name))
        })
        .cloned()
        .collect();
    if !remaining.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Other parameters")).strong())
                .id_salt("propulsion_cycle::other")
                .default_open(false)
                .show(ui, |ui| {
                    edits.extend(dynamic_form(
                        ui,
                        &remaining,
                        values,
                        error_fields,
                        lang,
                        show_help,
                    ));
                });
        });
    }
    edits
}

fn render_preview(
    state: &mut AppState,
    ui: &mut Ui,
    preview: Option<&str>,
    preview_title: Option<&str>,
) {
    let preview_id = if let Some(preview) = preview {
        preview
    } else if state.active_page == "cabin" {
        // Cabin changes deserve immediate visual confirmation even though this
        // advanced page has no report-only figure.
        "cabin_3d"
    } else {
        return;
    };
    ui.add_space(8.0);
    let title = preview_title.unwrap_or(if preview_id == "cabin_3d" {
        "Seat and payload preview"
    } else {
        "Preview"
    });
    ui.label(RichText::new(alas_i18n::t(Some(title), None)).strong());
    match crate::scene::build_page_preview(state, preview_id) {
        Some(scene) => {
            let view_key = format!("page_preview::{preview_id}");
            let height = (ui.available_width() * 0.62).clamp(240.0, 420.0);
            let view = alas_viz::SceneView::new(&scene, state.view_state_mut(view_key))
                .static_view()
                .show_toolbar(false)
                .desired_size(vec2(ui.available_width().max(220.0), height));
            ui.add(view);
        }
        None => {
            ui.label(RichText::new(tr("Preview needs a completed run.")).weak());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_external_tools_field;

    #[test]
    fn mission_locations_are_managed_only_on_the_external_tools_page() {
        for name in ["navdata_dir", "texture_path", "routes_dir"] {
            assert!(is_external_tools_field("mission", name));
        }
        assert!(!is_external_tools_field("mission", "great_circle_points"));
        assert!(!is_external_tools_field("mses", "mses_dir"));
    }
}
