// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Group the otherwise flat cycle inputs by their physical subsystem.
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
    const GROUPS: [(&str, &[&str], bool); 5] = [
        (
            "Cycle design inputs",
            &[
                "turboprop_overall_pressure_ratio",
                "turboprop_turbine_inlet_temperature_k",
            ],
            true,
        ),
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
            true,
        ),
        (
            "Turbines",
            &[
                "hpt_polytropic_efficiency",
                "lpt_polytropic_efficiency",
                "turbine_mechanical_efficiency",
            ],
            true,
        ),
        (
            "Nozzles",
            &[
                "core_nozzle_pressure_ratio",
                "fan_nozzle_pressure_ratio",
                "core_nozzle_efficiency",
                "fan_nozzle_efficiency",
            ],
            true,
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
                .default_open(true)
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

/// The smallest and largest height a page preview is drawn at, in points.
const PREVIEW_MIN_HEIGHT: f32 = 240.0;
const PREVIEW_MAX_HEIGHT: f32 = 480.0;

/// The size a page preview widget is given, in points.
///
/// The widget used to take the container's full width with a height capped at
/// 420 points, so a wide Advanced Settings window drew the three-view schematic
/// height-limited inside a box four times wider than the drawing: about 72% of
/// the container was empty background and the axis labels rendered 4-7 px tall.
/// Sizing the widget to the *scene's own aspect* makes the drawing as large as
/// the height budget allows and removes the empty band, because the widget is
/// no longer wider than what it draws.
fn preview_size(available_width: f32, screen_height: f32, scene: (f64, f64)) -> egui::Vec2 {
    let available_width = available_width.max(220.0);
    let aspect = if scene.1 > 0.0 && scene.0 > 0.0 {
        (scene.0 / scene.1) as f32
    } else {
        1.6
    };
    let ceiling = (screen_height * 0.6).clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT);
    let floor = PREVIEW_MIN_HEIGHT.min(ceiling);
    let height = (available_width / aspect).clamp(floor, ceiling);
    let width = (height * aspect).min(available_width);
    vec2(width, height)
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
            let size = preview_size(
                ui.available_width(),
                ui.ctx().screen_rect().height(),
                (scene.width, scene.height),
            );
            let view = alas_viz::SceneView::new(&scene, state.view_state_mut(view_key))
                .static_view()
                .show_toolbar(false)
                .desired_size(size);
            // Centred, and only as wide as the drawing: the widget used to
            // claim the whole container and letterbox the figure inside it.
            ui.vertical_centered(|ui| {
                ui.add(view);
            });
        }
        None => {
            ui.label(RichText::new(tr("Preview needs a completed run.")).weak());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::placement::{optimizer_ui_fields, relocated_paths};
    use super::{engine_editor_model, EngineEditorModel};
    use crate::nav;
    use crate::state::AppState;
    use alas_config::ConfigNode;

    #[test]
    fn a_page_preview_is_as_wide_as_the_drawing_and_no_wider() {
        // `22-advanced-settings-window.png`: the three-view raster occupied
        // x 558-990 inside a container spanning x 10-1540, so ~72% of the
        // container width was empty and the axis labels were illegible.
        let three_view = (900.0, 700.0);
        let size = super::preview_size(1530.0, 973.0, three_view);
        let aspect = size.x / size.y;
        assert!(
            (aspect - (three_view.0 / three_view.1) as f32).abs() < 0.01,
            "the widget must carry the scene's aspect, got {aspect}"
        );
        assert!(size.x <= 1530.0, "never wider than the container");
        // The drawing is drawn larger than the old height-limited raster.
        assert!(size.x > 600.0, "three-view width {} is too small", size.x);
        // A narrow container is width-limited instead, and still fits.
        let narrow = super::preview_size(320.0, 973.0, three_view);
        assert!(narrow.x <= 320.0);
        assert!(narrow.y >= super::PREVIEW_MIN_HEIGHT - 0.01);
        // A short window never asks for more height than it has.
        let short = super::preview_size(1530.0, 560.0, three_view);
        assert!(short.y <= 560.0 * 0.6 + 0.01);
        // A degenerate scene falls back instead of dividing by zero.
        let degenerate = super::preview_size(800.0, 900.0, (0.0, 0.0));
        assert!(degenerate.x.is_finite() && degenerate.y.is_finite());
        assert!(degenerate.x > 0.0 && degenerate.y > 0.0);
    }

    #[test]
    fn mission_locations_are_managed_only_on_the_external_tools_page() {
        for name in ["navdata_dir", "texture_path", "routes_dir"] {
            assert!(relocated_paths("mission").contains(&name));
        }
        assert!(!relocated_paths("mission").contains(&"great_circle_points"));
        assert!(!relocated_paths("mses").contains(&"mses_dir"));
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

    #[test]
    fn propulsion_page_routes_to_the_rendered_thermodynamic_preview() {
        let page = nav::page("engine_designer").expect("propulsion page");
        assert_eq!(page.preview, Some("engine"));
        assert_eq!(page.preview_title, Some("Thermodynamic cycle preview"));

        let state = AppState::default();
        let scene = crate::scene::build_page_preview(&state, page.preview.unwrap())
            .expect("default turbofan cycle preview");
        // `ts_preview::diagram` embeds the configured engine's name in the
        // drawn title (distinct from the static page heading asserted
        // above), so the default GE9X turbofan renders "GE9X * T-s".
        assert_eq!(scene.title.as_deref(), Some("GE9X * T-s"));
        assert!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(
                    element,
                    alas_report::scene::SceneElement::Polyline { .. }
                ))
                .count()
                >= 2
        );
    }

    #[test]
    fn propulsion_preview_title_tracks_the_configured_engine_name() {
        // Locks the routing contract as dynamic, not a hardcoded string: the
        // rendered preview must always name the engine actually configured,
        // so renaming the engine changes what the user sees without any
        // other edit.
        let mut state = AppState::default();
        let mut config = state.typed_config().expect("default config");
        // A real, database-registered turbofan distinct from the default
        // GE9X (see `crates/alas-config/data/engines.json`), so the binding
        // stays valid and only the configured name changes.
        config.geometry.engine.engine_name = "LEAP-1A".to_owned();
        state.config_values = serde_json::to_value(&config).expect("serialize config");

        let scene = crate::scene::build_page_preview(&state, "engine")
            .expect("turbofan cycle preview for the renamed engine");
        assert_eq!(scene.title.as_deref(), Some("LEAP-1A * T-s"));
    }

    #[test]
    fn optimizer_page_hides_legacy_methods_and_exposes_only_mads_settings() {
        let config = alas_config::AlasConfig::default();
        let fields = config
            .schema()
            .field("optimizer")
            .expect("optimizer group")
            .entry
            .clone();
        let alas_config::Entry::Node(node) = fields else {
            panic!("optimizer must be a group");
        };
        let filtered = optimizer_ui_fields(&node.fields);
        assert!(filtered
            .iter()
            .all(|field| { !matches!(field.name, "weights" | "design_space") }));
        let solver = filtered
            .iter()
            .find(|field| field.name == "solver")
            .expect("MADS settings group");
        let alas_config::Entry::Node(solver) = &solver.entry else {
            panic!("solver must be a group");
        };
        assert!(solver.fields.iter().all(|field| !matches!(
            field.name,
            "method"
                | "strategy"
                | "finite_difference_step"
                | "constraint_tolerance"
                | "tolerance"
                | "workers"
                | "display_progress"
                | "seed_near_initial_design"
                | "seed_perturbation_fraction"
        )));
        if let Some(iterations) = solver
            .fields
            .iter()
            .find(|field| field.name == "max_iterations")
        {
            assert_eq!(iterations.label, "MADS poll/search iterations");
        }
    }
}
