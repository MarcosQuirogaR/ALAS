// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Match the GUI's automatic route-profile initialization without a GUI dependency.
use alas_config::AlasConfig;

pub(super) use super::route_profile::initialize_route_profile;

/// Apply the same persisted installation/data locations as GUI startup.
pub(super) fn apply_machine_preferences(config: &mut AlasConfig, p: &alas_exec::ToolPreferences) {
    macro_rules! copy {
        ($target:expr, $source:expr) => {
            if let Some(value) = &$source {
                $target.clone_from(value);
            }
        };
    }
    copy!(config.mses.mses_dir, p.mses_dir);
    copy!(config.structures.nastran_exe_path, p.nastran_exe);
    copy!(config.structures.nastran_solver_path, p.nastran_solver);
    copy!(config.structures.nastran95_dir_path, p.nastran95_dir);
    copy!(
        config.structures.nastran95_runtime_path,
        p.nastran95_runtime
    );
    copy!(
        config.structures.nastran95_rf_stage_path,
        p.nastran95_rf_stage
    );
    copy!(
        config.structures.nastran95_open_core_words,
        p.nastran95_open_core_words
    );
    copy!(config.structures.patran_exe_path, p.patran_exe);
    copy!(config.mission.navdata_dir, p.navdata_dir);
    copy!(config.mission.routes_dir, p.routes_dir);
}

/// Explicit audit scope; native execution is the default, external tools require opt-in.
pub(super) fn execution_scope(native_only: bool) -> &'static str {
    if native_only {
        "native_only"
    } else {
        "all_requested"
    }
}

/// Alter execution switches only. Native sizing, mission, requirements and search stay intact.
pub(super) fn apply_execution_scope(config: &mut AlasConfig, native_only: bool) {
    if native_only {
        config.mses.enabled = false;
        config.downstream.openvsp = false;
        config.downstream.vspaero = false;
        config.downstream.avl = false;
        config.downstream.flowunsteady = false;
        config.structures.run_nastran = false; // Governs MSC and NASTRAN-95.
        config.structures.run_patran_export = false;
    }
}

/// Centralized decoration includes every early failure, panic and cancelled row.
pub(super) fn record_scope(row: &mut serde_json::Value, native_only: bool) {
    let scope = execution_scope(native_only);
    row["execution_scope"] = scope.into();
    row["configuration"]["execution_scope"] = scope.into();
    row["external_completion_claimed"] = false.into();
    if row.get("optimizer").is_none() {
        return; // No newly written effective config on an early failure.
    }
    if let Some(directory) = row["output_dir"].as_str() {
        let path = std::path::Path::new(directory).join("effective_config.yaml");
        if let Ok(yaml) = std::fs::read_to_string(&path) {
            let _ = std::fs::write(path, format!("# execution_scope: {scope}\n{yaml}"));
        }
    }
}

#[cfg(test)]
// A registered preset that fails to load is a failed assertion.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::presets;

    #[test]
    fn every_registered_route_initializes_without_changing_aircraft_schedule() {
        for preset in presets::registry() {
            let mut config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
                .expect("registered preset");
            let before = config.mission.profile.clone();
            initialize_route_profile(&mut config);
            let profile = &mut config.mission.profile;
            let fractions = [
                profile.cruise_1_distance_fraction,
                profile.cruise_2_distance_fraction,
                profile.cruise_3_distance_fraction,
            ];
            assert!((fractions.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
            profile.cruise_1_distance_fraction = before.cruise_1_distance_fraction;
            profile.cruise_2_distance_fraction = before.cruise_2_distance_fraction;
            profile.cruise_3_distance_fraction = before.cruise_3_distance_fraction;
            profile.initial_climb_altitude_fraction = before.initial_climb_altitude_fraction;
            profile.step_climb_1_altitude_fraction = before.step_climb_1_altitude_fraction;
            assert_eq!(*profile, before, "{} schedule", preset.name);
        }
    }
}
