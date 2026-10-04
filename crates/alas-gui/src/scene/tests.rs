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
        ("LOAD & TRIM SHEET", &envelope),
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
            SceneElement::Text { text, .. } => Some(text.clone()),
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
    let title = localize_scene_text("Airbus A320-200  -  LOAD & TRIM SHEET (ALAS model)");
    alas_i18n::set_language(Some("en"));
    assert!(title.starts_with("Airbus A320-200  -  "), "{title}");
    assert!(!title.contains("LOAD & TRIM"), "{title}");
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
                element, SceneElement::Text { text, .. } if text == label
            )),
            "missing load-and-trim label: {label}"
        );
    }
}
