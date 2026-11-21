// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Save, close, reopen for every object kind of the file library.

use std::path::PathBuf;

use alas_exec::ToolLocator;
use serde_json::{json, Value};

use crate::file_library::{self, ObjectKind};
use crate::state::AppState;

fn state_in(tag: &str) -> (AppState, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "alas-library-ops-{tag}-{}-{}",
        std::process::id(),
        crate::user_files::now_unix()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut state = AppState {
        tool_locator: ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.addons.dir = root.join("addons");
    state.boot_frames_remaining = 0;
    (state, root)
}

fn entry_of(state: &AppState, kind: ObjectKind) -> file_library::LibraryEntry {
    file_library::list(&state.data_dir(), kind)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("a saved {kind:?} file"))
}

/// The engine type: the installation without where it is mounted.
fn engine_type(state: &AppState) -> Value {
    let mut engine = pointer(state, "/geometry/engine");
    if let Some(map) = engine.as_object_mut() {
        for key in [
            "spanwise_positions_m",
            "z_m",
            "z_per_engine_m",
            "inlet_x_offset_m",
            "centre_engine",
            // Held by the registered aircraft the engine is opened into.
            "nacelle_profile",
            "radius_scale_m",
        ] {
            map.remove(key);
        }
    }
    engine
}

fn pointer(state: &AppState, path: &str) -> Value {
    state
        .config_values
        .pointer(path)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn an_aircraft_survives_save_close_reopen() {
    let (mut state, root) = state_in("aircraft");
    state.load_preset("A320-200");
    let saved = state.config_values.clone();
    assert!(state.library_save(ObjectKind::Aircraft, "Case", ""));
    state.load_preset("A220-300");
    assert_ne!(pointer(&state, "/geometry/wing"), saved["geometry"]["wing"]);
    let entry = entry_of(&state, ObjectKind::Aircraft);
    assert!(state.library_open(&entry));
    assert_eq!(pointer(&state, "/geometry/wing"), saved["geometry"]["wing"]);
    assert_eq!(pointer(&state, "/requirements"), saved["requirements"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_engine_a_mission_plan_and_a_payload_layout_survive_save_close_reopen() {
    let (mut state, root) = state_in("parts");
    state.load_preset("A320-200");
    let engine = engine_type(&state);
    let mission = pointer(&state, "/mission");
    let departure = pointer(&state, "/departure_airport");
    let cabin = pointer(&state, "/cabin");
    let cabin_preset = pointer(&state, "/requirements/cabin_preset");
    for kind in [
        ObjectKind::Engine,
        ObjectKind::MissionPlan,
        ObjectKind::PayloadLayout,
    ] {
        assert!(state.library_save(kind, "Part", ""), "{kind:?}");
    }
    state.load_preset("A220-300");
    assert_ne!(engine_type(&state), engine);
    for kind in [
        ObjectKind::Engine,
        ObjectKind::MissionPlan,
        ObjectKind::PayloadLayout,
    ] {
        let entry = entry_of(&state, kind);
        assert!(state.library_open(&entry), "{kind:?}");
    }
    assert_eq!(engine_type(&state), engine);
    assert_eq!(pointer(&state, "/mission"), mission);
    assert_eq!(pointer(&state, "/departure_airport"), departure);
    assert_eq!(pointer(&state, "/cabin"), cabin);
    assert_eq!(pointer(&state, "/requirements/cabin_preset"), cabin_preset);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_custom_airport_and_a_custom_airfoil_survive_save_close_reopen() {
    let (mut state, root) = state_in("registries");
    // The registries are process-wide: put back what they held when done.
    let airports_before = alas_config::airport_io::registered_custom_airports();
    let airfoils_before = alas_geom::airfoil_io::records();
    let airport = alas_config::airport_io::CustomAirport {
        icao: "ZZQQ".to_owned(),
        name: "Library Test Field (ZZQQ)".to_owned(),
        latitude_deg: 42.0,
        longitude_deg: -8.0,
        isa_delta_c: 3.0,
        altitude_m: 120.0,
        runway_lengths_m: vec![2_400.0],
        declared_toda_m: Some(2_500.0),
        declared_lda_m: Some(2_300.0),
        provenance: alas_config::airport_io::AirportProvenance::default(),
        departure_surveys: Vec::new(),
    };
    alas_config::airport_io::register_custom_airport(airport).unwrap();
    assert!(state.library_save(ObjectKind::Airport, "Field", "Library Test Field (ZZQQ)"));
    alas_config::airport_io::replace_custom_airports(airports_before.clone()).unwrap();
    assert!(alas_config::airport_io::find_custom("ZZQQ").is_none());
    assert!(state.library_open(&entry_of(&state, ObjectKind::Airport)));
    let restored = alas_config::airport_io::find_custom("ZZQQ").expect("airport restored");
    assert_eq!(restored.declared_toda_m, Some(2_500.0));
    alas_config::airport_io::replace_custom_airports(airports_before.clone()).unwrap();

    let points: Vec<String> = (0..=40)
        .map(|index| {
            let angle = std::f64::consts::PI * f64::from(index) / 40.0;
            let x = 0.5 * (1.0 + angle.cos());
            let y = 0.06 * (1.0 - angle.cos()) * angle.sin().signum().max(0.0);
            format!("{x:.5} {y:.5}")
        })
        .chain((1..40).map(|index| {
            let angle = std::f64::consts::PI * f64::from(index) / 40.0;
            let x = 0.5 * (1.0 - angle.cos());
            let y = 0.06 * (1.0 - angle.cos()) * angle.sin();
            format!("{x:.5} {:.5}", -y)
        }))
        .chain(std::iter::once("1.00000 0.00000".to_owned()))
        .collect();
    let text = format!("Library Test Foil\n{}\n", points.join("\n"));
    let imported = alas_geom::airfoil_io::parse_dat_with_provenance(&text, "test").unwrap();
    alas_geom::airfoil_io::replace_records(vec![alas_geom::airfoil_io::AirfoilRecord::from(
        &imported,
    )])
    .unwrap();
    assert!(state.library_save(ObjectKind::Airfoil, "Foil", "Library Test Foil"));
    alas_geom::airfoil_io::replace_records(airfoils_before.clone()).unwrap();
    assert!(state.library_open(&entry_of(&state, ObjectKind::Airfoil)));
    assert!(alas_geom::airfoil_io::get("Library Test Foil").is_some());
    alas_geom::airfoil_io::replace_records(airfoils_before).unwrap();
    alas_config::airport_io::replace_custom_airports(airports_before).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn add_on_settings_survive_save_close_reopen() {
    let (mut state, root) = state_in("addons");
    state
        .addons
        .settings
        .enabled
        .insert("demo@1".to_owned(), true);
    assert!(state.library_save(ObjectKind::AddonSettings, "Settings", ""));
    state.addons.settings.enabled.clear();
    assert!(state.library_open(&entry_of(&state, ObjectKind::AddonSettings)));
    assert_eq!(state.addons.settings.enabled.get("demo@1"), Some(&true));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn results_need_a_run_to_save_and_reopen_restores_their_aircraft() {
    let (mut state, root) = state_in("results");
    state.load_preset("A320-200");
    assert!(!state.library_save(ObjectKind::Results, "Run", ""));
    let wing = pointer(&state, "/geometry/wing");
    let body = json!({"summary": {}, "workspace": state.workspace_document()});
    state.load_preset("A220-300");
    state.library_apply(ObjectKind::Results, &body).unwrap();
    assert_eq!(pointer(&state, "/geometry/wing"), wing);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_file_of_the_wrong_kind_is_refused_and_leaves_the_case_alone() {
    let (mut state, root) = state_in("wrong");
    state.load_preset("A320-200");
    assert!(state.library_save(ObjectKind::Engine, "Engine", ""));
    let before = state.config_values.clone();
    let path = entry_of(&state, ObjectKind::Engine).path;
    assert!(file_library::read(ObjectKind::Airfoil, &path).is_err());
    assert!(state
        .library_apply(ObjectKind::Airport, &json!({"nope": 1}))
        .is_err());
    assert_eq!(state.config_values, before);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_engine_file_cannot_change_the_locked_nacelle_of_a_registered_aircraft() {
    let (mut state, root) = state_in("engine-lock");
    state.load_preset("A320-200");
    assert!(state.manual_geometry_locked());
    let radius = pointer(&state, "/geometry/engine/radius_scale_m");
    let profile = pointer(&state, "/geometry/engine/nacelle_profile");
    let mut file = state.library_capture(ObjectKind::Engine, "").unwrap();
    file["radius_scale_m"] = json!(2.5);
    file["nacelle_profile"] = json!([[0.0, 0.9], [1.0, 0.0]]);
    state.library_apply(ObjectKind::Engine, &file).unwrap();
    assert_eq!(pointer(&state, "/geometry/engine/radius_scale_m"), radius);
    assert_eq!(pointer(&state, "/geometry/engine/nacelle_profile"), profile);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn only_a_file_holding_the_whole_case_settles_unsaved_edits() {
    let (mut state, root) = state_in("dirty");
    state.load_preset("A320-200");
    state.reset_undo_baseline();
    state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(805.0);
    state.on_config_modified();
    assert!(state.has_unsaved_edits());
    assert!(state.library_save(ObjectKind::Engine, "Part", ""));
    assert!(state.library_save(ObjectKind::MissionPlan, "Plan", ""));
    assert!(state.has_unsaved_edits(), "a part of the case was saved");
    assert!(state.library_save(ObjectKind::Aircraft, "Whole", ""));
    assert!(!state.has_unsaved_edits());
    // Exit now asks only when something changed since.
    state.guarded(crate::shell_confirm::Pending::Preset("A220-300".to_owned()));
    assert!(
        state.shell.pending.is_none(),
        "nothing unsaved: it ran at once"
    );
    state.config_values["mass_model"]["fuel_density_kg_m3"] = json!(795.0);
    state.on_config_modified();
    state.guarded(crate::shell_confirm::Pending::Exit);
    assert!(state.shell.pending.is_some());
    state.shell.pending = None;
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_engine_round_trips_its_nacelle_when_the_case_is_not_locked() {
    let (mut state, root) = state_in("engine-nacelle");
    state.load_preset("A320-200");
    state.active_preset = String::new();
    assert!(!state.manual_geometry_locked());
    state.config_values["geometry"]["engine"]["radius_scale_m"] = json!(1.31);
    state.config_values["geometry"]["engine"]["nacelle_profile"] = json!([[0.0, 0.9], [1.0, 0.0]]);
    let radius = pointer(&state, "/geometry/engine/radius_scale_m");
    let profile = pointer(&state, "/geometry/engine/nacelle_profile");
    assert!(state.library_save(ObjectKind::Engine, "Free", ""));
    state.config_values["geometry"]["engine"]["radius_scale_m"] = json!(1.0);
    state.config_values["geometry"]["engine"]["nacelle_profile"] = json!([[0.0, 0.5], [1.0, 0.0]]);
    let entry = entry_of(&state, ObjectKind::Engine);
    assert!(state.library_open(&entry));
    assert_eq!(pointer(&state, "/geometry/engine/radius_scale_m"), radius);
    assert_eq!(pointer(&state, "/geometry/engine/nacelle_profile"), profile);
    let _ = std::fs::remove_dir_all(root);
}
