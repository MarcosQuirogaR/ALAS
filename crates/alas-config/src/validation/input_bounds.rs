// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Input-bound rules: preset identity, fuel policy/tank layout, solver worker
//! count and rib-count override. Split out of `validation.rs` to keep that file
//! at its frozen review size; see `docs/source-size-budgets.tsv`.

use super::{Severity, ValidationIssue};
use crate::AlasConfig;

/// A non-empty `preset` must name a registered aircraft; an unknown name used
/// to load silently as a defaults-only configuration.
/// An empty name is the preset-less configuration and is valid.
pub(super) fn preset_is_registered(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Some(message) = config.takeoff_override_issue() {
        issues.push(ValidationIssue {
            field_path: "toda_override_m".to_owned(),
            message,
            severity: Severity::Error,
        });
    }
    for (field, code) in [
        ("landing_gear.mlg_tire", &config.landing_gear.mlg_tire),
        ("landing_gear.nlg_tire", &config.landing_gear.nlg_tire),
    ] {
        if code != "auto" && !crate::landing_gear::CERTIFIED_TIRE_CODES.contains(&code.as_str()) {
            issues.push(ValidationIssue {
                field_path: field.to_owned(),
                message: format!(
                    "Tire {code:?} is not a certified tire designation; choose one of: auto, {}.",
                    crate::landing_gear::CERTIFIED_TIRE_CODES.join(", ")
                ),
                severity: Severity::Error,
            });
        }
    }
    if let Some(message) = config.landing_override_issue() {
        issues.push(ValidationIssue {
            field_path: "lda_override_m".to_owned(),
            message,
            severity: Severity::Error,
        });
    }
    let name = config.preset.as_str();
    if name.is_empty() || crate::presets::get(name).is_ok() {
        return issues;
    }
    let known: Vec<&str> = crate::presets::registry()
        .iter()
        .map(|preset| preset.name)
        .collect();
    issues.push(ValidationIssue {
        field_path: "preset".to_owned(),
        message: format!(
            "Preset {name:?} is not a registered aircraft; choose one of: {}, or leave it blank.",
            known.join(", ")
        ),
        severity: Severity::Error,
    });
    issues
}

/// Largest explicit `optimizer.solver.workers` accepted. `0` (every reported
/// thread) and any count up to this bound are valid; a larger value is a typo,
/// not a machine.
pub const MAX_SOLVER_WORKERS: i64 = 1024;

/// Bounds of `structures.num_ribs_override` (rib stations along one semi-span).
pub const NUM_RIBS_OVERRIDE_RANGE: std::ops::RangeInclusive<i64> = 2..=1000;

/// The fuel policy, tank layout and mission plan carry their own typed validators; they are
/// part of the blocking validation so a bad user-authored value is reported.
pub(super) fn fuel_and_plan_inputs_are_valid(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Err(message) = config.fuel_policy.validate() {
        issues.push(ValidationIssue {
            field_path: "fuel_policy".to_owned(),
            message,
            severity: Severity::Error,
        });
    }
    if let Err(message) = config.fuel_tanks.validate() {
        issues.push(ValidationIssue {
            field_path: "fuel_tanks".to_owned(),
            message,
            severity: Severity::Error,
        });
    }
    issues.extend(config.mission.plan.issue());
    issues.extend(config.mission.plan.items_issue(&config.cabin.payload_items));
    issues
}

/// A negative worker count was silently coerced to one thread; an enormous
/// one would try to spawn that many. Valid inputs (`0` or `1..=MAX`) are
/// unchanged.
pub(super) fn solver_workers_are_in_range(config: &AlasConfig) -> Vec<ValidationIssue> {
    let workers = config.optimizer.solver.workers;
    if (0..=MAX_SOLVER_WORKERS).contains(&workers) {
        return Vec::new();
    }
    vec![ValidationIssue {
        field_path: "optimizer.solver.workers".to_owned(),
        message: format!(
            "Worker thread count {workers} must lie in 0..={MAX_SOLVER_WORKERS} (0 uses every thread the machine reports)."
        ),
        severity: Severity::Error,
    }]
}

/// An explicit rib count must describe at least root and tip and stay finite
/// enough to be a plausible structure.
pub(super) fn rib_count_override_is_in_range(config: &AlasConfig) -> Vec<ValidationIssue> {
    match config.structures.num_ribs_override {
        Some(count) if !NUM_RIBS_OVERRIDE_RANGE.contains(&count) => vec![ValidationIssue {
            field_path: "structures.num_ribs_override".to_owned(),
            message: format!(
                "Rib count override {count} must lie in {}..={} (leave blank for the derived count).",
                NUM_RIBS_OVERRIDE_RANGE.start(),
                NUM_RIBS_OVERRIDE_RANGE.end()
            ),
            severity: Severity::Error,
        }],
        _ => Vec::new(),
    }
}

/// The struts and fairing of a centreline pod can be built. A negative chord would
/// otherwise reach the mass model as a negative painted area.
pub(super) fn centre_engine_surfaces_can_be_built(config: &AlasConfig) -> Vec<ValidationIssue> {
    let pod = config.geometry.engine.centre_engine.as_ref();
    pod.and_then(crate::CentreEngineInstallation::surface_issue)
        .map(|issue| ValidationIssue {
            field_path: "geometry.engine.centre_engine".to_owned(),
            message: format!("Centre engine {issue}."),
            severity: Severity::Error,
        })
        .into_iter()
        .collect()
}

/// A declared rear-fuselage mount must place the engines: its inlet ahead of the
/// tail end, aft of the nose of the shortest fuselage the run may build, and a
/// finite axis height. An invalid one would otherwise be read as no mount at
/// all, silently hanging the nacelles on the wing.
pub(super) fn aft_fuselage_mount_is_valid(config: &AlasConfig) -> Vec<ValidationIssue> {
    let Some(mount) = config.geometry.engine.aft_fuselage.as_ref() else {
        return Vec::new();
    };
    let shortest_m = shortest_fuselage_m(config);
    if mount.fits_fuselage(shortest_m) {
        return Vec::new();
    }
    vec![ValidationIssue {
        field_path: "geometry.engine.aft_fuselage".to_owned(),
        message: format!(
            "A rear-fuselage mount needs its inlet ahead of the tail end, aft of the nose of the shortest fuselage of the run ({shortest_m:.2} m), and a finite axis height."
        ),
        severity: Severity::Error,
    }]
}

/// The shortest fuselage the run may build: the lower end of its fuselage-length
/// range, or the start when that lies below it.
fn shortest_fuselage_m(config: &AlasConfig) -> f64 {
    let nominal = config.configured_nominal_design();
    config
        .optimizer
        .design_space
        .envelope(&nominal)
        .into_iter()
        .find(|variable| variable.name == "fuselage_length_m")
        .map_or(nominal.fuselage_length_m, |variable| {
            variable.lower.min(variable.nominal)
        })
}

/// A declared count of reversing engines names at least one and no more than
/// the aircraft has; equation 86 would otherwise price reversers on engines that
/// do not exist, or on none while the installation says it has them.
pub(super) fn reverser_count_fits_the_engines(config: &AlasConfig) -> Vec<ValidationIssue> {
    let structure = &config.mass_model.flops_structure;
    let engines = config.geometry.engine.spanwise_positions_m.len();
    match structure.thrust_reverser_engine_count {
        Some(count) if structure.thrust_reversers_installed && (count == 0 || count > engines) => {
            vec![ValidationIssue {
                field_path: "mass_model.flops_structure.thrust_reverser_engine_count".to_owned(),
                message: format!(
                    "Engines with thrust reversers must lie between 1 and the {engines} engines installed; clear the reverser option instead of declaring none."
                ),
                severity: Severity::Error,
            }]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod reverser_count_tests {
    use super::*;

    #[test]
    fn the_a380_pair_fits_and_a_count_beyond_the_engines_does_not() {
        let mut config = AlasConfig {
            geometry: crate::presets::get("A380-800")
                .expect("A380")
                .geometry
                .clone(),
            ..AlasConfig::default()
        };
        for (count, issues) in [
            (None, 0),
            (Some(2), 0),
            (Some(4), 0),
            (Some(0), 1),
            (Some(5), 1),
        ] {
            config
                .mass_model
                .flops_structure
                .thrust_reverser_engine_count = count;
            assert_eq!(
                reverser_count_fits_the_engines(&config).len(),
                issues,
                "{count:?}"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod aft_fuselage_mount_tests {
    use super::*;

    #[test]
    fn an_invalid_mount_is_an_error_and_reads_as_no_mount() {
        let mut config = AlasConfig {
            geometry: crate::presets::get("LJ60").expect("LJ60").geometry.clone(),
            ..AlasConfig::default()
        };
        assert!(aft_fuselage_mount_is_valid(&config).is_empty());
        assert!(config.geometry.engine.rear_fuselage_mount().is_some());
        config.preset = "LJ60".to_owned();
        assert!(aft_fuselage_mount_is_valid(&config).is_empty());
        // Ahead of the nose of the LJ60 fuselage: valid in itself, refused here.
        config.geometry.engine.aft_fuselage = Some(crate::AftFuselageEngineInstallation {
            inlet_ahead_of_tail_m: 30.0,
            axis_z_m: 0.725,
        });
        assert_eq!(aft_fuselage_mount_is_valid(&config).len(), 1);
        for (inlet, axis) in [(-1.0, 0.5), (f64::NAN, 0.5), (3.0, f64::INFINITY)] {
            config.geometry.engine.aft_fuselage = Some(crate::AftFuselageEngineInstallation {
                inlet_ahead_of_tail_m: inlet,
                axis_z_m: axis,
            });
            assert_eq!(
                aft_fuselage_mount_is_valid(&config).len(),
                1,
                "{inlet} {axis}"
            );
            assert!(config.geometry.engine.rear_fuselage_mount().is_none());
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod centre_engine_tests {
    use super::*;

    #[test]
    fn a_strut_chord_not_above_zero_is_refused_by_name() {
        let mut config = AlasConfig {
            geometry: crate::presets::get("DC-10")
                .expect("preset")
                .geometry
                .clone(),
            ..AlasConfig::default()
        };
        assert!(centre_engine_surfaces_can_be_built(&config).is_empty());
        for chord in [0.0, -0.5, f64::NAN] {
            let pod = config.geometry.engine.centre_engine.as_mut().expect("pod");
            pod.struts[0].chord_at_pod_m = chord;
            let issues = centre_engine_surfaces_can_be_built(&config);
            assert_eq!(issues.len(), 1, "{chord}");
            assert!(
                issues[0].message.contains("strut 0"),
                "{}",
                issues[0].message
            );
            assert!(!config
                .geometry
                .engine
                .centre_engine
                .as_ref()
                .expect("pod")
                .is_valid());
        }
    }
}

#[cfg(test)]
mod tire_tests {
    use super::*;

    #[test]
    fn an_unknown_certified_tire_is_an_error_naming_the_choices() {
        let mut config = AlasConfig::default();
        assert!(preset_is_registered(&config).is_empty());
        config.landing_gear.mlg_tire = "46x17.0R20/32PR typo".to_owned();
        config.landing_gear.nlg_tire = "49x17/32PR".to_owned();
        let issues = preset_is_registered(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_path, "landing_gear.mlg_tire");
        assert!(issues[0].message.contains("H49x19.0-22/32PR"));
    }

    #[test]
    fn every_preset_names_a_listed_tire_or_auto() {
        for preset in crate::presets::registry() {
            let gear = &preset.landing_gear;
            for code in [&gear.mlg_tire, &gear.nlg_tire] {
                assert!(
                    code == "auto"
                        || crate::landing_gear::CERTIFIED_TIRE_CODES.contains(&code.as_str()),
                    "{}: {code}",
                    preset.name
                );
            }
        }
    }
}
