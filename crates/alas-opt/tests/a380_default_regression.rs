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
use alas_opt::{assess_candidate, DesignObjective, DesignOptimizer, OptimizationError};

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

/// The default A380-800 search reports a typed, attributable ground-reaction
/// cause instead of the opaque `mass_coordinates`/`unknown` bucket the
/// original report carried.
///
/// # What this test does and does not claim
///
/// This does **not** assert that the default run finds a feasible design.
/// The nominal A380-800's own `AnalyzedTakeoff` loading state (full design
/// mission fuel over the default EGLL-OMDB route) has its physical CG about
/// 0.37 m aft of the weighted main-gear station, which the search correctly
/// rejects on `min_nose_gear_load` (and, more tightly, `static_margin_floor`)
/// -- see `the_a380_default_nominal_candidate_is_evaluable` above: the
/// nominal candidate is *evaluable* (a real, typed, sized aircraft) but not
/// *hard-feasible*.
///
/// The root cause is traced to `alas_mass::tanks::distribute`'s ground
/// fill-order rule (tanks burned last are filled first): the A380-800's
/// sourced burn order puts its tailplane trim tank third of four, so this
/// partial-fuel state fills the trim tank and the outer wing cells before
/// the inner feed tanks and pulls the fuel centre of gravity about 12.75 m
/// aft of an inner-first fill of the same mass -- see that module's own doc
/// comment and
/// `alas_mass::tanks::distribute::tests::a_partial_a380_load_fills_the_trim_and_outer_tanks_and_moves_the_fuel_aft`,
/// which already pins the consequence. That fill-order rule is explicitly
/// documented as unsourced (the real ground fill order lives in a
/// non-public Weight and Balance Manual) and is `alas-mass` territory, not
/// `alas-opt`'s: fixing it here would mean adjusting a design-space bound or
/// a search weight to paper over a genuine, correctly-flagged mass-model
/// gap, which is not a fix.
///
/// What *is* this crate's contract, and what regresses if it breaks, is that
/// the search evaluates real candidates and reports *why* they were
/// rejected in a form a caller can act on: not a single opaque phase label
/// covering 98% of the population.
#[test]
fn the_a380_default_search_reports_a_typed_ground_reaction_cause() {
    let (mut config, _) = default_preset_route("A380-800");
    // A reduced but honest budget (mirrors `staged_search.rs`'s convention):
    // small enough to run in a unit test, large enough that a differential-
    // evolution generation actually completes and the population's rejection
    // reasons are the search's own rather than a single seed point's.
    config.optimizer.solver.seed = Some(20_260_922);
    config.optimizer.solver.max_iterations = 3;
    config.optimizer.solver.population_size = 2;

    let mut optimizer = DesignOptimizer::new(config);
    match optimizer.run(None, None, None) {
        Ok(_) => {
            // A future `alas-mass` fix (a trim-tank-aware fill order, or the
            // missing WBM source) may make the default route fully feasible.
            // That is strictly better than today's contract and this test
            // must not forbid it.
        }
        Err(OptimizationError::NoFeasibleDesign(evidence)) => {
            assert!(
                evidence.evaluated_candidates > 0,
                "the search must evaluate real candidates before reporting no feasible design"
            );
            assert!(
                !evidence
                    .rejection_reason_counts
                    .contains_key("mass_coordinates"),
                "the rejection reason must name the physical invariant that failed, not the \
                 phase that noticed it: {:?}",
                evidence.rejection_reason_counts
            );
            assert!(
                !evidence.rejection_reason_counts.contains_key("unknown"),
                "every rejected candidate in this run has a typed reason: {:?}",
                evidence.rejection_reason_counts
            );
            assert!(
                evidence
                    .rejection_reason_counts
                    .contains_key("min_nose_gear_load"),
                "the known cause (the fuel-tank fill-order gap in \
                 alas_mass::tanks::distribute) must still be visible under its own name: {:?}",
                evidence.rejection_reason_counts
            );
        }
        Err(error) => panic!("the A380-800 default route must not fail bounds/config: {error}"),
    }
}
