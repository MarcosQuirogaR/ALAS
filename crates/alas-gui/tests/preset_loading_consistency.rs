// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{AlasConfig, DesignMode};
use alas_gui::state::AppState;
use serde_json::{json, Value};

// Preserve enum spellings and optional-field types while contaminating every
// numeric/boolean leaf, including settings added to the schema in the future.
fn contaminate(value: &mut Value) {
    match value {
        Value::Object(fields) => fields.values_mut().for_each(contaminate),
        Value::Array(items) => items.iter_mut().for_each(contaminate),
        Value::Bool(value) => *value = !*value,
        Value::Number(number) => {
            *number = if let Some(value) = number.as_i64() {
                (value + 1).into()
            } else {
                serde_json::Number::from_f64(number.as_f64().unwrap() + 1.0).unwrap()
            };
        }
        _ => {}
    }
}

#[test]
fn preset_selection_resets_every_study_group_and_survives_save_reload() {
    let mut state = AppState::default();
    let names: Vec<String> = state
        .preset_names
        .iter()
        .map(|(key, _)| key.clone())
        .collect();
    // Two opposite traversal orders expose stale aircraft-specific state.
    for name in names.iter().chain(names.iter().rev()) {
        let mut clean = AppState::default();
        clean.load_preset(name);
        let mut expected = clean.typed_config().unwrap();

        contaminate(&mut state.config_values);
        expected.structures.nastran_memory_mb = state.config_values["structures"]
            ["nastran_memory_mb"]
            .as_i64()
            .unwrap();
        state.config_values["mission"]["simbrief_username"] = json!("unrelated-dispatch");
        state.config_values["optimizer"]["design_space"]["mode"] =
            serde_json::to_value(DesignMode::CleanSheet).unwrap();
        assert!(
            state.typed_config().is_some(),
            "contamination must remain typed"
        );
        state
            .design_values
            .insert("obsolete_variable".into(), 999.0);
        state
            .bounds
            .insert("obsolete_variable".into(), (1.0, 999.0));
        state
            .selected_aux_preset
            .insert("solver".into(), "stale selection".into());
        state.load_preset(name);
        assert_eq!(
            state.typed_config().unwrap(),
            expected,
            "{name}: full configuration"
        );
        assert_eq!(state.design_values, clean.design_values, "{name}: design");
        assert_eq!(state.bounds, clean.bounds, "{name}: bounds");
        assert!(state.selected_aux_preset.is_empty());

        // This is the exact document and loader used by Save/Load, through
        // actual JSON encoding rather than an in-memory config-only copy.
        let text = serde_json::to_string(&state.workspace_document()).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        let mut reloaded = AppState::default();
        reloaded.apply_workspace_document(&saved).unwrap();
        assert_eq!(
            reloaded.typed_config().unwrap(),
            expected,
            "{name}: saved configuration"
        );
        assert_eq!(
            reloaded.design_values, state.design_values,
            "{name}: saved design"
        );
        assert_eq!(reloaded.bounds, state.bounds, "{name}: saved bounds");
    }
}

#[test]
fn switching_presets_loads_each_aircrafts_complete_physical_inputs() {
    let mut state = AppState::default();
    // Reproduce the logged selection order, then return from turboprop to jet.
    for name in [
        "AVE",
        "A340-300",
        "A380-800",
        "B787-9",
        "A320-200",
        "A220-300",
        "ATR72-600",
        "DC-10",
        "ATR72-600",
        "A220-300",
        "A320-200",
        "A380-800",
        "A340-300",
        "AVE",
    ] {
        state.load_preset(name);
        let actual = state.typed_config().expect("GUI configuration");
        let expected = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
        assert_eq!(actual.geometry, expected.geometry, "{name}: geometry");
        assert_eq!(actual.mass_model, expected.mass_model, "{name}: mass model");
        assert_eq!(actual.fuel_tanks, expected.fuel_tanks, "{name}: tanks");
        assert_eq!(
            actual.performance, expected.performance,
            "{name}: performance"
        );
        assert_eq!(actual.landing_gear, expected.landing_gear, "{name}: gear");
        assert_eq!(
            actual.structures.skin_material, expected.structures.skin_material,
            "{name}: skin"
        );
        assert_eq!(
            actual.structures.spar_web_material, expected.structures.spar_web_material,
            "{name}: web"
        );
        assert_eq!(
            actual.structures.spar_cap_material, expected.structures.spar_cap_material,
            "{name}: cap"
        );
        assert_eq!(
            actual.optimizer.design_space.mode,
            DesignMode::ReferenceAdaptation
        );
        assert_eq!(
            state.current_design().unwrap().fuselage_length_m,
            alas_config::presets::get(name)
                .unwrap()
                .design_vector
                .fuselage_length_m,
            "{name}: retain the reference fuselage"
        );
    }
}

#[test]
fn selecting_a_reference_preserves_solver_preferences_and_baseline_workflow() {
    let mut state = AppState::default();
    let mut config = state.typed_config().unwrap();
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
    config.structures.nastran_exe_path = "C:/custom/solver.exe".into();
    config.mission.navdata_dir = "C:/custom/navdata".into();
    state.config_values = serde_json::to_value(&config).unwrap();
    state.load_preset("A340-300");
    let actual = state.typed_config().unwrap();
    assert_eq!(
        actual.optimizer.design_space.mode,
        DesignMode::ReferenceAdaptation
    );
    assert_eq!(
        actual.structures.nastran_exe_path,
        config.structures.nastran_exe_path
    );
    assert_eq!(actual.mission.navdata_dir, config.mission.navdata_dir);

    state.set_design_mode(DesignMode::BaselineSandbox);
    state.load_preset("ATR72-600");
    assert_eq!(state.design_mode(), DesignMode::BaselineSandbox);
    assert!(!state.run_options.optimize);
}

#[test]
fn only_machine_locations_and_memory_budgets_survive_a_reference_change() {
    let mut state = AppState::default();
    let mut clean = AppState::default();
    clean.load_preset("ATR72-600");
    let mut expected = serde_json::to_value(clean.typed_config().unwrap()).unwrap();
    for (group, field) in [
        ("mses", "mses_dir"),
        ("mses", "osmap_path"),
        ("mission", "navdata_dir"),
        ("mission", "routes_dir"),
        ("mission", "texture_path"),
        ("structures", "nastran_exe_path"),
        ("structures", "nastran_solver_path"),
        ("structures", "nastran95_dir_path"),
        ("structures", "nastran95_runtime_path"),
        ("structures", "nastran95_rf_stage_path"),
        ("structures", "nastran95_open_core_words"),
        ("structures", "patran_exe_path"),
    ] {
        let location = json!(format!("custom/{field}"));
        state.config_values[group][field] = location.clone();
        expected[group][field] = location;
    }
    // Thirteenth preserved setting: a host resource, not aircraft physics.
    state.config_values["structures"]["nastran_memory_mb"] = json!(4096);
    expected["structures"]["nastran_memory_mb"] = json!(4096);
    state.load_preset("ATR72-600");
    assert_eq!(
        serde_json::to_value(state.typed_config().unwrap()).unwrap(),
        expected
    );
    let mut reloaded = AppState::default();
    reloaded
        .apply_workspace_document(&state.workspace_document())
        .unwrap();
    assert_eq!(
        serde_json::to_value(reloaded.typed_config().unwrap()).unwrap(),
        expected,
        "the saved aircraft must retain installation locations and memory budgets"
    );
}
