// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::engine::{engine_editor_model, EngineEditorModel};
use super::placement::{optimizer_ui_fields, relocated_paths};
use super::reset_page_to_defaults;
use crate::nav;
use crate::state::AppState;
use alas_config::ConfigNode;
use serde_json::json;

#[test]
fn propulsion_reset_restores_only_its_relocated_mass_inputs() {
    let defaults = serde_json::to_value(alas_config::AlasConfig::default())
        .expect("default config serializes");
    let mut state = AppState::default();
    state.config_values["mass_model"]["propulsion_twr_factor"] = json!(99.0);
    state.config_values["mass_model"]["propulsion_installation_factor"] = json!(99.0);
    state.config_values["mass_model"]["flops_turboprop"] = json!({"changed": true});
    state.config_values["mass_model"]["suspended_mass_fraction"] = json!(0.123);

    let page = nav::ADVANCED_SETTINGS_PAGES
        .iter()
        .find(|page| page.id == "propulsion_advanced")
        .expect("Propulsion Advanced page");
    reset_page_to_defaults(&mut state, page, "propulsion_cycle");

    for name in ["propulsion_twr_factor", "propulsion_installation_factor"] {
        assert_eq!(
            state.config_values["mass_model"][name], defaults["mass_model"][name],
            "{name} is edited on the Propulsion page"
        );
    }
    assert_eq!(
        state.config_values["mass_model"]["suspended_mass_fraction"],
        json!(0.123),
        "the Mass page keeps its own settings"
    );
    assert_eq!(
        state.config_values["mass_model"]["flops_turboprop"],
        json!({"changed": true}),
        "turboprop mass inputs are edited on the Mass page"
    );
}

#[test]
fn mass_reset_preserves_mass_inputs_edited_on_propulsion() {
    let defaults = serde_json::to_value(alas_config::AlasConfig::default())
        .expect("default config serializes");
    let mut state = AppState::default();
    state.config_values["mass_model"]["propulsion_twr_factor"] = json!(42.0);
    state.config_values["mass_model"]["propulsion_installation_factor"] = json!(2.0);
    state.config_values["mass_model"]["flops_turboprop"] = json!({"engine_dry_mass_kg": 123.0});
    state.config_values["mass_model"]["suspended_mass_fraction"] = json!(0.123);
    state.config_values["mass_model"]["systems_mass_fraction"] = json!(0.321);

    let page = nav::ADVANCED_SETTINGS_PAGES
        .iter()
        .find(|page| page.id == "mass_advanced")
        .expect("Mass Advanced page");
    reset_page_to_defaults(&mut state, page, "mass_model");

    assert_eq!(
        state.config_values["mass_model"]["propulsion_twr_factor"],
        json!(42.0)
    );
    assert_eq!(
        state.config_values["mass_model"]["propulsion_installation_factor"],
        json!(2.0)
    );
    for name in ["flops_turboprop", "suspended_mass_fraction"] {
        assert_eq!(
            state.config_values["mass_model"][name], defaults["mass_model"][name],
            "{name} belongs to Mass and should reset"
        );
    }
    assert_eq!(
        state.config_values["mass_model"]["systems_mass_fraction"],
        json!(0.321),
        "Modeling Mass values must survive an Advanced Mass reset"
    );
}

#[test]
fn modeling_mass_reset_keeps_advanced_mass_inputs() {
    let defaults = serde_json::to_value(alas_config::AlasConfig::default())
        .expect("default config serializes");
    let mut state = AppState::default();
    state.config_values["mass_model"]["propulsion_twr_factor"] = json!(42.0);
    state.config_values["mass_model"]["flops_turboprop"] = json!({"engine_dry_mass_kg": 123.0});
    state.config_values["mass_model"]["suspended_mass_fraction"] = json!(0.123);
    state.config_values["mass_model"]["systems_mass_fraction"] = json!(0.321);

    let page = nav::page("mass_model").expect("Modeling Mass page");
    reset_page_to_defaults(&mut state, page, "mass_model");

    assert_eq!(
        state.config_values["mass_model"]["systems_mass_fraction"],
        defaults["mass_model"]["systems_mass_fraction"],
        "Modeling Mass values should reset"
    );
    assert_eq!(
        state.config_values["mass_model"]["propulsion_twr_factor"],
        json!(42.0)
    );
    assert_eq!(
        state.config_values["mass_model"]["flops_turboprop"],
        json!({"engine_dry_mass_kg": 123.0})
    );
    assert_eq!(
        state.config_values["mass_model"]["suspended_mass_fraction"],
        json!(0.123),
        "Advanced Mass values must survive a Modeling Mass reset"
    );
}

#[test]
fn propulsion_summary_uses_the_width_of_a_wide_settings_window() {
    let model =
        engine_editor_model(&alas_config::EngineConfig::default()).expect("default engine summary");
    let ctx = egui::Context::default();
    let mut output = None;
    for _ in 0..2 {
        output = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1_535.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::engine::render_engine_physics_summary(ui, &model);
                });
            },
        ));
    }
    fn text_x(shape: &egui::Shape, xs: &mut Vec<f32>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_x(shape, xs);
                }
            }
            egui::Shape::Text(text) => xs.push(text.pos.x),
            _ => {}
        }
    }
    let mut xs = Vec::new();
    for clipped in &output.expect("rendered summary").shapes {
        text_x(&clipped.shape, &mut xs);
    }
    let span = xs.iter().copied().reduce(f32::max).unwrap_or(0.0)
        - xs.iter().copied().reduce(f32::min).unwrap_or(0.0);
    assert!(
        span > 600.0,
        "summary metrics should use the wide page, but text spans only {span} points: {xs:?}"
    );
}

#[test]
fn a_page_preview_is_as_wide_as_the_drawing_and_no_wider() {
    // `22-advanced-settings-window.png`: the three-view raster occupied
    // x 558-990 inside a container spanning x 10-1540, so ~72% of the
    // container width was empty and the axis labels were illegible.
    let three_view = (900.0, 700.0);
    let size = super::preview::preview_size(1530.0, 973.0, three_view);
    let aspect = size.x / size.y;
    assert!(
        (aspect - (three_view.0 / three_view.1) as f32).abs() < 0.01,
        "the widget must carry the scene's aspect, got {aspect}"
    );
    assert!(size.x <= 1530.0, "never wider than the container");
    // The drawing is not limited by a fixed raster height.
    assert!(size.x > 600.0, "three-view width {} is too small", size.x);
    // A narrow container is width-limited instead, and still fits.
    let narrow = super::preview::preview_size(320.0, 973.0, three_view);
    assert!(narrow.x <= 320.0);
    assert!(narrow.y >= super::preview::PREVIEW_MIN_HEIGHT - 0.01);
    // A short window never asks for more height than it has.
    let short = super::preview::preview_size(1530.0, 560.0, three_view);
    assert!(short.y <= 560.0 * 0.6 + 0.01);
    // A degenerate scene falls back instead of dividing by zero.
    let degenerate = super::preview::preview_size(800.0, 900.0, (0.0, 0.0));
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
            .filter(|element| matches!(element, alas_report::scene::SceneElement::Polyline { .. }))
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
fn optimizer_page_exposes_both_profiles_and_their_settings() {
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
    assert!(filtered.iter().any(|field| field.name == "weights"));
    assert!(filtered.iter().all(|field| field.name != "design_space"));
    let solver = filtered
        .iter()
        .find(|field| field.name == "solver")
        .expect("optimizer profile and solver settings group");
    let alas_config::Entry::Node(solver) = &solver.entry else {
        panic!("solver must be a group");
    };
    for shown in [
        "method",
        "strategy",
        "seed_near_initial_design",
        "seed_perturbation_fraction",
        "display_progress",
    ] {
        assert!(
            solver.fields.iter().any(|field| field.name == shown),
            "{shown} must be available to configure the selected profile"
        );
    }
    assert!(solver.fields.iter().all(|field| !matches!(
        field.name,
        "finite_difference_step" | "constraint_tolerance"
    )));
    for shown in [
        "max_iterations",
        "population_size",
        "seed",
        "workers",
        "tolerance",
        "convergence_stagnation_generations",
    ] {
        assert!(
            solver.fields.iter().any(|field| field.name == shown),
            "{shown} must be shown on the optimizer settings page"
        );
    }
    if let Some(iterations) = solver
        .fields
        .iter()
        .find(|field| field.name == "max_iterations")
    {
        assert_eq!(iterations.label, "Max generations");
    }
}
