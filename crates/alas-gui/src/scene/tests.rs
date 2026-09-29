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
        ("CG envelope (illustrative)", &envelope),
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
fn report_figure_labels_translate_dynamic_prefixes_without_changing_values() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    assert_eq!(
        localize_scene_text("Wingbox planform  (39 ribs)"),
        "Planta del caj\u{f3}n alar  (39 ribs)"
    );
    assert_eq!(
        localize_scene_text("FEM vs Torenbeek wing mass  (delta = -3%)"),
        "Masa alar: FEM frente a Torenbeek  (delta = -3%)"
    );
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
