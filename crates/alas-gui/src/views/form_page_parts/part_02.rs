// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    use super::{engine_editor_model, is_external_tools_field, EngineEditorModel};

    #[test]
    fn mission_locations_are_managed_only_on_the_external_tools_page() {
        for name in ["navdata_dir", "texture_path", "routes_dir"] {
            assert!(is_external_tools_field("mission", name));
        }
        assert!(!is_external_tools_field("mission", "great_circle_points"));
        assert!(!is_external_tools_field("mses", "mses_dir"));
    }

    #[test]
    fn pw127m_uses_the_shaft_power_editor_payload() {
        let mut engine = alas_config::EngineConfig {
            engine_name: "PW127M".to_owned(),
            ..Default::default()
        };
        engine.try_apply_engine_spec().expect("PW127M binding");

        let model = engine_editor_model(&engine).expect("supported technology");
        let EngineEditorModel::Turboprop {
            propeller_model,
            takeoff_kw,
            provenance,
            ..
        } = model
        else {
            panic!("PW127M must not expose turbofan controls");
        };
        assert_eq!(propeller_model, "Hamilton Sundstrand 568F-1");
        assert!(takeoff_kw > 1_800.0);
        assert!(provenance.contains("EASA"));
    }

    #[test]
    fn default_engine_preserves_the_turbofan_editor() {
        assert!(matches!(
            engine_editor_model(&alas_config::EngineConfig::default()),
            Ok(EngineEditorModel::Turbofan { .. })
        ));
    }
}

