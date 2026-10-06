// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the mission-sized objective (`alas_opt::mdo`).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, MtowSizing, ObjectiveKind};
use alas_opt::mdo::ResidualRole;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, assess_product_candidate, ConstraintFamily, DesignOptimizer};

fn block_fuel_config() -> AlasConfig {
    let mut config = AlasConfig::default();
    config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
    // These tests isolate mission sizing and residual-family behavior. The
    // transport body-attitude window is a separate clean-sheet requirement;
    // the canonical default vector is intentionally outside that preferred
    // window and is therefore not a suitable generic fixture for this file.
    config
        .optimizer
        .weights
        .transport_planform_constraints_enabled = false;
    // Single-pass closure isolates residual behavior; the clean-sheet
    // default instead sizes the takeoff mass by the mission.
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config
}

/// (1) The default aircraft, `MtowSizing::FixedRequirement`, evaluates to a
/// finite cost, a positive block fuel, a finite L/D, and a residual list
/// that names every family.
#[test]
fn the_default_design_evaluates_with_every_family_represented() {
    let config = block_fuel_config();
    let objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let assessment = assess_candidate(&objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(assessment.cost.is_finite());
    assert!(assessment.sized.block_fuel_kg > 0.0);
    assert!(assessment.sized.lift_to_drag.is_finite() && assessment.sized.lift_to_drag > 0.0);

    for family in [
        ConstraintFamily::Mass,
        ConstraintFamily::Balance,
        ConstraintFamily::Performance,
        ConstraintFamily::Geometry,
    ] {
        assert!(
            assessment
                .residuals
                .iter()
                .any(|residual| residual.family == family),
            "no residual recorded for {family:?}"
        );
    }
}

#[test]
fn strict_feasibility_and_cost_follow_the_residual_ledger_bit_for_bit() {
    let mut config = block_fuel_config();
    config.requirements.max_wing_area_m2 = 1.0;
    let mass_scale_kg = config.requirements.mtow_kg;
    let objective = DesignObjective::new(config);
    let assessment = assess_candidate(&objective, &DesignVector::default().to_array()).unwrap();
    let constraints: Vec<_> = assessment
        .residuals
        .iter()
        .filter(|row| row.role == ResidualRole::Constraint)
        .collect();
    let hard_sum: f64 = constraints.iter().map(|row| row.normalized_violation).sum();
    let preference_sum: f64 = assessment
        .residuals
        .iter()
        .filter(|row| row.role == ResidualRole::Preference)
        .map(|row| row.normalized_violation)
        .sum();
    assert!(!assessment.hard_feasible);
    assert_eq!(
        assessment.hard_feasible,
        constraints.iter().all(|row| !row.violated())
    );
    assert_eq!(assessment.hard_violation_sum.to_bits(), hard_sum.to_bits());
    // Block fuel uses 30 percent of declared MTOW as its scale; study
    // preferences retain their ten-to-one cost scale and infeasibility adds
    // one plus the aggregate normalized hard violation.
    let expected_cost = assessment.sized.block_fuel_kg / (0.3 * mass_scale_kg)
        + 10.0 * preference_sum
        + (1.0 + hard_sum);
    assert_eq!(assessment.cost.to_bits(), expected_cost.to_bits());
    assert!(assessment.violated_hard_ids().contains(&"wing_area"));
}

#[test]
fn saved_strict_configuration_preserves_nondefault_preference_cost_bits() {
    let mut config = block_fuel_config();
    config
        .optimizer
        .weights
        .transport_planform_constraints_enabled = true;
    config.optimizer.weights.transport_shape_priors_enabled = true;
    config.optimizer.weights.min_root_wingbox_depth_m = 100.0;
    config.optimizer.objective.preference_weight = 37.0;
    let mut document = serde_json::to_value(&config).unwrap();
    let objective = document["optimizer"]["objective"].as_object_mut().unwrap();
    objective.remove("preference_weight");
    objective.insert("soft_penalty_weight".to_owned(), serde_json::json!(37.0));
    for family in [
        "mass_constraints",
        "balance_constraints",
        "performance_constraints",
        "geometry_constraints",
    ] {
        objective.insert(family.to_owned(), serde_json::json!("hard"));
    }
    document["optimizer"]["relaxation"] = serde_json::json!({"enabled": false});
    let old_file = serde_json::to_string(&document).unwrap();
    let migrated = AlasConfig::from_value(&serde_json::from_str(&old_file).unwrap()).unwrap();
    assert_eq!(migrated.optimizer.objective.preference_weight, 37.0);
    let x = DesignVector::default().to_array();
    let current = assess_candidate(&DesignObjective::new(config), &x).unwrap();
    let restored = assess_candidate(&DesignObjective::new(migrated), &x).unwrap();
    assert!(current.soft_violation_sum > 0.0);
    assert_eq!(restored.cost.to_bits(), current.cost.to_bits());
    assert_eq!(restored.hard_feasible, current.hard_feasible);
    let expected = restored.objective_value / (0.3 * restored.sized.takeoff_mass_kg)
        + 37.0 * restored.soft_violation_sum
        + if restored.hard_feasible {
            0.0
        } else {
            1.0 + restored.hard_violation_sum
        };
    assert_eq!(restored.cost.to_bits(), expected.to_bits());
}

#[test]
fn direct_invalid_preference_weight_never_produces_a_feasible_candidate() {
    let mut config = block_fuel_config();
    config.optimizer.objective.preference_weight = f64::NAN;
    let assessment = assess_candidate(
        &DesignObjective::new(config),
        &DesignVector::default().to_array(),
    )
    .unwrap();
    assert!(!assessment.hard_feasible);
    assert!(assessment
        .violated_hard_ids()
        .contains(&"candidate_state_unavailable"));
}

#[test]
fn direct_invalid_plausibility_window_rejects_the_candidate() {
    let mut config = block_fuel_config();
    config.optimizer.plausibility.max_aspect_ratio = f64::NAN;
    let assessment = assess_candidate(
        &DesignObjective::new(config),
        &DesignVector::default().to_array(),
    )
    .unwrap();
    assert!(!assessment.hard_feasible);
    assert!(assessment
        .violated_hard_ids()
        .contains(&"plausibility_configuration_invalid"));
}

/// (2) `MtowSizing::SizedByMission` closes the outer fixed point and never
/// exceeds the takeoff-mass ceiling: `solve_dispatch` clamps every candidate
/// takeoff mass at `mtow_kg`, so this also exercises that the outer loop
/// reads the clamped value back correctly.
#[test]
fn sized_by_mission_closes_and_never_exceeds_the_ceiling() {
    let mut config = block_fuel_config();
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let ceiling_kg = config.requirements.mtow_kg;
    let objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let assessment = assess_candidate(&objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(
        assessment.sized.sizing_closed,
        "sizing did not close within the default iteration budget"
    );
    assert!(assessment.sized.takeoff_mass_kg <= ceiling_kg + 1e-6);
}

#[test]
fn registered_hard_mtow_nominals_separate_model_defects_from_physical_findings() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = assess_product_candidate(&config, &design).unwrap();
        assert_eq!(
            config.optimizer.objective.mtow_sizing,
            MtowSizing::FixedRequirement
        );
        assert_eq!(
            assessment.sized.takeoff_mass_kg,
            assessment.sized.zero_fuel_mass_kg
                + (config.requirements.mtow_kg - assessment.sized.zero_fuel_mass_kg)
                    .min(assessment.sized.usable_capacity_kg)
        );
        if matches!(name, "A320-200" | "DC-10") {
            let loading = assessment.sized.takeoff_loading.unwrap();
            assert_eq!(
                loading.status,
                alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited
            );
            assert!(loading.mtow_margin_kg > 0.0);
            assert!(!assessment.violated_hard_ids().contains(&"cg_model_error"));
        }
        let expected: &[&str] = match name {
            "A220-300" => &[],
            "A320-200" => &[],
            // With the trimmable stabiliser at its takeoff nose-up setting
            // the rotation boundary lies ahead of the item-level takeoff CG.
            // At the built -2 deg incidence it did not (boundary against
            // takeoff CG, %MAC: A340 30.8 against 26.7, B787 21.9 against
            // 16.9).
            "A340-300" | "B787-9" => &[],
            // Declared-mass loading misses the nose reaction. Its maximum-fuel
            // mission's mid-cruise lift clears Korn divergence (19.5 against
            // 26.9 counts). The takeoff trim clears the rotation boundary
            // that sat 0.8 %MAC aft of the flown takeoff CG (42.6 %MAC).
            "A380-800" => &["min_nose_gear_load"],
            // Pin update (round 3): was ["forward_cg_range",
            // "min_nose_gear_load", "tip_back"]. The 5 deg/s^2 class pitch
            // acceleration (Sadraey 12.3), the DATCOM K' elevator table with
            // the measured 21.8 deg up travel, and the tail-down-aware
            // tip-back requirement (8 deg model tail-down vs 10.7 deg
            // tip-back, no 15 deg floor) clear the rotation and tip-back
            // findings. The nose-gear load shortfall remains.
            "ATR72-600" => &["min_nose_gear_load"],
            // The maximum-fuel mission's mid-cruise lift still crosses the
            // Korn divergence boundary (27.5 against 26.9 counts). The
            // takeoff trim clears the rotation boundary.
            "AVE" => &["sweep_consistent_with_cruise_mach"],
            // The conventional Korn section factor leaves insufficient
            // cruise thrust and sweep. The empty aircraft's CG lies 3 mm
            // aft of the published minimum nose-gear share at its weight
            // (0.0598 against 0.0600 of its weight, inside the chart's
            // +-0.15 % read). The takeoff trim clears the rotation boundary
            // that lay aft of the volume-limited takeoff CG.
            "DC-10" => &[
                "min_nose_gear_load",
                "cruise_thrust",
                "sweep_consistent_with_cruise_mach",
            ],
            // No hard finding at the 62,500 kg MTOW nominal (EASA.IM.A.071
            // Issue 28): the registered E195-E2 clears every hard residual.
            "E195-E2" => &[],
            // No hard finding at the 75,100 kg MTOW nominal for the C919
            // (estimated planform, secondary-source weights): it clears every
            // hard residual once the inboard trailing edge is unswept.
            "C919" => &[],
            // Pin update (upper-deck hump): was ["min_nose_gear_load",
            // "cruise_thrust", "sweep_consistent_with_cruise_mach"]. The
            // partial upper deck now seats passengers over the forward
            // fuselage (it was an unseated full-length deck), the declared
            // ACAP doors bound the main cabin, and the furnishings carry the
            // upper floor's share, so the item-level takeoff CG moves forward
            // of the published minimum nose-gear share (6.7 % of weight at
            // the maximum taxi weight). The Korn and thrust findings remain:
            // the conventional section factor (0.87) leaves the 41 deg
            // leading-edge sweep short at Mach 0.85, and the model cruise
            // L/D needs more thrust than the four CF6-80C2B1F
            // estimate-flagged cruise reference (52.4 kN each).
            "B747-400" => &["cruise_thrust", "sweep_consistent_with_cruise_mach"],
            // New preset (no earlier pin): gear stations and the aft-fuselage
            // upsweep are estimates, so the nose reaction at the declared mass
            // (about -1 % of weight against 6 %) and the tip-back angle
            // (about 9.4 deg against the 15 deg floor of an unvalidated
            // tail-down) are estimate-driven findings, not A400M facts.
            "A400M" => &["min_nose_gear_load", "tip_back"],
            _ => panic!("{name}: establish the registered hard-MTOW findings"),
        };
        let mesh_failure = assessment
            .residuals
            .iter()
            .find(|row| row.id == "structural_mesh_invalid");
        assert!(
            mesh_failure.is_none(),
            "{name}: mesh construction is a model-validity prerequisite"
        );
        let mesh = assessment
            .residuals
            .iter()
            .find(|row| row.id == "structural_mesh_mass_discrepancy")
            .unwrap_or_else(|| {
                panic!("{name}: successful mesh construction must report its material mass")
            });
        assert!(mesh.actual.is_finite() && mesh.actual > 0.0);
        let mut actual: Vec<_> = assessment
            .violated_hard_ids()
            .into_iter()
            .filter(|id| *id != "structural_mesh_invalid")
            .collect();
        let mut expected = expected.to_vec();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "{name}: review changed physical findings");
        assert_eq!(
            assessment.hard_feasible,
            expected.is_empty() && mesh_failure.is_none(),
            "{name}"
        );
    }
}

/// An impossible mission exceeds the mass limit and increases the violation cost.
#[test]
fn an_impossible_design_range_is_hard_infeasible_and_costs_more() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"})).unwrap();
    let design = alas_config::presets::get("A220-300").unwrap().design_vector;
    let feasible = assess_product_candidate(&config, &design).unwrap();
    assert!(feasible.hard_feasible, "{:?}", feasible.violated_hard_ids());
    assert!(feasible.violated_hard_ids().is_empty());
    assert!(feasible
        .residuals
        .iter()
        .filter(|r| r.family == ConstraintFamily::Mass)
        .all(|r| !r.violated()));

    let mut impossible_config = config;
    // A trans-global mission exceeds the regional aircraft's mass and fuel
    // limits while remaining a finite mission for the dispatch model.
    impossible_config.optimizer.objective.design_range_nmi = 12_000.0;
    let impossible = assess_product_candidate(&impossible_config, &design).unwrap();

    assert!(!impossible.hard_feasible);
    assert!(impossible
        .residuals
        .iter()
        .any(|residual| residual.id == "mtow_ceiling" && residual.normalized_violation > 0.0));
    assert!(impossible.cost > feasible.cost);
}

/// (5) An infeasible candidate's reject reason lists the violated hard
/// residual ids joined by `+`, and the history vectors stay aligned.
#[test]
fn the_reject_reason_lists_violated_ids_joined_by_plus() {
    let mut config = block_fuel_config();
    config.optimizer.objective.design_range_nmi = 12_000.0;
    let mut objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let cost = objective.evaluate(&x);
    let history = &objective.history;
    let last = history.n_evaluations() - 1;

    assert!(!history.valid[last]);
    assert_eq!(history.cost[last], cost);
    let reason = &history.reject_reason[last];
    assert!(!reason.is_empty());
    assert!(reason.contains("mtow_ceiling"));
    for id in reason.split('+') {
        assert!(!id.is_empty(), "reason {reason:?} has an empty '+' segment");
    }

    // Every history vector, including the mission-sized additions, stays the
    // same length as the number of evaluations.
    assert_eq!(history.design_vectors.len(), history.n_evaluations());
    assert_eq!(history.objective_value.len(), history.n_evaluations());
    assert_eq!(history.takeoff_mass_kg.len(), history.n_evaluations());
    assert_eq!(history.block_fuel_kg.len(), history.n_evaluations());
    assert_eq!(history.hard_violation.len(), history.n_evaluations());
    assert_eq!(history.soft_violation.len(), history.n_evaluations());
}

/// A finite search budget need not find a design meeting every hard
/// requirement. It must return either a hard-feasible design or a typed
/// failure that identifies the violated residuals.
#[test]
fn a_trivial_de_run_returns_ok_or_names_the_failing_residuals() {
    let mut config = block_fuel_config();
    config.optimizer.solver.method = "differential_evolution".to_owned();
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.refinement.max_evaluations = 24;
    config.optimizer.solver.seed = Some(1);

    let result = DesignOptimizer::new(config).run(None, None, None);

    match result {
        Ok(outcome) => {
            assert!(outcome.best_valid);
            assert!(outcome.best_cost.is_finite());
        }
        Err(alas_opt::OptimizationError::NoFeasibleDesign(no_feasible)) => {
            assert!(no_feasible.evaluated_candidates > 0);
            assert!(!no_feasible.rejection_reason_counts.is_empty());
        }
        Err(other) => panic!("unexpected optimizer error: {other}"),
    }
}

/// (7) `MtowSizing::Unconstrained` uses the declared requirement only to seed
/// the first pass: a declared value the mission does not actually need
/// (`SizedByMission`'s own natural closure, well under the real ceiling)
/// still lets the closure re-converge back up past a seed deliberately
/// lowered below that natural point, is never rejected by the
/// `mtow_ceiling` residual (there is no ceiling under this mode), and is not
/// clamped by dispatch the way the same lowered value clamps
/// `SizedByMission`.
#[test]
fn unconstrained_reconverges_past_a_seed_lowered_below_the_natural_closure() {
    let natural_config = block_fuel_config();
    let mut sized_by_mission_config = natural_config.clone();
    sized_by_mission_config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let natural = assess_candidate(
        &DesignObjective::new(sized_by_mission_config),
        &DesignVector::default().to_array(),
    )
    .unwrap_or_else(|reason| panic!("{reason}"));
    // The default design's mission-sized closure must actually be below its
    // own declared ceiling for this test to say anything about the ceiling
    // being dropped rather than merely restated.
    assert_eq!(
        natural.sized.dispatch.status,
        alas_mass::dispatch::DispatchStatus::Converged
    );
    assert!(natural.sized.takeoff_mass_kg < natural_config.requirements.mtow_kg);
    let natural_mass_kg = natural.sized.takeoff_mass_kg;

    let mut lowered_config = natural_config;
    // Below the natural closure, so a ceiling-bound mode cannot reach it, but
    // not so far below it that the structural mass this design's own FLOPS
    // regression assigns at that lower declared MTOW (which does not shrink
    // proportionally with it) would itself already exceed the lowered
    // ceiling before any fuel is even added: that would report
    // `DispatchStatus::ModelFailed` (an input-validation rejection) instead
    // of the graceful `MtowLimited` clamp this test means to exercise. 0.8x
    // was chosen empirically against this exact fixture: comfortably below
    // the natural closure while still leaving the lowered ceiling above the
    // zero-fuel mass FLOPS assigns at that mass.
    lowered_config.requirements.mtow_kg = 0.8 * natural_mass_kg;

    let mut lowered_sized_by_mission = lowered_config.clone();
    lowered_sized_by_mission.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let clamped = assess_candidate(
        &DesignObjective::new(lowered_sized_by_mission),
        &DesignVector::default().to_array(),
    )
    .unwrap_or_else(|reason| panic!("{reason}"));
    assert!(
        matches!(
            clamped.sized.dispatch.status,
            alas_mass::dispatch::DispatchStatus::MtowLimited { .. }
        ),
        "expected the lowered ceiling to bind as MtowLimited, got {:?}",
        clamped.sized.dispatch.status
    );
    assert!(
        !clamped.hard_feasible,
        "a mission the lowered ceiling cannot fit must be hard-infeasible under SizedByMission"
    );
    assert!(
        clamped
            .residuals
            .iter()
            .any(|residual| residual.id == "mtow_ceiling" && residual.normalized_violation > 0.0),
        "SizedByMission must still check the mission-required mass against the lowered ceiling"
    );
    assert!(
        clamped.sized.takeoff_mass_kg <= lowered_config.requirements.mtow_kg + 1.0,
        "SizedByMission must not exceed the lowered ceiling: {} vs {}",
        clamped.sized.takeoff_mass_kg,
        lowered_config.requirements.mtow_kg
    );

    let mut lowered_unconstrained = lowered_config.clone();
    lowered_unconstrained.optimizer.objective.mtow_sizing = MtowSizing::Unconstrained;
    let unconstrained = assess_candidate(
        &DesignObjective::new(lowered_unconstrained),
        &DesignVector::default().to_array(),
    )
    .unwrap_or_else(|reason| panic!("{reason}"));
    assert!(
        !unconstrained
            .residuals
            .iter()
            .any(|residual| residual.id == "mtow_ceiling"),
        "Unconstrained must push no mtow_ceiling residual at all"
    );
    assert_eq!(
        unconstrained.sized.dispatch.status,
        alas_mass::dispatch::DispatchStatus::Converged,
        "the free closure from the lowered seed must still converge, not clamp"
    );
    assert!(
        unconstrained.sized.takeoff_mass_kg > lowered_config.requirements.mtow_kg,
        "the free-converged mass ({}) must climb back past the lowered seed ({}), proving the \
         seed was not re-applied as a ceiling",
        unconstrained.sized.takeoff_mass_kg,
        lowered_config.requirements.mtow_kg
    );
    // The fixed point the closure maps to depends on the physics at each
    // pass's own takeoff mass, not on where the iteration started (see
    // `mdo::mda`'s module doc comment), so the seed-independent natural
    // closure and the lowered-seed Unconstrained closure should land at
    // essentially the same mass.
    let relative_difference =
        (unconstrained.sized.takeoff_mass_kg - natural_mass_kg).abs() / natural_mass_kg;
    assert!(
        relative_difference < 0.01,
        "expected the Unconstrained closure ({}) to reconverge within 1% of the seed-independent \
         natural closure ({natural_mass_kg}), got {:.4}%",
        unconstrained.sized.takeoff_mass_kg,
        relative_difference * 100.0
    );
}

/// (8) A registered aircraft is a fixed aircraft. In `DesignMode::BaselineSandbox`
/// the mission-sized closure may move fuel and dispatch mass, never the
/// component ledger: two ranges and the single-pass declared-MTOW evaluation
/// must agree on every operating-empty group to numerical tolerance, and the
/// candidate must say which design weights it was sized on. The clean-sheet
/// mode is the coupled sizing loop and must still let the ledger follow the
/// closure.
#[test]
fn a_fixed_aircraft_keeps_its_component_ledger_under_mission_only_changes() {
    let preset = alas_config::presets::get("A220-300").unwrap();
    let base = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).unwrap();
    let run = |mode: DesignMode, sizing: MtowSizing, range_nmi: f64| {
        let mut config = base.clone();
        config.optimizer.design_space.mode = mode;
        config.optimizer.objective.mtow_sizing = sizing;
        config.optimizer.objective.design_range_nmi = range_nmi;
        assess_product_candidate(&config, &preset.design_vector)
            .unwrap_or_else(|reason| panic!("{mode:?} {sizing:?} {range_nmi} nmi: {reason}"))
    };
    let groups = |assessment: &alas_opt::CandidateAssessment| {
        let masses = &assessment.resolved.masses;
        [
            masses.wing,
            masses.h_stab,
            masses.v_stab,
            masses.fuselage,
            masses.gear,
            masses.propulsion,
            masses.systems,
            masses.furnishings,
        ]
    };

    let short = run(
        DesignMode::BaselineSandbox,
        MtowSizing::SizedByMission,
        600.0,
    );
    let long = run(
        DesignMode::BaselineSandbox,
        MtowSizing::SizedByMission,
        1_800.0,
    );
    let declared = run(
        DesignMode::BaselineSandbox,
        MtowSizing::FixedRequirement,
        600.0,
    );
    for assessment in [&short, &long] {
        assert_eq!(
            assessment.sized.dispatch.status,
            alas_mass::dispatch::DispatchStatus::Converged,
            "the fixed-aircraft mission must close for this check to mean anything"
        );
        assert!(assessment.sized.sizing_iterations > 1);
    }
    // The mission changed what it should: fuel and dispatch mass.
    assert!(long.sized.block_fuel_kg > short.sized.block_fuel_kg + 100.0);
    assert!(long.sized.takeoff_mass_kg > short.sized.takeoff_mass_kg + 100.0);
    assert!(short.sized.takeoff_mass_kg < base.requirements.mtow_kg);
    // And not what it must not: the component ledger.
    for (name, (a, b)) in [
        "wing",
        "h_stab",
        "v_stab",
        "fuselage",
        "gear",
        "propulsion",
        "systems",
        "furnishings",
    ]
    .into_iter()
    .zip(groups(&short).into_iter().zip(groups(&long)))
    {
        assert!(
            (a - b).abs() < 1.0e-6,
            "{name} moved with the mission on a fixed aircraft: {a} vs {b}"
        );
    }
    for (name, (a, b)) in [
        "wing",
        "h_stab",
        "v_stab",
        "fuselage",
        "gear",
        "propulsion",
        "systems",
        "furnishings",
    ]
    .into_iter()
    .zip(groups(&short).into_iter().zip(groups(&declared)))
    {
        assert!(
            (a - b).abs() < 1.0e-6,
            "{name} at the closed mass differs from the declared-MTOW ledger: {a} vs {b}"
        );
    }
    for assessment in [&short, &long, &declared] {
        assert_eq!(assessment.sized.sizing_basis, "fixed_aircraft");
        assert_eq!(
            assessment.sized.design_gross_mass_kg,
            base.requirements.mtow_kg
        );
        assert_eq!(
            assessment.sized.design_landing_mass_kg,
            preset.reference.mlw_kg.unwrap()
        );
    }

    // Clean sheet is the coupled sizing loop: the design gross mass follows
    // the closure, so the wing sized on the short mission is lighter than the
    // wing sized on the long one. The default clean-sheet fixture is used
    // because the registered A220 vector is not a converging clean-sheet
    // design (its re-solved fuselage fails the climb energy check).
    let coupled = |range_nmi: f64| {
        let mut config = block_fuel_config();
        config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
        config.optimizer.objective.design_range_nmi = range_nmi;
        assess_candidate(
            &DesignObjective::new(config),
            &DesignVector::default().to_array(),
        )
        .unwrap_or_else(|reason| panic!("{reason}"))
    };
    let coupled_short = coupled(2_000.0);
    let coupled_long = coupled(4_000.0);
    for assessment in [&coupled_short, &coupled_long] {
        assert_eq!(
            assessment.sized.dispatch.status,
            alas_mass::dispatch::DispatchStatus::Converged
        );
        assert_eq!(assessment.sized.sizing_basis, "coupled");
        assert!(
            (assessment.sized.design_gross_mass_kg - assessment.sized.takeoff_mass_kg).abs()
                < 1.0e-6
        );
    }
    assert!(
        coupled_short.resolved.masses.wing < coupled_long.resolved.masses.wing,
        "coupled sizing must let the wing follow the closed mass: {} vs {}",
        coupled_short.resolved.masses.wing,
        coupled_long.resolved.masses.wing
    );
}

/// The `landing_mass` residual's declared limit from an assessment, or a
/// panic naming what was missing: every mass-family assessment pushes this
/// residual (`mdo::residuals::mass_residuals`), so its absence is itself a
/// test failure, not a value to default away.
fn landing_mass_limit_kg(assessment: &alas_opt::CandidateAssessment) -> f64 {
    assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "landing_mass")
        .unwrap_or_else(|| {
            panic!(
                "no landing_mass residual in {:?}",
                assessment.sized.sizing_basis
            )
        })
        .limit
}

/// (9) `MtowSizing::Unconstrained` and `MtowSizing::SizedByMission` must
/// evaluate an identical aircraft for a fixed-aircraft basis
/// (`DesignMode::BaselineSandbox`) whenever the mission closes below the
/// declared MTOW: `Unconstrained` differs from `SizedByMission` only in
/// whether the declared value is a dispatch ceiling, not in what the
/// component ledger or the design weights are (see
/// `alas_config::MassSizingBasis` and `docs/mass-model-architecture.md`,
/// "The three questions and the sizing basis").
///
/// B787-9 is used because its mission-sized route closure converges under
/// both modes; it also
/// carries a declared MLW, so a second case on AVE (no declared MLW) is
/// added below to exercise the landing-limit fallback the declared-MLW
/// aircraft cannot.
#[test]
fn unconstrained_and_sized_by_mission_agree_on_a_fixed_aircraft_whose_mission_closes_below_mtow() {
    let preset = alas_config::presets::get("B787-9").unwrap();
    let mut base = AlasConfig::from_value(&serde_json::json!({ "preset": "B787-9" })).unwrap();
    base.optimizer.design_space.mode = DesignMode::BaselineSandbox;

    let run = |sizing: MtowSizing| {
        let mut config = base.clone();
        config.optimizer.objective.mtow_sizing = sizing;
        assess_product_candidate(&config, &preset.design_vector)
            .unwrap_or_else(|reason| panic!("{sizing:?}: {reason}"))
    };
    let unconstrained = run(MtowSizing::Unconstrained);
    let sized_by_mission = run(MtowSizing::SizedByMission);

    for assessment in [&unconstrained, &sized_by_mission] {
        assert_eq!(
            assessment.sized.dispatch.status,
            alas_mass::dispatch::DispatchStatus::Converged,
            "{:?}: the mission must close for this equivalence to mean anything",
            assessment.sized.sizing_basis
        );
        assert_eq!(assessment.sized.sizing_basis, "fixed_aircraft");
    }
    // The precondition the whole test is built on: the mission-required mass
    // sits below the declared MTOW, so `SizedByMission`'s dispatch ceiling
    // never binds and both modes are free to converge to the same closure.
    assert!(
        sized_by_mission.sized.takeoff_mass_kg < base.requirements.mtow_kg - 1.0,
        "the B787-9 route closure ({}) must close below its declared MTOW ({}) for this test",
        sized_by_mission.sized.takeoff_mass_kg,
        base.requirements.mtow_kg
    );

    const TOLERANCE_KG: f64 = 1.0e-6;
    assert!(
        (unconstrained.sized.takeoff_mass_kg - sized_by_mission.sized.takeoff_mass_kg).abs()
            < TOLERANCE_KG,
        "takeoff mass: {} vs {}",
        unconstrained.sized.takeoff_mass_kg,
        sized_by_mission.sized.takeoff_mass_kg
    );
    assert!(
        (unconstrained.sized.operating_empty_mass_kg
            - sized_by_mission.sized.operating_empty_mass_kg)
            .abs()
            < TOLERANCE_KG,
        "operating empty mass: {} vs {}",
        unconstrained.sized.operating_empty_mass_kg,
        sized_by_mission.sized.operating_empty_mass_kg
    );
    assert!(
        (unconstrained.sized.block_fuel_kg - sized_by_mission.sized.block_fuel_kg).abs()
            < TOLERANCE_KG,
        "block fuel: {} vs {}",
        unconstrained.sized.block_fuel_kg,
        sized_by_mission.sized.block_fuel_kg
    );
    assert!(
        (unconstrained.sized.zero_fuel_mass_kg - sized_by_mission.sized.zero_fuel_mass_kg).abs()
            < TOLERANCE_KG,
        "zero-fuel mass: {} vs {}",
        unconstrained.sized.zero_fuel_mass_kg,
        sized_by_mission.sized.zero_fuel_mass_kg
    );
    assert!(
        (unconstrained.sized.design_gross_mass_kg - sized_by_mission.sized.design_gross_mass_kg)
            .abs()
            < TOLERANCE_KG,
        "design gross mass: {} vs {}",
        unconstrained.sized.design_gross_mass_kg,
        sized_by_mission.sized.design_gross_mass_kg
    );
    assert!(
        (unconstrained.sized.design_landing_mass_kg
            - sized_by_mission.sized.design_landing_mass_kg)
            .abs()
            < TOLERANCE_KG,
        "design landing mass: {} vs {}",
        unconstrained.sized.design_landing_mass_kg,
        sized_by_mission.sized.design_landing_mass_kg
    );
    let unconstrained_mlw_limit = landing_mass_limit_kg(&unconstrained);
    let sized_by_mission_mlw_limit = landing_mass_limit_kg(&sized_by_mission);
    assert!(
        (unconstrained_mlw_limit - sized_by_mission_mlw_limit).abs() < TOLERANCE_KG,
        "landing_mass residual limit: {} vs {}",
        unconstrained_mlw_limit,
        sized_by_mission_mlw_limit
    );
    // The declared design landing mass is the residual's own limit, in both
    // modes: the bug this test guards against recomputed the limit from the
    // dispatch iterate under `Unconstrained` instead.
    assert!(
        (unconstrained_mlw_limit - unconstrained.sized.design_landing_mass_kg).abs() < TOLERANCE_KG
    );

    let masses = |assessment: &alas_opt::CandidateAssessment| assessment.resolved.masses;
    let a = masses(&unconstrained);
    let b = masses(&sized_by_mission);
    for (name, (x, y)) in [
        ("wing", (a.wing, b.wing)),
        ("h_stab", (a.h_stab, b.h_stab)),
        ("v_stab", (a.v_stab, b.v_stab)),
        ("fuselage", (a.fuselage, b.fuselage)),
        ("gear", (a.gear, b.gear)),
        ("propulsion", (a.propulsion, b.propulsion)),
        ("systems", (a.systems, b.systems)),
        ("furnishings", (a.furnishings, b.furnishings)),
        ("payload", (a.payload, b.payload)),
        ("fuel", (a.fuel, b.fuel)),
    ] {
        assert!(
            (x - y).abs() < TOLERANCE_KG,
            "resolved.masses.{name} differs between Unconstrained and SizedByMission: {x} vs {y}"
        );
    }

    // A coupled clean-sheet closure is not held to this equivalence: its
    // design gross and landing masses are defined to follow whatever the
    // dispatch mass converges to (`MassSizingBasis::Coupled`), so
    // `Unconstrained`'s seed-only requirement and `SizedByMission`'s bound
    // ceiling are free to close at different masses. This is only asserted
    // as "coupled", never as numerical equality with the fixed-aircraft
    // case above.
    let coupled = |sizing: MtowSizing| {
        let mut config = block_fuel_config();
        config.optimizer.objective.mtow_sizing = sizing;
        assess_candidate(
            &DesignObjective::new(config),
            &DesignVector::default().to_array(),
        )
        .unwrap_or_else(|reason| panic!("{sizing:?}: {reason}"))
    };
    let coupled_unconstrained = coupled(MtowSizing::Unconstrained);
    let coupled_sized_by_mission = coupled(MtowSizing::SizedByMission);
    assert_eq!(coupled_unconstrained.sized.sizing_basis, "coupled");
    assert_eq!(coupled_sized_by_mission.sized.sizing_basis, "coupled");
}

/// (10) On AVE, which declares no reference MLW, the landing-mass limit is
/// `mlw_fraction_mtow x declared MTOW` in every `MtowSizing` mode; it must
/// not follow the dispatch/mission-closed mass the way
/// `mdo::mda::converge`'s per-pass dispatch limit and the `landing_mass`
/// residual would (`landing_mass_limit_kg` 245,155 kg against the
/// declared-basis 329,976 kg).
#[test]
fn ave_landing_limit_is_the_declared_mtow_fraction_in_every_mtow_sizing_mode_not_the_dispatch_mass()
{
    let preset = alas_config::presets::get("AVE").unwrap();
    let mut base = AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" })).unwrap();
    base.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    assert!(
        preset.reference.mlw_kg.is_none(),
        "this case requires a preset with no declared MLW"
    );
    let expected_mlw_kg = base.requirements.mtow_kg * base.mass_model.mlw_fraction_mtow;

    for sizing in [
        MtowSizing::FixedRequirement,
        MtowSizing::SizedByMission,
        MtowSizing::Unconstrained,
    ] {
        let mut config = base.clone();
        config.optimizer.objective.mtow_sizing = sizing;
        let assessment = assess_product_candidate(&config, &preset.design_vector)
            .unwrap_or_else(|reason| panic!("{sizing:?}: {reason}"));
        assert_eq!(assessment.sized.sizing_basis, "fixed_aircraft");
        assert!(
            (assessment.sized.design_landing_mass_kg - expected_mlw_kg).abs() < 1.0e-6,
            "{sizing:?}: design_landing_mass_kg {} vs mlw_fraction_mtow x declared MTOW {}",
            assessment.sized.design_landing_mass_kg,
            expected_mlw_kg
        );
        let residual_limit_kg = landing_mass_limit_kg(&assessment);
        assert!(
            (residual_limit_kg - expected_mlw_kg).abs() < 1.0e-6,
            "{sizing:?}: landing_mass residual limit {} vs mlw_fraction_mtow x declared MTOW {}",
            residual_limit_kg,
            expected_mlw_kg
        );
        if sizing != MtowSizing::FixedRequirement {
            // The regression this guards against: the limit must not equal
            // what recomputing from the dispatch/mission-closed mass instead
            // of the declared design gross mass would give, whenever the
            // dispatch mass actually differs from the declared MTOW.
            let dispatch_mass_kg = assessment.sized.dispatch.takeoff_mass_kg;
            let bug_would_give_kg = dispatch_mass_kg * base.mass_model.mlw_fraction_mtow;
            if (dispatch_mass_kg - base.requirements.mtow_kg).abs() > 1.0 {
                assert!(
                    (residual_limit_kg - bug_would_give_kg).abs() > 1.0,
                    "{sizing:?}: landing limit ({residual_limit_kg}) coincides with the \
                     dispatch-mass recomputation ({bug_would_give_kg}); this case does not \
                     distinguish the fix from the bug"
                );
            }
        }
    }
}

/// The B787-9 route closure on its frozen plans is a property of the
/// aircraft, not of where the iteration starts or of the integration step
/// count it starts from: the converged takeoff mass agrees within 0.1 % from
/// the plan's seed (cold), from warm starts on either side of the closure,
/// and from two other starting step counts. Before the plan was frozen per
/// pass, the dispatch re-chose cruise levels and climb revisions at every
/// Picard mass, and the closure changed with the start and the step count.
///
/// The bound: the frozen step count meets a Richardson trip-fuel error of
/// 5e-4 (`frozen_plan::RICHARDSON_TOLERANCE`) and trip fuel is under half of
/// the takeoff mass, so discretization moves the closure by well under
/// 0.1 %; the outer loop's own tolerance is 1 kg.
#[test]
fn the_b787_closure_does_not_depend_on_its_start_or_step_count() {
    use alas_opt::mdo::SizingControls;
    let name = "B787-9";
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let design = alas_config::presets::get(name).unwrap().design_vector;
    let close = |controls: SizingControls| {
        alas_opt::mdo::assess_product_candidate_with_controls(&config, &design, controls)
            .unwrap_or_else(|reason| panic!("{controls:?}: {reason}"))
            .sized
    };
    let cold = close(SizingControls::default());
    assert!(cold.sizing_closed, "the cold closure must close");
    let cold_kg = cold.takeoff_mass_kg;
    for controls in [
        SizingControls {
            initial_takeoff_mass_kg: Some(0.9 * cold_kg),
            ..SizingControls::default()
        },
        SizingControls {
            initial_takeoff_mass_kg: Some(1.04 * cold_kg),
            ..SizingControls::default()
        },
        SizingControls {
            steps_per_segment: Some(2),
            ..SizingControls::default()
        },
        SizingControls {
            steps_per_segment: Some(8),
            ..SizingControls::default()
        },
    ] {
        let sized = close(controls);
        let relative = (sized.takeoff_mass_kg - cold_kg).abs() / cold_kg;
        assert!(
            sized.sizing_closed && relative <= 1.0e-3,
            "{controls:?}: {} kg against {cold_kg} kg ({relative:.2e})",
            sized.takeoff_mass_kg
        );
    }
}

/// A spent work budget rejects the candidate with its own reason, whichever
/// limit binds, rather than returning a partly closed aircraft.
#[test]
fn an_exhausted_sizing_budget_is_its_own_rejection_reason() {
    use alas_opt::mdo::mission_model::{SizingBudget, SIZING_BUDGET_EXHAUSTED};
    use alas_opt::mdo::SizingControls;
    let name = "A320-200";
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let design = alas_config::presets::get(name).unwrap().design_vector;
    let unlimited = SizingBudget {
        max_trip_flights: u32::MAX,
        max_deck_evals: u64::MAX,
        max_outer_passes: u32::MAX,
    };
    for budget in [
        SizingBudget {
            max_trip_flights: 3,
            ..unlimited
        },
        SizingBudget {
            max_deck_evals: 1_000,
            ..unlimited
        },
        SizingBudget {
            max_outer_passes: 1,
            ..unlimited
        },
    ] {
        let controls = SizingControls {
            budget: Some(budget),
            ..SizingControls::default()
        };
        let result =
            alas_opt::mdo::assess_product_candidate_with_controls(&config, &design, controls);
        assert_eq!(
            result.err().as_deref(),
            Some(SIZING_BUDGET_EXHAUSTED),
            "{budget:?}"
        );
    }
    // The same candidate inside an ample budget closes and reports its work.
    let sized = alas_opt::mdo::assess_product_candidate_with_controls(
        &config,
        &design,
        SizingControls {
            budget: Some(unlimited),
            ..SizingControls::default()
        },
    )
    .unwrap()
    .sized;
    assert!(sized.sizing_closed);
    assert!(sized.work.plan_freezes >= 1 && sized.work.trip_flights > 0);
    assert!(sized.work.deck_evals > 0);
}
