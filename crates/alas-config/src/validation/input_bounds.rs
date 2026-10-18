// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Input-bound rules: preset identity, fuel policy/tank layout, solver worker
//! count and rib-count override. Split out of `validation.rs` to keep that file
//! at its frozen review size; see `docs/source-size-budgets.tsv`.

use super::{Severity, ValidationIssue};
use crate::AlasConfig;

/// A non-empty `preset` must name a registered aircraft; an unknown name used
/// to load silently as a defaults-only configuration (owner decision AD4).
/// An empty name is the preset-less configuration and is valid.
pub(super) fn preset_is_registered(config: &AlasConfig) -> Vec<ValidationIssue> {
    let name = config.preset.as_str();
    if name.is_empty() || crate::presets::get(name).is_ok() {
        return Vec::new();
    }
    let known: Vec<&str> = crate::presets::registry()
        .iter()
        .map(|preset| preset.name)
        .collect();
    vec![ValidationIssue {
        field_path: "preset".to_owned(),
        message: format!(
            "Preset {name:?} is not a registered aircraft; choose one of: {}, or leave it blank.",
            known.join(", ")
        ),
        severity: Severity::Error,
    }]
}

/// Largest explicit `optimizer.solver.workers` accepted. `0` (every reported
/// thread) and any count up to this bound are valid; a larger value is a typo,
/// not a machine.
pub const MAX_SOLVER_WORKERS: i64 = 1024;

/// Bounds of `structures.num_ribs_override` (rib stations along one semi-span).
pub const NUM_RIBS_OVERRIDE_RANGE: std::ops::RangeInclusive<i64> = 2..=1000;

/// The fuel policy and tank layout carry their own typed validators; they are
/// part of the blocking validation so a bad user-authored value is reported.
pub(super) fn fuel_policy_and_tank_layout_are_valid(config: &AlasConfig) -> Vec<ValidationIssue> {
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
