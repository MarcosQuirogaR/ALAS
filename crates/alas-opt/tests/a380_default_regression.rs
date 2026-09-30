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

/// The A380-800 default search reports a typed, attributable cause for every
/// rejected candidate instead of the opaque `mass_coordinates`/`unknown`
/// bucket the original report carried.
///
/// This does not assert that the default run finds a feasible design (the
/// A380 takeoff CG is aft of its ground limit on the pipeline basis, see
/// `alas-pipeline/tests/a380_ground_cg_limits.rs`), and it does not assert that any particular constraint appears: the typed
/// reason is whatever physical invariant the candidate actually violates.
/// The contract is that the search evaluates real candidates and names why
/// each was rejected in a form a caller can act on.
#[test]
fn the_a380_default_search_reports_a_typed_ground_reaction_cause() {
    let (mut config, _) = default_preset_route("A380-800");
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    // A reduced but honest budget (mirrors `staged_search.rs`'s convention):
    // small enough to run in a unit test, large enough that a differential-
    // evolution generation actually completes and the population's rejection
    // reasons are the search's own rather than a single seed point's.
    config.optimizer.solver.seed = Some(20_260_922);
    config.optimizer.solver.max_iterations = 3;
    config.optimizer.solver.population_size = 2;

    let mut optimizer = DesignOptimizer::new(config);
    match optimizer.run(None, None, None) {
        Ok(_) => {}
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
        }
        Err(error) => panic!("the A380-800 default route must not fail bounds/config: {error}"),
    }
}

/// Trim tanks fill last: at a mission-representative partial load the A380
/// tailplane trim tank is empty, every other tank is filled first, and the
/// fuel centre of gravity is forward of the full-tank one, so trim fuel no
/// longer drags the takeoff CG aft. Mechanism: a trim tank sits some 25 m aft
/// of the wing box, so fuel in it can only move the CG aft; the fuel computer
/// holds it empty on the ground unless the wing tanks are full [S Airbus A380
/// Flight Deck and Systems Briefing for Pilots, Issue 2, section 10.10].
/// Frame: x in metres aft of the nose tip.
#[test]
fn the_a380_trim_tank_is_empty_at_partial_fuel_and_does_not_move_the_cg_aft() {
    use alas_mass::tanks::{FuelState, FuelTankLayout, TankKind};

    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" })).unwrap();
    let preset = alas_config::presets::get("A380-800").unwrap();
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    let (density_kg_m3, published_total_l) =
        alas_mass::product_stations::tank_reference(&config, &preset.design_vector);
    let layout = FuelTankLayout::resolve(
        &plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    )
    .unwrap();

    let capacity_kg = layout.usable_capacity_kg();
    let trim_capacity_kg: f64 = layout
        .tanks()
        .iter()
        .filter(|tank| tank.kind == TankKind::Trim)
        .map(|tank| tank.usable_capacity_kg)
        .sum();
    assert!(trim_capacity_kg > 0.0, "the A380 carries a trim tank");

    let trim_fill_kg = |state: &FuelState| -> f64 {
        let items = state.mass_items(&layout);
        layout
            .tanks()
            .iter()
            .filter(|tank| tank.kind == TankKind::Trim)
            .map(|tank| {
                items
                    .iter()
                    .find(|item| item.id == tank.id)
                    .map_or(0.0, |item| item.mass_kg)
            })
            .sum()
    };

    // Every load up to "all non-trim tanks full" leaves the trim tank empty.
    let wings_full = (capacity_kg - trim_capacity_kg) / capacity_kg;
    for fraction in [0.05, 0.3735, 0.6, wings_full] {
        let state = layout.distribute(fraction * capacity_kg).unwrap();
        assert!(
            trim_fill_kg(&state) < 1.0e-6,
            "trim tank holds fuel at fraction {fraction}"
        );
    }

    let partial = layout.distribute(0.3735 * capacity_kg).unwrap();
    let full = layout.distribute(capacity_kg).unwrap();
    assert!(trim_fill_kg(&full) > 0.99 * trim_capacity_kg);
    let partial_x = partial.properties(&layout).cg_m[0];
    let full_x = full.properties(&layout).cg_m[0];
    assert!(
        partial_x < full_x,
        "partial-load fuel CG {partial_x:.2} m must be forward of the full-tank CG {full_x:.2} m"
    );
}
