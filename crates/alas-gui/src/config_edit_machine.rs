// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which settings survive a change of aircraft: this machine's own.

use alas_config::AlasConfig;

/// Preserve installed tools, local data and machine memory budgets. Do not
/// retain solver switches, physics settings or the dispatch account: the
/// latter can override the new aircraft's route with an unrelated flight.
pub(crate) fn preserve_machine_locations(config: &mut AlasConfig, previous: &AlasConfig) {
    config.mses.mses_dir.clone_from(&previous.mses.mses_dir);
    config.mses.osmap_path.clone_from(&previous.mses.osmap_path);
    let mission = &mut config.mission;
    mission
        .navdata_dir
        .clone_from(&previous.mission.navdata_dir);
    mission.routes_dir.clone_from(&previous.mission.routes_dir);
    mission
        .texture_path
        .clone_from(&previous.mission.texture_path);
    macro_rules! preserve_structure_location {
        ($($field:ident),+ $(,)?) => { $(
            config.structures.$field.clone_from(&previous.structures.$field);
        )+ };
    }
    preserve_structure_location!(
        nastran_exe_path,
        nastran_solver_path,
        nastran_memory_mb,
        nastran95_dir_path,
        nastran95_runtime_path,
        nastran95_rf_stage_path,
        nastran95_open_core_words,
        patran_exe_path,
    );
}

/// Keep the external-tool switches the user has set: MSES, the four
/// downstream stages and the two structures exports. A preset load restores
/// the preset's own values only the first time, when the switches still hold
/// their defaults.
pub(crate) fn preserve_tool_toggles(config: &mut AlasConfig, previous: &AlasConfig) {
    config.mses.enabled = previous.mses.enabled;
    config.downstream.openvsp = previous.downstream.openvsp;
    config.downstream.vspaero = previous.downstream.vspaero;
    config.downstream.avl = previous.downstream.avl;
    config.downstream.flowunsteady = previous.downstream.flowunsteady;
    config.structures.run_nastran = previous.structures.run_nastran;
    config.structures.run_patran_export = previous.structures.run_patran_export;
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use serde_json::json;

    fn toggles(state: &AppState) -> [bool; 7] {
        let config = state.typed_config().expect("valid configuration");
        [
            config.mses.enabled,
            config.downstream.openvsp,
            config.downstream.vspaero,
            config.downstream.avl,
            config.downstream.flowunsteady,
            config.structures.run_nastran,
            config.structures.run_patran_export,
        ]
    }

    #[test]
    fn a_preset_load_keeps_the_tool_switches_the_user_set() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        // Every switch defaults on except VSPAERO, the slowest stage.
        assert_eq!(
            toggles(&state),
            [true, true, false, true, true, true, true],
            "the first load keeps the defaults"
        );
        for (group, key) in [
            ("mses", "enabled"),
            ("downstream", "openvsp"),
            ("downstream", "vspaero"),
            ("downstream", "avl"),
            ("downstream", "flowunsteady"),
            ("structures", "run_nastran"),
            ("structures", "run_patran_export"),
        ] {
            state.config_values[group][key] = json!(false);
        }
        state.load_preset("LJ60");
        assert_eq!(toggles(&state), [false; 7], "the user's switches survive");
        state.config_values["downstream"]["avl"] = json!(true);
        state.load_preset("ATR72-600");
        assert_eq!(
            toggles(&state),
            [false, false, false, true, false, false, false]
        );
    }
}
