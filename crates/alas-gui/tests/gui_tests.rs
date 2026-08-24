// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless unit and state integration tests for `alas-gui`.

use alas_gui::layout;
use alas_gui::nav::{self, PageKind};
use alas_gui::scene::{
    build_page_preview, build_screening_figure, figure_cache_key, PREVIEW_DISPATCH_IDS,
    RESULT_DISPATCH_IDS, SCREENING_DISPATCH_IDS,
};
use alas_gui::state::{nav_overlay_open, nav_overlay_open_with_bounds, AppState, PreviewCamera};
use alas_gui::theme::AppTheme;
use egui::Context;

#[test]
fn default_app_state_loads_the_first_preset_and_a_valid_preview() {
    let state = AppState::default();

    assert_eq!(state.active_preset, "AVE");
    let config = state
        .typed_config()
        .expect("the freshly-serialized default config must round-trip");
    assert!(config.requirements.mtow_kg > 50_000.0);
    assert!(state.preview_scene.is_some());

    let scene = state.preview_scene.as_ref().unwrap();
    assert!(!scene.elements.is_empty());
}

#[test]
fn desktop_dark_preview_uses_the_accessible_canvas_and_spines() {
    let state = AppState::default();
    let scene = build_page_preview(&state, "drag").expect("dark drag preview");
    let palette = alas_report::theme::get_palette(Some("dark-accessible"));
    let background = alas_report::scene::Color::from_hex(palette.bg);
    let spine = alas_report::scene::Color::from_hex(palette.spine);

    assert_eq!(state.theme.figure_theme_name(), "dark-accessible");
    assert_eq!(scene.background, Some(background));
    assert!(scene.elements.iter().any(|element| match element {
        alas_report::scene::SceneElement::Line { stroke, .. }
        | alas_report::scene::SceneElement::Polyline { stroke, .. } => stroke.color == spine,
        alas_report::scene::SceneElement::Rect {
            stroke: Some(stroke),
            ..
        }
        | alas_report::scene::SceneElement::Polygon {
            stroke: Some(stroke),
            ..
        }
        | alas_report::scene::SceneElement::Circle {
            stroke: Some(stroke),
            ..
        } => stroke.color == spine,
        _ => false,
    }));
}

#[test]
fn preset_switching_updates_the_json_configuration_and_scene() {
    let mut state = AppState::default();

    state.load_preset("A380-800");
    assert_eq!(state.active_preset, "A380-800");
    let config = state.typed_config().unwrap();
    assert!(config.requirements.mtow_kg > 400_000.0);
    assert!(config.geometry.fuselage.diameter_m > 6.0);

    state.load_preset("A220-300");
    assert_eq!(state.active_preset, "A220-300");
    assert!(state.typed_config().unwrap().requirements.mtow_kg < 80_000.0);
}

#[test]
fn preset_switching_recenters_every_nonzero_design_bound() {
    let mut state = AppState::default();

    for preset in alas_config::presets::registry() {
        state.load_preset(preset.name);
        for spec in alas_config::DESIGN_VARIABLE_SPECS {
            let value = state.design_values[spec.name];
            let (lower, upper) = state.bounds[spec.name];
            assert!(
                lower < upper,
                "{} {} has an empty preset-local range",
                preset.name,
                spec.name
            );
            assert!(
                lower <= value && value <= upper,
                "{} {} baseline is outside its preset-local range",
                preset.name,
                spec.name
            );
        }
    }

    state.load_preset("A220-300");
    let a220_span_bounds = state.bounds["span_m"];
    assert_eq!(state.design_values["span_m"], 35.10);
    assert_eq!(a220_span_bounds, (29.8, 40.4));
    state.load_preset("A380-800");
    let a380_span_bounds = state.bounds["span_m"];
    assert_ne!(a220_span_bounds, a380_span_bounds);
    assert_eq!(state.design_values["span_m"], 79.75);
    assert_eq!(a380_span_bounds, (67.7, 91.8));
}

#[test]
fn preset_local_bounds_drive_the_design_space_sampler() {
    let mut state = AppState::default();
    state.load_preset("A220-300");

    let sample = state.sample_design(0.0);
    for (name, value) in sample {
        let (lower, upper) = state.bounds[&name];
        assert!(lower <= value && value <= upper, "{name}");
    }
}

#[test]
fn the_run_design_is_the_same_design_the_preview_shows() {
    let mut state = AppState::default();
    state.load_preset("A220-300");
    state.design_values.insert("span_m".to_owned(), 36.25);

    let design = state.current_design().expect("complete edited design");
    let bounds = state
        .current_design_bounds()
        .expect("complete edited bounds");

    assert_eq!(design.span_m, 36.25);
    assert_eq!(bounds.len(), alas_config::DESIGN_VARIABLE_SPECS.len());
}

#[test]
fn every_config_group_field_survives_the_json_round_trip() {
    // The generic form edits `config_values` directly; if a field the schema
    // lists could not be found back in the JSON, the form would silently do
    // nothing when the user edited it.
    let state = AppState::default();
    for field in &state.schema.fields {
        assert!(
            state.config_values.get(field.name).is_some(),
            "schema names field '{}' with nothing in config_values",
            field.name
        );
    }
}

#[test]
fn typed_config_round_trip_preserves_nested_class_mix_edits() {
    let mut state = AppState::default();
    if let Some(value) = state
        .config_values
        .get_mut("cabin")
        .and_then(|cabin| cabin.get_mut("passenger"))
        .and_then(|passenger| passenger.get_mut("class_mix_mode"))
    {
        *value = serde_json::json!("count");
    }
    if let Some(value) = state
        .config_values
        .get_mut("cabin")
        .and_then(|cabin| cabin.get_mut("passenger"))
        .and_then(|passenger| passenger.get_mut("business"))
        .and_then(|business| business.get_mut("count"))
    {
        *value = serde_json::json!(42);
    }

    let config = state
        .typed_config()
        .expect("nested form values must deserialize into typed config");
    assert_eq!(config.cabin.passenger.class_mix_mode, "count");
    assert_eq!(config.cabin.passenger.business.count, 42);
}

#[test]
fn design_space_labels_expose_human_names_for_key_variables() {
    let sweep = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .find(|spec| spec.name == "sweep_deg")
        .expect("sweep variable");
    let tail = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .find(|spec| spec.name == "tail_scale")
        .expect("tail scale variable");

    assert_eq!(sweep.name, "sweep_deg");
    assert_eq!(tail.name, "tail_scale");
    assert_eq!(sweep.description, "Inboard leading-edge sweep angle");
    assert_eq!(tail.description, "Uniform scale factor on the empennage");
}

#[test]
fn all_preview_figure_ids_generate_valid_scenes() {
    let mut state = AppState::default();

    for id in [
        "exterior_3d",
        "cabin_3d",
        "geometry",
        "drag",
        "mass_cg",
        "landing_gear",
        "control_surfaces",
        "structures",
        "engine",
        "3view",
    ] {
        state.selected_preview_id = id.to_owned();
        state.update_preview_scene();
        assert!(
            state.preview_scene.is_some(),
            "preview scene for {id} failed to build"
        );
        let scene = state.preview_scene.as_ref().unwrap();
        assert!(
            !scene.elements.is_empty(),
            "preview scene for {id} has 0 elements"
        );
    }
}

#[test]
fn python_geometry_preview_id_dispatches_to_the_three_view_scene() {
    let state = AppState::default();
    let scene = build_page_preview(&state, "geometry").expect("geometry preview");

    assert_eq!(scene.title.as_deref(), Some("Three-View Drawing"));
}

#[test]
fn drag_model_menu_uses_the_python_preview_registry_id() {
    let page = nav::NAV
        .iter()
        .flat_map(|group| group.subgroups)
        .flat_map(|subgroup| subgroup.pages)
        .find(|page| page.id == "drag_model")
        .expect("drag model page");

    assert_eq!(page.preview, Some("drag"));
}

#[test]
fn live_previews_match_the_python_registry_and_produce_scenes() {
    let state = AppState::default();

    for id in ["drag", "mass_cg", "landing_gear", "control_surfaces"] {
        let scene = build_page_preview(&state, id).expect("live preview scene");
        assert!(!scene.elements.is_empty(), "preview {id} is empty");
    }
}

#[test]
fn every_registered_figure_has_a_dispatch_entry_and_every_entry_is_registered() {
    use alas_report::registry::{PREVIEW_FIGURES, RESULT_FIGURES, SCREENING_FIGURES};
    fn assert_bidirectional(registry: &[alas_report::FigureDescriptor], dispatch: &[&str]) {
        for figure in registry {
            assert!(
                dispatch.contains(&figure.id),
                "registered figure {} has no GUI dispatch",
                figure.id
            );
        }
        for id in dispatch {
            assert!(
                registry.iter().any(|figure| figure.id == *id),
                "GUI dispatch {} has no registry descriptor",
                id
            );
        }
    }
    assert_bidirectional(PREVIEW_FIGURES, PREVIEW_DISPATCH_IDS);
    assert_bidirectional(RESULT_FIGURES, RESULT_DISPATCH_IDS);
    assert_bidirectional(SCREENING_FIGURES, SCREENING_DISPATCH_IDS);
}

#[test]
fn w32_registry_entries_have_live_gui_dispatch_and_stable_export_names() {
    for id in [
        "airfoil_evolution",
        "threeview",
        "cabin_payload",
        "design_evolution",
        "planform_comparison",
        "wireframe_wing",
        "wireframe_fuselage",
        "wireframe_empennage",
        "geometry",
    ] {
        assert!(
            RESULT_DISPATCH_IDS.contains(&id) || PREVIEW_DISPATCH_IDS.contains(&id),
            "missing GUI dispatch for {id}"
        );
        assert!(
            alas_report::find_figure(id).is_some(),
            "missing public registry entry for {id}"
        );
    }
    assert_eq!(alas_report::export_file_stem("threeview"), "threeview");
    assert_eq!(
        format!("{}.svg", alas_report::export_file_stem("threeview")),
        "threeview.svg"
    );
}

#[test]
fn w33_public_mission_result_reaches_every_mission_dispatch() {
    use alas_config::AlasConfig;
    use alas_exec::RunEnvironment;
    use alas_pipeline::PipelineOptions;
    use alas_route::route::{Route, RouteSource, Waypoint};

    let config = AlasConfig::default();
    let route = Route {
        waypoints: vec![
            Waypoint::named(40.47, -3.56, "LEMD"),
            Waypoint::named(45.0, -2.0, "W33_FIX"),
            Waypoint::named(51.15, -0.19, "EGKK"),
        ],
        source: RouteSource::SimbriefApi,
        origin_airport: None,
        dest_airport: None,
    };
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config)
        .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
        .expect("public GUI-equivalent pipeline run");
    assert!(result.route.is_some());
    assert!(result.mission_result.is_some());

    let mut state = AppState {
        pipeline_result: Some(result),
        ..AppState::default()
    };
    for id in [
        "mission_profile",
        "mission_velocities",
        "mission_flight_path",
        "mission_aero_coefficients",
        "mission_aero_forces",
        "mission_drag_components",
        "mission_route_2d",
        "mission_route_3d",
    ] {
        let scene = alas_gui::scene::build_result_figure(
            &state,
            id,
            &state
                .pipeline_result
                .as_ref()
                .expect("pipeline result")
                .config,
            "dark",
        )
        .flatten()
        .expect("public mission figure dispatch");
        assert!(!scene.elements.is_empty(), "empty W3.3 scene for {id}");
    }

    let config = state
        .pipeline_result
        .as_ref()
        .expect("pipeline result")
        .config
        .clone();
    let cache_key = "run=1;theme=dark;figure=mission_route_3d";
    let initial = state
        .cached_result_figure_with_camera(
            cache_key,
            "mission_route_3d",
            &config,
            "dark",
            Some(alas_report::scene::Camera3D::isometric()),
        )
        .expect("initial route scene");
    let rebuilt = state
        .rebuild_result_figure_with_camera(
            cache_key,
            "mission_route_3d",
            &config,
            "dark",
            alas_report::scene::Camera3D::top(),
        )
        .expect("reprojected route scene");
    assert_ne!(*initial, *rebuilt);
    assert_eq!(state.result_figure_cache.len(), 1);
}

#[test]
fn figure_cache_keys_change_with_run_config_theme_and_figure_identity() {
    alas_i18n::set_language(Some("en"));
    let config = alas_config::AlasConfig::default();
    let base = figure_cache_key(7, &config, "dark", "polar_comparison");
    assert!(base.contains("run=7"));
    assert!(base.contains("config="));
    assert!(base.contains("theme=dark"));
    assert!(base.contains("language=en"));
    assert!(base.contains("figure=polar_comparison"));
    assert_ne!(
        base,
        figure_cache_key(8, &config, "dark", "polar_comparison")
    );
    assert_ne!(
        base,
        figure_cache_key(7, &config, "light", "polar_comparison")
    );
    assert_ne!(base, figure_cache_key(7, &config, "dark", "aero_panel"));
    alas_i18n::set_language(Some("es"));
    assert_ne!(
        base,
        figure_cache_key(7, &config, "dark", "polar_comparison")
    );
    alas_i18n::set_language(Some("en"));
}

#[test]
fn screening_dispatch_has_no_scene_before_a_sweep_finishes() {
    let state = AppState::default();

    for id in [
        "trade_map",
        "rerank_2d_3d",
        "ranking_bars",
        "mses_verification",
        "section_shapes",
    ] {
        assert!(
            build_screening_figure(&state, id, "light").is_none(),
            "screening figure {id} should be unavailable before a sweep"
        );
    }
}

#[test]
fn every_advanced_form_page_names_a_group_the_schema_has() {
    let state = AppState::default();
    for group in nav::NAV {
        for sub in group.subgroups {
            for page in sub.pages {
                if page.kind == PageKind::Form {
                    let name = page.group.expect("a Form page names a group");
                    assert!(
                        state.schema.field(name).is_some(),
                        "page '{}' names group '{name}', which the schema does not have",
                        page.id
                    );
                }
            }
        }
    }
}

#[test]
fn theme_application_configures_context_visuals() {
    let ctx = Context::default();

    alas_gui::apply_theme(AppTheme::Dark, &ctx);
    assert!(ctx.style().visuals.dark_mode);

    alas_gui::apply_theme(AppTheme::Light, &ctx);
    assert!(!ctx.style().visuals.dark_mode);

    alas_gui::apply_theme(AppTheme::Grey, &ctx);
    assert!(ctx.style().visuals.dark_mode);
}

#[test]
fn desktop_shell_defaults_to_hover_navigation_and_a_readable_run_log() {
    let state = AppState::default();

    assert!(!state.nav_pinned);
    assert_eq!(state.run_log_height, 220.0);
    assert!(!state.logs.is_empty());
}

#[test]
fn unpinned_navigation_enters_only_from_the_rail_and_retains_the_overlay() {
    assert!(!nav_overlay_open(false, Some(100.0), 0.0));
    assert!(nav_overlay_open(false, Some(8.0), 0.0));
    assert!(nav_overlay_open(true, Some(180.0), 0.0));
    assert!(!nav_overlay_open(true, Some(240.0), 0.0));
}

#[test]
fn navigation_hover_geometry_remains_reachable_at_common_sizes_and_scales() {
    const {
        assert!(layout::NAV_PANEL_MIN_WIDTH < layout::NAV_PANEL_WIDTH);
        assert!(layout::NAV_PANEL_WIDTH < layout::NAV_PANEL_MAX_WIDTH);
    }
    for (physical_width, physical_height) in [(880.0, 560.0), (1280.0, 820.0), (1920.0, 1080.0)] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let width = physical_width / scale;
            let height = physical_height / scale;
            let expanded = layout::expanded_navigation_width(width);
            let log_max = layout::run_log_max_height(height);

            assert!(expanded >= layout::NAV_RAIL_WIDTH);
            assert!(expanded <= layout::NAV_PANEL_WIDTH);
            assert!(log_max >= layout::RUN_LOG_MIN_HEIGHT);
            assert!(log_max <= layout::RUN_LOG_MAX_HEIGHT);

            let rail_left = 16.0;
            assert!(nav_overlay_open_with_bounds(
                false,
                Some(rail_left + layout::NAV_RAIL_WIDTH),
                rail_left,
                layout::NAV_RAIL_WIDTH,
                expanded,
            ));
            assert!(nav_overlay_open_with_bounds(
                true,
                Some(rail_left + expanded),
                rail_left,
                layout::NAV_RAIL_WIDTH,
                expanded,
            ));
            assert!(!nav_overlay_open_with_bounds(
                true,
                Some(rail_left + expanded + 1.0),
                rail_left,
                layout::NAV_RAIL_WIDTH,
                expanded,
            ));
        }
    }
}

#[test]
fn preview_dock_uses_the_right_side_at_every_breakpoint() {
    for available_width in [480.0, 680.0, 1_048.0, 2_400.0] {
        assert_eq!(
            layout::preview_placement(available_width),
            layout::PreviewPlacement::Side
        );
    }
}

#[test]
fn run_log_height_stays_resizable_without_hiding_the_central_pane() {
    for viewport_height in [560.0, 700.0, 820.0, 1080.0] {
        let max_height = layout::run_log_max_height(viewport_height);
        assert_eq!(
            layout::run_log_height(viewport_height, f32::INFINITY),
            max_height
        );
        assert_eq!(
            layout::run_log_height(viewport_height, f32::NEG_INFINITY),
            layout::RUN_LOG_MIN_HEIGHT
        );
        assert_eq!(
            layout::run_log_height(viewport_height, 140.0),
            140.0_f32.clamp(layout::RUN_LOG_MIN_HEIGHT, max_height)
        );
    }
}

#[test]
fn interactive_canvases_keep_independent_viewport_state() {
    let mut state = AppState::default();

    state.view_state_mut("result::mass_breakdown").zoom = 2.0;
    state.view_state_mut("result::mass_distribution").pan = egui::vec2(12.0, -8.0);
    state.view_state_mut("screening::trade_map").zoom = 3.0;

    assert_eq!(state.view_states.len(), 3);
    assert_eq!(state.view_state_mut("result::mass_breakdown").zoom, 2.0);
    assert_eq!(
        state.view_state_mut("result::mass_distribution").pan,
        egui::vec2(12.0, -8.0)
    );
    assert_eq!(state.view_state_mut("screening::trade_map").zoom, 3.0);
    assert_eq!(
        state.view_state_mut("result::mass_breakdown").pan,
        egui::Vec2::ZERO
    );
}

#[test]
fn three_dimensional_previews_keep_independent_orbit_cameras() {
    let mut state = AppState::default();
    state.preview_camera_mut("exterior_3d").yaw_deg = 120.0;
    state.preview_camera_mut("exterior_3d").pitch_deg = 10.0;
    state.preview_camera_mut("cabin_3d").yaw_deg = 210.0;

    let exterior = *state.preview_camera_mut("exterior_3d");
    let cabin = *state.preview_camera_mut("cabin_3d");
    assert_eq!(exterior.yaw_deg, 120.0);
    assert_eq!(exterior.pitch_deg, 10.0);
    assert_eq!(exterior.zoom, 1.0);
    assert_eq!(cabin.yaw_deg, 210.0);
    assert_eq!(cabin.pitch_deg, 22.0);
}

#[test]
fn result_orbit_cameras_are_independent_from_previews_and_other_runs() {
    let mut state = AppState::default();
    state
        .result_camera_mut("result_camera::run=4;figure=mission_route_3d")
        .yaw_deg = 48.0;
    state
        .result_camera_mut("result_camera::run=5;figure=mission_route_3d")
        .yaw_deg = 120.0;

    assert_eq!(
        state
            .result_camera_mut("result_camera::run=4;figure=mission_route_3d")
            .yaw_deg,
        48.0
    );
    assert_eq!(
        state
            .result_camera_mut("result_camera::run=5;figure=mission_route_3d")
            .yaw_deg,
        120.0
    );
    assert_eq!(state.preview_camera_mut("mission_route_3d").yaw_deg, -125.0);
}

#[test]
fn three_dimensional_fit_changes_only_the_active_orbit_view() {
    let mut state = AppState::default();
    state.preview_camera_mut("exterior_3d").zoom = 3.0;
    state.preview_camera_mut("cabin_3d").zoom = 2.0;

    state.preview_camera_mut("exterior_3d").fit();

    assert_eq!(state.preview_camera_mut("exterior_3d").zoom, 1.0);
    assert_eq!(state.preview_camera_mut("cabin_3d").zoom, 2.0);
}

#[test]
fn three_dimensional_camera_presets_match_the_report_contract() {
    let top = PreviewCamera::top();
    let front = PreviewCamera::front();
    let side = PreviewCamera::side();
    let isometric = PreviewCamera::isometric();

    assert_eq!((top.pitch_deg, top.yaw_deg), (90.0, 0.0));
    assert_eq!((front.pitch_deg, front.yaw_deg), (0.0, 90.0));
    assert_eq!((side.pitch_deg, side.yaw_deg), (0.0, 0.0));
    assert_eq!((isometric.pitch_deg, isometric.yaw_deg), (22.0, -125.0));
}
