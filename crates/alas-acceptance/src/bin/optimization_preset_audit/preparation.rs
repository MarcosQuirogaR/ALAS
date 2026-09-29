// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Match the GUI's automatic route-profile initialization without a GUI dependency.
use alas_config::{airport_dataset, airports::Airport, presets, AlasConfig};

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

fn airport(name: &str) -> Option<Airport> {
    let record = airport_dataset::resolve(name).ok()?;
    Some(Airport {
        name: record.name.value.unwrap_or_else(|| name.to_owned()),
        icao: record.icao.value.unwrap_or_default(),
        elevation_m: record.elevation_m.value?,
        toda_m: record.toda_m.value.unwrap_or_default(),
        lda_m: record.lda_m.value.unwrap_or_default(),
        isa_deviation_c: record.isa_deviation_c.value.unwrap_or_default(),
        notes: String::new(),
        latitude_deg: record.latitude_deg.value?,
        longitude_deg: record.longitude_deg.value?,
    })
}

/// Same endpoint resolution, great-circle radius, proposal, preset leg cap
/// and fraction split as GUI `mission_profile_inputs::initialize_route_profile`.
/// Aircraft-specific speeds/rates/altitudes remain unchanged.
pub(super) fn initialize_route_profile(config: &mut AlasConfig) {
    let Some(origin) = airport(&config.departure_airport) else {
        return;
    };
    let Some(destination) = airport(&config.arrival_airport) else {
        return;
    };
    let lat1 = origin.latitude_deg.to_radians();
    let lat2 = destination.latitude_deg.to_radians();
    let dlat = lat2 - lat1;
    let dlon = destination.longitude_deg.to_radians() - origin.longitude_deg.to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let distance_m = 2.0 * 6_371_000.0 * a.sqrt().atan2((1.0 - a).sqrt());
    let Ok(proposal) =
        alas_mission::propose_profile_for_route(config, &origin, &destination, distance_m)
    else {
        return;
    };
    let Ok(preset) = presets::get(&config.preset) else {
        return;
    };
    let preset_profile = preset.operational_mission_defaults().profile;
    let max_legs = [
        preset_profile.cruise_1_distance_fraction,
        preset_profile.cruise_2_distance_fraction,
        preset_profile.cruise_3_distance_fraction,
    ]
    .iter()
    .rposition(|f| *f > 1.0e-6)
    .map_or(1, |i| i + 1);
    let profile = &mut config.mission.profile;
    let fractions = match proposal.active_cruise_legs.clamp(1, max_legs.clamp(1, 3)) {
        1 => [1.0, 0.0, 0.0],
        2 => [0.5, 0.5, 0.0],
        _ => {
            let mut fractions = [
                profile.cruise_1_distance_fraction,
                profile.cruise_2_distance_fraction,
                profile.cruise_3_distance_fraction,
            ];
            if !fractions.iter().all(|f| f.is_finite() && *f > 1.0e-9) {
                let defaults = alas_config::MissionProfileConfig::default();
                fractions = [
                    defaults.cruise_1_distance_fraction,
                    defaults.cruise_2_distance_fraction,
                    defaults.cruise_3_distance_fraction,
                ];
            }
            let total: f64 = fractions.iter().sum();
            fractions.map(|f| f / total)
        }
    };
    [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ] = fractions;
}

/// Explicit audit scope; the default keeps all configured external requests.
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
            assert_eq!(*profile, before, "{} schedule", preset.name);
        }
    }
}
