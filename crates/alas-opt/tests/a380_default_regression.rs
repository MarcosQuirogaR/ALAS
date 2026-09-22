// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The unchanged A380-800 default optimisation route.
//!
//! A user selected the A380-800 preset in the GUI with VLM optimisation and
//! default settings, and the run ended with no feasible design at all:
//!
//! ```text
//! NoFeasibleDesign {
//!   evaluated_candidates: 1540,
//!   rejection_reason_counts: {"geometry_build": 32, "mass_coordinates": 1508}
//! }
//! ```
//!
//! 1508 of 1540 is not a search-effort result. A differential-evolution run
//! seeds near its initial design, so if essentially the whole population is
//! refused then the *nominal* registered design is refused too, and no
//! population size or generation count can repair that. This file pins the
//! nominal candidate rather than the search: it is the narrowest
//! deterministic statement of the same defect, it runs in one coupled
//! evaluation instead of 1540, and it cannot be made to pass by widening a
//! bound or buying more iterations.
//!
//! SI throughout; body frame origin at the fuselage nose, +x aft,
//! +y starboard, +z up.

// An integration test asserting on values it constructed here.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::{assess_candidate, DesignObjective};

/// Load a registered preset exactly the way selecting it in the GUI does:
/// the preset name alone, with every other setting left at its default.
///
/// Selecting a *registered aircraft* lands in `DesignMode::ReferenceAdaptation`,
/// not the `DesignSpaceConfig::default()` of `CleanSheet`: loading a preset
/// document overrides the struct default, because a registered type is
/// adapted from its own reference rather than drawn from a blank sheet. That
/// is the mode the all-preset benchmark runs and the mode the user's run was
/// in, and it is asserted here so this fixture cannot silently drift onto a
/// different route from the one that failed.
fn default_preset_route(name: &str) -> (AlasConfig, Vec<f64>) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .unwrap_or_else(|error| panic!("the {name} preset loads: {error}"));
    assert_eq!(
        config.optimizer.design_space.mode,
        alas_config::DesignMode::ReferenceAdaptation,
        "selecting the registered {name} with defaults is a reference-adaptation design space"
    );
    let nominal = alas_config::presets::get(name)
        .unwrap_or_else(|error| panic!("{name} is registered: {error}"))
        .design_vector
        .to_array();
    (config, nominal)
}

/// The registered A380-800 nominal design must be an evaluable aircraft on
/// the same candidate path the optimizer uses.
///
/// This is the user-facing contract: an unchanged preset, selected with
/// defaults, has to produce at least one candidate the evaluator will accept.
/// The failure message carries the rejection reason so the cause is named
/// rather than inferred.
#[test]
fn the_a380_default_nominal_candidate_is_evaluable() {
    let (config, nominal) = default_preset_route("A380-800");
    let objective = DesignObjective::new(config);
    if let Err(reason) = assess_candidate(&objective, &nominal) {
        panic!(
            "the unchanged A380-800 default nominal candidate was rejected as `{reason}`; \
             a default preset the GUI offers must be evaluable without custom settings"
        );
    }
}

/// The same contract for every registered preset the GUI lists.
///
/// The A380 is the one the user hit, but a nominal design that its own
/// evaluator refuses is a defect on any preset, and finding a second one by
/// accident later is worse than finding it here. Presets are reported
/// together so one failure does not hide the rest.
#[test]
fn every_registered_preset_nominal_candidate_is_evaluable() {
    let mut rejected = Vec::new();
    for name in alas_config::presets::available() {
        let (config, nominal) = default_preset_route(name);
        let objective = DesignObjective::new(config);
        if let Err(reason) = assess_candidate(&objective, &nominal) {
            rejected.push(format!("{name}: {reason}"));
        }
    }
    assert!(
        rejected.is_empty(),
        "registered presets whose own nominal design is not evaluable:\n  {}",
        rejected.join("\n  ")
    );
}
