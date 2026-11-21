// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::localization::localize_scene_text;
use super::preview::{cap_interactive_preview_mesh, preview_unavailable_title};
use alas_config::AlasConfig;
use alas_report::scene::SceneElement;

#[test]
fn interactive_preview_caps_mesh_without_changing_solver_configuration() {
    let original = AlasConfig::default();
    let original_wing_subdivisions = original.geometry.wing.n_subdivisions;
    let original_empennage_subdivisions = original.geometry.empennage.n_subdivisions;
    let mut preview = original.clone();

    cap_interactive_preview_mesh(&mut preview);

    assert_eq!(preview.geometry.wing.n_subdivisions, 2);
    assert_eq!(preview.geometry.empennage.n_subdivisions, 2);
    assert_eq!(
        original.geometry.wing.n_subdivisions,
        original_wing_subdivisions
    );
    assert_eq!(
        original.geometry.empennage.n_subdivisions,
        original_empennage_subdivisions
    );
}

#[test]
fn mass_live_previews_use_the_materialized_cabin_flops_without_unverified_warning() {
    let state = crate::state::AppState::default();
    for id in ["landing_gear", "control_surfaces"] {
        let scene = super::build_page_preview(&state, id).expect("mass preview scene");
        let text = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        assert!(!text.contains("unverified"), "{id} warning: {text}");
    }
}

#[test]
fn a_failed_preview_is_titled_after_the_figure_it_replaces() {
    // One `Err` arm serves three previews. Each is titled after its own figure, not
    // "Mass and balance preview unavailable", so the Landing Gear page
    // never reports a failure of a different artefact.
    let gear = preview_unavailable_title("landing_gear");
    let envelope = preview_unavailable_title("mass_cg");
    let surfaces = preview_unavailable_title("control_surfaces");
    for (id, title) in [
        ("Landing-gear planform", &gear),
        ("Load & trim sheet", &envelope),
        ("Control-surface layout", &surfaces),
    ] {
        assert!(
            title.contains(&crate::views::tr(id)),
            "{title} does not name {id}"
        );
    }
    assert_ne!(gear, envelope);
    assert_ne!(gear, surfaces);
    assert_ne!(envelope, surfaces);
}

#[test]
fn layout_counts_keep_values_while_translating_their_units() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("349 seats, 65.0 t"),
        "349 asientos, 65.0 t"
    );
    assert_eq!(localize_scene_text("12 ULD, 30.5 t"), "12 ULD, 30.5 t");
    assert_eq!(
        localize_scene_text("9 463L pallet, 30.5 t"),
        "9 pal\u{e9}s 463L, 30.5 t"
    );
}

#[test]
fn cabin_section_lines_keep_their_numbers_while_translating_the_words() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text(
            "Representative station x = 16.7 m | outer section 3.95 x 4.14 m | lining 3.65 x 3.84 m"
        ),
        "Estaci\u{f3}n representativa x = 16.7 m | secci\u{f3}n exterior 3.95 x 4.14 m | revestimiento 3.65 x 3.84 m"
    );
    assert_eq!(
        localize_scene_text(
            "Representative station x = 25.9 m | Economy row | outer section 5.94 x 6.09 m | lining 5.64 x 5.79 m"
        ),
        "Estaci\u{f3}n representativa x = 25.9 m | fila: Clase turista | secci\u{f3}n exterior 5.94 x 6.09 m | revestimiento 5.64 x 5.79 m"
    );
    assert_eq!(
        localize_scene_text(
            "Model floor z = -0.870 m; lining keel z = -0.862 m, crown z = 0.862 m; chord at the floor 0.00 m (at least 0.80 m needed)"
        ),
        "Suelo del modelo z = -0.870 m; quilla del revestimiento z = -0.862 m, clave z = 0.862 m; cuerda en el suelo 0.00 m (se necesitan al menos 0.80 m)"
    );
    assert_eq!(
        localize_scene_text(
            "WARNING: 2 of 6 seats in this row do not fit inside the inner contour at the drawn floor height."
        ),
        "AVISO: 2 de 6 asientos de esta fila no caben dentro del contorno interior a la altura de suelo dibujada."
    );
    assert_eq!(
        localize_scene_text(
            "WARNING: 1 of 3 freight items at this station stand outside the inner contour at the floor."
        ),
        "AVISO: 1 de 3 elementos de carga de esta estaci\u{f3}n quedan fuera del contorno interior a la altura del suelo."
    );
    assert_ne!(
        localize_scene_text(
            "Cabin section cannot be drawn: the model's floor does not stand inside the lining."
        ),
        "Cabin section cannot be drawn: the model's floor does not stand inside the lining."
    );
}

#[test]
fn dynamic_mass_and_mode_labels_keep_values_while_translating_names() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("Wing\n41.9 t  (12%)"),
        "Ala\n41.9 t  (12%)"
    );
    assert_eq!(
        localize_scene_text("Physical CG  x=35.7 m"),
        "CG f\u{00ed}sico  x=35.7 m"
    );
    assert_eq!(localize_scene_text("Mode 3: 15.92 Hz"), "Modo 3: 15.92 Hz");
}

#[test]
fn analyzed_and_sized_weight_labels_keep_their_roles_and_mass_in_spanish() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    for (source, expected) in [
        ("Analyzed ZFW 56 430 kg", "ZFW analizada 56 430 kg"),
        ("Sized TOW 60 740 kg", "TOW dimensionada 60 740 kg"),
        ("Analyzed TOW 60 740 kg", "TOW analizada 60 740 kg"),
        ("Design LW 60 740 kg", "LW de dise\u{00f1}o 60 740 kg"),
        (
            "Sized TOW / Design LW 60 740 kg",
            "TOW dimensionada / LW de dise\u{00f1}o 60 740 kg",
        ),
        (
            "Analyzed TOW / Design LW / Analyzed ZFW 60 740 kg",
            "TOW analizada / LW de dise\u{00f1}o / ZFW analizada 60 740 kg",
        ),
    ] {
        assert_eq!(localize_scene_text(source), expected);
    }
}

#[test]
fn report_figure_labels_translate_dynamic_prefixes_without_changing_values() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("Wingbox planform  (39 ribs)"),
        "Planta del caj\u{f3}n alar  (39 ribs)"
    );
    for text in [
        "Wing mass by model (different scopes)",
        "FE (Nastran)\nprimary wingbox",
        "Native beam\n(primary structure)",
        "FLOPS estimate\ncomplete wing",
    ] {
        assert_ne!(localize_scene_text(text), text, "{text} has no Spanish");
    }
    assert_eq!(
        localize_scene_text("Per-engine thrust, this cruise pt :   242.4 kN"),
        "Empuje por motor, en este punto de crucero :   242.4 kN"
    );
    assert_eq!(
        localize_scene_text("Torenbeek\nestimate"),
        "Torenbeek\nestimaci\u{00f3}n"
    );
}

#[test]
fn optimization_history_labels_translate_and_keep_their_counts() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    use alas_report::families::optimization::{
        BEST_VALID_LABEL, FAILED_LABEL, HISTORY_TITLE, HISTORY_X_LABEL, REJECTED_LABEL, VALID_LABEL,
    };
    for label in [VALID_LABEL, REJECTED_LABEL, FAILED_LABEL] {
        let counted = format!("{label} (38214)");
        let localized = localize_scene_text(&counted);
        assert_ne!(localized, counted, "{label} has no Spanish");
        assert!(localized.ends_with(" (38214)"), "{localized}");
    }
    for label in [BEST_VALID_LABEL, HISTORY_TITLE, HISTORY_X_LABEL] {
        assert_ne!(localize_scene_text(label), label, "{label} has no Spanish");
    }
    for stage in alas_opt::TraceStage::ALL {
        assert_ne!(
            localize_scene_text(stage.label()),
            stage.label(),
            "{stage:?} has no Spanish"
        );
    }
    // A parenthesis that is not a bare count is left to the other rules.
    assert_eq!(localize_scene_text("Valid (n/a)"), "Valid (n/a)");
}

#[test]
fn report_annotations_translate_generated_prefixes_without_changing_measurements() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("Governing load case: pull-up"),
        "Caso de carga determinante: pull-up"
    );
    assert_eq!(
        localize_scene_text("Semi-wing mass: 20,291 kg"),
        "Masa de semiala: 20,291 kg"
    );
    assert_eq!(
        localize_scene_text("  Spar caps: 5,470 kg"),
        "  Tapas de larguero: 5,470 kg"
    );
    assert_eq!(
        localize_scene_text("Landing Gear Planform: NLG: 2xHeavy"),
        "Planta del tren de aterrizaje: NLG: 2xHeavy"
    );
}

#[test]
fn cg_steering_constraint_uses_control_in_spanish() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("Min nose load (steering)"),
        "Carga m\u{00ed}nima en morro (control)"
    );
    assert_eq!(
        localize_scene_text("Min nose load\n(steering)"),
        "Carga m\u{00ed}nima en morro\n(control)"
    );
}

#[test]
fn route_footers_preserve_measurements_while_translating_the_prose() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(localize_scene_text("Origin"), "Origen");
    assert_eq!(localize_scene_text("Destination"), "Destino");
    assert_eq!(localize_scene_text("Total Mass (t)"), "Masa total (t)");
    assert_eq!(
        localize_scene_text(
            "5 waypoints | 5525 km | flown profile: 254000 kg to 215000 kg, 0 to 0 m"
        ),
        "5 puntos de ruta | 5525 km | perfil volado: de 254000 kg a 215000 kg, de 0 a 0 m"
    );
    assert_eq!(
        localize_scene_text(
            "5 waypoints | 5525 km | flown profile: 254000 kg to 215000 kg, peak altitude 11278 m"
        ),
        "5 puntos de ruta | 5525 km | perfil volado: de 254000 kg a 215000 kg, altitud m\u{00e1}xima 11278 m"
    );
    assert_eq!(
        localize_scene_text(
            "5 waypoints | 5525 km | flown altitude and mass profile | orthographic globe; drag to orbit; wheel zoom in fullscreen"
        ),
        "5 puntos de ruta | 5525 km | perfil de altitud y masa volado | globo ortogr\u{00e1}fico; arrastre para orbitar; use la rueda para ampliar en pantalla completa"
    );
}

#[test]
fn route_mass_scale_keeps_english_when_english_is_selected() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("en"));
    assert_eq!(localize_scene_text("Total Mass (t)"), "Total Mass (t)");
}

/// The texts of a scene, in drawing order.
fn scene_texts(scene: &alas_report::scene::Scene) -> Vec<String> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_mass_preview_shows_the_run_sheet_when_the_run_adjusted_its_own_configuration() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"})).unwrap();
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config)
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .unwrap();
    let mut state = crate::state::AppState {
        config_values: serde_json::to_value(&result.config).unwrap(),
        ..Default::default()
    };
    state.set_completed_pipeline_result(result);
    // The dispatch policy changes the run's copy of the configuration (it
    // anchors the optimizer bounds); the form keeps what the user set.
    state
        .pipeline_result
        .as_mut()
        .unwrap()
        .config
        .optimizer
        .solver
        .screening
        .max_evaluations += 1;
    let config = state.pipeline_result.as_ref().unwrap().config.clone();
    let preview = super::build_page_preview(&state, "mass_cg").unwrap();
    let sheet = super::build_result_figure(&state, "cg_envelope", &config, "light")
        .flatten()
        .unwrap();
    assert_eq!(scene_texts(&preview), scene_texts(&sheet));
    // An edit after the run returns the preview to the pre-run draft.
    state.config_values["requirements"]["mtow_kg"] = serde_json::json!(70_000.0);
    let edited = super::build_page_preview(&state, "mass_cg").unwrap();
    assert_ne!(scene_texts(&edited), scene_texts(&sheet));
}

#[test]
fn a_sheet_title_translates_the_sheet_name_and_keeps_the_aircraft_name() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let title = localize_scene_text("Airbus A320-200 \u{2013} Load & trim sheet (ALAS model)");
    alas_i18n::set_language(Some("en"));
    assert!(title.starts_with("Airbus A320-200 \u{2013} "), "{title}");
    assert!(!title.contains("Load & trim"), "{title}");
}

#[test]
fn completed_mass_preview_uses_the_load_trim_sheet() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config)
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .unwrap();
    let mut state = crate::state::AppState {
        config_values: serde_json::to_value(&result.config).unwrap(),
        ..Default::default()
    };
    state.set_completed_pipeline_result(result);
    let scene = super::build_page_preview(&state, "mass_cg").unwrap();
    for label in ["%MAC", "DOW", "ZFW", "TOW", "LW"] {
        assert!(
            scene.elements.iter().any(|element| matches!(
                element, SceneElement::Text { text, .. }
                    // A state label is its name and its CG ("ZFW 27.1"); the unit
                    // is bracketed in the axis label, "CG position [%MAC]", like
                    // every other axis label.
                    if text == label
                        || (label != "%MAC" && text.starts_with(&format!("{label} ")))
                        || (label == "%MAC" && text.contains("[%MAC]"))
            )),
            "missing load-and-trim label: {label}"
        );
    }
}

/// One page of results names the same aerodromes in every field-performance
/// figure, also when the plan flies another pair than the configured one.
#[test]
fn the_field_figures_of_a_swapped_route_name_the_dispatched_aerodromes() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"})).unwrap();
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config.clone())
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .unwrap();
    let mut state = crate::state::AppState {
        config_values: serde_json::to_value(&result.config).unwrap(),
        ..Default::default()
    };
    state.set_completed_pipeline_result(result);
    let config = state.pipeline_result.as_ref().unwrap().config.clone();
    let configured_origin = alas_config::airports::get(&config.departure_airport)
        .unwrap()
        .clone();
    let configured_destination = alas_config::airports::get(&config.arrival_airport)
        .unwrap()
        .clone();
    assert_ne!(configured_origin.icao, configured_destination.icao);
    let text = |state: &crate::state::AppState, id: &str| {
        scene_texts(
            &super::build_result_figure(state, id, &config, "light")
                .flatten()
                .unwrap_or_else(|| panic!("{id} builds")),
        )
        .join("\n")
    };
    // Configured pair: departure figure names the departure, arrival figure
    // the arrival, and the matching chart carries both.
    assert!(text(&state, "lto_departure").contains(&configured_origin.name));
    assert!(text(&state, "lto_arrival").contains(&configured_destination.name));
    // The plan flies the other way.
    let mut route = alas_route::Route::new(Vec::new(), alas_route::RouteSource::GreatCircle);
    route.origin_airport = Some(configured_destination.clone());
    route.dest_airport = Some(configured_origin.clone());
    state.pipeline_result.as_mut().unwrap().route = Some(route);
    let departure = text(&state, "lto_departure");
    let arrival = text(&state, "lto_arrival");
    let matching = text(&state, "matching_chart");
    assert!(
        departure.contains(&configured_destination.name),
        "{departure}"
    );
    assert!(!departure.contains(&configured_origin.name));
    assert!(arrival.contains(&configured_origin.name), "{arrival}");
    assert!(!arrival.contains(&configured_destination.name));
    assert!(
        matching.contains(&configured_origin.name)
            && matching.contains(&configured_destination.name),
        "{matching}"
    );
    // An airport that does not resolve keeps the report's status scene.
    let mut broken = config.clone();
    broken.arrival_airport = "NOT-AN-AIRPORT".to_owned();
    state.pipeline_result.as_mut().unwrap().route = None;
    let status = scene_texts(
        &super::build_result_figure(&state, "matching_chart", &broken, "light")
            .flatten()
            .expect("a status scene, not an empty slot"),
    )
    .join("\n");
    assert!(
        status.contains("Departure/arrival airport not found in the database."),
        "{status:?}"
    );
}

#[test]
fn the_gear_line_translates_the_tire_rating_basis_as_one_unit() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let line = localize_scene_text(
        "Tip-back: 12.0 deg (OK)  |  Tire margin nose/main: 1.04 / 1.16 (OK); basis book / bound",
    );
    assert!(
        line.contains("; base de la carga nominal: cat\u{e1}logo / cota inferior"),
        "{line}"
    );
    assert!(!line.contains("basis"), "{line}");
}

/// Paint `scene` through the desktop egui backend into the software raster and write a PNG.
fn paint_scene_png(scene: &alas_report::scene::Scene, name: &str) {
    use crate::test_raster::{render, write_in};
    let (w, h) = (scene.width as f32, scene.height as f32);
    let shot = render(w, h, 3, 0, |ui| {
        ui.ctx().set_pixels_per_point(ppp());
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h));
        let transform = alas_viz::ViewportTransform::fit(scene.width, scene.height, rect);
        let shapes = alas_viz::render_scene_to_shapes_with_context(scene, &transform, ui.ctx());
        ui.painter().extend(shapes);
    });
    write_in(
        &crate::test_raster::render_dir(),
        &format!("p{}-{name}", ppp()),
        &shot,
    );
}

/// Renders the tail-volume legend and the load and trim tables for a look.
#[test]
fn polish_renders_control_layout_and_load_trim_tables() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config)
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .unwrap();
    let mut state = crate::state::AppState {
        config_values: serde_json::to_value(&result.config).unwrap(),
        ..Default::default()
    };
    state.set_completed_pipeline_result(result);
    for width in [560.0_f64, 780.0, 1000.0] {
        let scene =
            super::build_page_preview_sized(&state, "mass_cg", Some((width, 800.0))).unwrap();
        paint_scene_png(&scene, &format!("loadtrim-{width}.png"));
        let png = alas_viz::raster::render_scene_png(&scene).unwrap();
        write_png_bytes(&format!("svg-loadtrim-{width}.png"), &png);
    }
    let scene =
        super::build_page_preview_sized(&state, "control_surfaces", Some((760.0, 620.0))).unwrap();
    paint_scene_png(&scene, "control-surfaces.png");
}

fn ppp() -> f32 {
    std::env::var("POLISH_PPP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
}

/// The live Aircraft-page cards are SVG figures rasterised by resvg; write that exact raster.
#[test]
fn polish_renders_live_svg_path() {
    let mut state = crate::state::AppState::default();
    state.load_preset("A320-200");
    for (id, canvas) in [
        ("control_surfaces", Some((760.0, 620.0))),
        ("control_surfaces", None),
    ] {
        let scene = super::build_page_preview_sized(&state, id, canvas).unwrap();
        let png = alas_viz::raster::render_scene_png(&scene).unwrap();
        let name = format!("svg-{id}-{}.png", canvas.map_or(0.0, |c| c.0));
        write_png_bytes(&name, &png);
    }
}

/// Write `png` as `name` into the render folder, when it exists.
fn write_png_bytes(name: &str, png: &[u8]) {
    let directory = crate::test_raster::render_dir();
    let directory = std::path::Path::new(&directory);
    if directory.is_dir() {
        let _ = std::fs::write(directory.join(name), png);
    }
}

#[test]
fn caption_note_and_status_texts_of_the_figure_fixes_translate_around_their_numbers() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text(
            "Peak altitude 21000 ft vs configured cruise 37000 ft (57 %) - route too short to reach the configured cruise altitude"
        ),
        "Altitud m\u{e1}xima 21000 ft frente a crucero configurado 37000 ft (57 %) - ruta demasiado corta para alcanzar la altitud de crucero configurada"
    );
    assert_eq!(
        localize_scene_text("Dotted beyond BPR 8.0: corner in SFn (cycle limit or solver branch switch), not a trend."),
        "L\u{ed}nea de puntos m\u{e1}s all\u{e1} de BPR 8.0: esquina en SFn (l\u{ed}mite del ciclo o cambio de rama del solver), no una tendencia."
    );
    assert_eq!(
        localize_scene_text("MSC NASTRAN SOL 103 (paired by MAC, 2 of 6 below 0.80)"),
        "MSC NASTRAN SOL 103 (emparejados por MAC, 2 de 6 por debajo de 0.80)"
    );
    assert_eq!(
        localize_scene_text("FE (paired by MAC, all 6 >= 0.80)"),
        "FE (emparejados por MAC, todos los 6 >= 0.80)"
    );
    assert_eq!(
        localize_scene_text(
            "NASTRAN-95 38.20 Hz: local / chordwise, no beam counterpart (MAC 0.31)"
        ),
        "NASTRAN-95 38.20 Hz: local / en la cuerda, sin equivalente de viga (MAC 0.31)"
    );
    assert_eq!(
        localize_scene_text(
            "NASTRAN-95 8.20 Hz: torsion-dominated, twist*c/2 / w = 2.31 (MAC 0.12)"
        ),
        "NASTRAN-95 8.20 Hz: dominado por torsi\u{f3}n, giro*c/2 / w = 2.31 (MAC 0.12)"
    );
    let card = localize_scene_text(
        "Patran not configured. Set its executable under Workspace > External Tools to render deformation plots. Reported status: Patran executable absent: configure it",
    );
    assert!(
        card.starts_with("Patran no configurado. Indique su ejecutable"),
        "{card}"
    );
    assert!(
        card.ends_with("Estado notificado: Patran executable absent: configure it"),
        "{card}"
    );
    assert_eq!(
        localize_scene_text("No feasible CG range: forward limit aft of aft limit (41000 to 60000 kg)"),
        "Sin rango de CG viable: l\u{ed}mite delantero detr\u{e1}s del l\u{ed}mite trasero (41000 a 60000 kg)"
    );
    let worst = localize_scene_text(
        "Worst at 60000 kg: forward 27.4 %MAC (limit set: ground / landing), aft 19.2 %MAC (limit set: landing)",
    );
    assert!(
        worst.starts_with("Caso m\u{e1}s desfavorable a 60000 kg: delantero 27.4 %CMA"),
        "{worst}"
    );
    assert!(
        worst.contains("(conjunto de l\u{ed}mites: tierra / aterrizaje)"),
        "{worst}"
    );
    assert_eq!(
        localize_scene_text("12 of 40 cruise points dropped: only points within 150 m of the highest cruise altitude flown are shown."),
        "12 de 40 puntos de crucero descartados: solo se muestran los puntos a menos de 150 m de la mayor altitud de crucero volada."
    );
    let tow = super::localization::localize_finding_message(
        "TOW limited at LEMD: CS 25.121(b) second-segment gradient / CAT.POL.A.210 obstacle clearance limit the take-off to 22520 kg of the 23200 kg mission mass; leave 680 kg of payload or fuel behind. An operating limitation, not a design violation: the sea-level ISA certification rows are reported separately",
    );
    assert!(tow.starts_with("Masa de despegue limitada en LEMD: gradiente del segundo tramo CS 25.121(b) / franqueamiento de obst"), "{tow}");
    assert!(tow.contains("a 22520 kg de los 23200 kg"), "{tow}");
    let hatched = localize_scene_text(
        "Hatched: Re 1.0e4 to 8.3e4 (240 cells per map) is not drawn, unreliable low-Re surrogate range. The colour scale uses only the drawn cells.",
    );
    assert!(
        hatched.starts_with("Rayado: Re de 1.0e4 a 8.3e4 (240 celdas por mapa)"),
        "{hatched}"
    );
    let masked = localize_scene_text(
        "5 grid point(s) masked (faint hollow markers): specific thrust fell more than 20 % below its running maximum along the OPR sweep (backward branch, not a valid operating point). SFn of the masked points: 80 to 120 m/s. 2 of them fall outside the plotted range and are not drawn.",
    );
    assert!(
        masked.contains("de 80 a 120 m/s. 2 de ellos quedan fuera"),
        "{masked}"
    );
    let vibration = localize_scene_text(
        "NASTRAN SOL 111 (modal frequency response) not configured. No sine-sweep or random-vibration result exists for this design. Reported status: NASTRAN-95 backend does not provide it",
    );
    assert!(
        vibration.contains("Estado notificado: NASTRAN-95 backend does not provide it"),
        "{vibration}"
    );
    assert_eq!(
        localize_scene_text("The aircraft builder rejected this design: bad span"),
        "El constructor de la aeronave rechaz\u{f3} este dise\u{f1}o: bad span"
    );
    alas_i18n::set_language(None);
}

/// Every `"Label [unit]"` axis literal of the report crate has a Spanish label,
/// except the symbol-only ones that read the same in both languages.
#[test]
fn bracketed_unit_axis_labels_of_the_report_crate_translate() {
    fn collect(dir: &std::path::Path, found: &mut std::collections::BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, found);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".rs") || name.contains("test") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for piece in text.split('"').skip(1).step_by(2) {
                let bracketed = piece.ends_with(']')
                    && piece.contains(" [")
                    && !piece.contains(['{', '}', '\\', '\n'])
                    && piece
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic());
                if bracketed {
                    found.insert(piece.to_owned());
                }
            }
        }
    }
    // Labels that are a symbol or a name, written the same in Spanish.
    const SAME_IN_SPANISH: &[&str] = &[
        "T [K]",
        "CG [%MAC]",
        "x (longitudinal) [m]",
        "y (lateral) [m]",
        "W [kg]",
        "X [m]",
        "Y [m]",
        "Z [m]",
        "x/c [-]",
        "y/c [-]",
        "SFn [m/s]",
        "TSFC [mg/(N.s)]",
    ];
    let report_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../alas-report/src");
    let mut labels = std::collections::BTreeSet::new();
    collect(&report_src, &mut labels);
    assert!(
        labels.len() > 30,
        "the scan found only {} labels",
        labels.len()
    );

    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let untranslated: Vec<_> = labels
        .iter()
        .filter(|label| !SAME_IN_SPANISH.contains(&label.as_str()))
        .filter(|label| localize_scene_text(label) == **label)
        .collect();
    alas_i18n::set_language(None);
    assert!(untranslated.is_empty(), "no Spanish for {untranslated:?}");
}
