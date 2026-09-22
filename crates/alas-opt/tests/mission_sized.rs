// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the mission-sized objective (`alas_opt::mdo`).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, ConstraintPolicy, DesignMode, MtowSizing, ObjectiveKind};
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, assess_product_candidate, ConstraintFamily, DesignOptimizer};

fn block_fuel_config() -> AlasConfig {
    let mut config = AlasConfig::default();
    config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
    // These tests isolate mission sizing and residual-family behavior. The
    // transport body-attitude window is a separate clean-sheet design policy;
    // the canonical default vector is intentionally outside that preferred
    // window and is therefore not a suitable generic fixture for this file.
    config
        .optimizer
        .weights
        .transport_planform_constraints_enabled = false;
    // The canonical vector is deliberately not a trimmed aircraft fixture;
    // keep balance residuals visible while excluding them from the sizing
    // assertions below so this file tests mission closure rather than a
    // particular landing-gear/CG layout.
    config.optimizer.objective.balance_constraints = ConstraintPolicy::Diagnostic;
    // The single-pass closure these tests were written against; the
    // product default sizes the takeoff mass by the mission.
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

/// (3) A design range no transport can fly is `MtowLimited`, hard
/// infeasible, and costs more than the feasible default.
///
/// The fixture disables the transport planform policy and records balance
/// residuals as diagnostics; this keeps the test focused on mission range,
/// sizing and residual accounting while the dedicated parity tests cover
/// reference balance layouts.
#[test]
fn an_impossible_design_range_is_hard_infeasible_and_costs_more() {
    let feasible_objective = DesignObjective::new(block_fuel_config());
    let x = DesignVector::default().to_array();
    let feasible =
        assess_candidate(&feasible_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    assert!(feasible.hard_feasible);

    let mut impossible_config = block_fuel_config();
    impossible_config.optimizer.objective.design_range_nmi = 20_000.0;
    let impossible_objective = DesignObjective::new(impossible_config);
    let impossible =
        assess_candidate(&impossible_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(!impossible.hard_feasible);
    assert!(impossible
        .residuals
        .iter()
        .any(|residual| residual.id == "mtow_ceiling" && residual.normalized_violation > 0.0));
    assert!(impossible.cost > feasible.cost);
}

/// (4) Turning a family off removes its residuals; a `Diagnostic` family's
/// residuals are recorded but never counted toward feasibility.
#[test]
fn an_off_family_is_removed_and_a_diagnostic_family_is_uncounted() {
    let mut off_config = block_fuel_config();
    off_config.optimizer.objective.geometry_constraints = ConstraintPolicy::Off;
    let off_objective = DesignObjective::new(off_config);
    let x = DesignVector::default().to_array();
    let off_assessment =
        assess_candidate(&off_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    assert!(off_assessment
        .residuals
        .iter()
        .all(|residual| residual.family != ConstraintFamily::Geometry));

    let mut diagnostic_config = block_fuel_config();
    diagnostic_config.optimizer.objective.geometry_constraints = ConstraintPolicy::Diagnostic;
    let diagnostic_objective = DesignObjective::new(diagnostic_config);
    let diagnostic_assessment =
        assess_candidate(&diagnostic_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    let geometry_residuals: Vec<_> = diagnostic_assessment
        .residuals
        .iter()
        .filter(|residual| residual.family == ConstraintFamily::Geometry)
        .collect();
    assert!(!geometry_residuals.is_empty());
    assert_eq!(
        diagnostic_assessment.hard_violation_sum, off_assessment.hard_violation_sum,
        "a diagnostic family must not add to the hard-violation sum"
    );
    assert_eq!(
        diagnostic_assessment.soft_violation_sum, off_assessment.soft_violation_sum,
        "a diagnostic family must not add to the soft-violation sum"
    );
}

/// (5) An infeasible candidate's reject reason lists the violated hard
/// residual ids joined by `+`, and the history vectors stay aligned.
#[test]
fn the_reject_reason_lists_violated_ids_joined_by_plus() {
    let mut config = block_fuel_config();
    config.optimizer.objective.design_range_nmi = 20_000.0;
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

/// (6) A trivial differential-evolution run (population seeded from the
/// default design plus a Latin-hypercube fill, with the default `Hard`
/// geometry policy) returns `Ok`, or reports `NoFeasibleDesign` naming the
/// residuals that failed; both are legitimate outcomes of this contract, so
/// the assertion accepts either.
///
/// The exact default design vector is itself marginally hard-infeasible
/// under the default `Hard` geometry policy (its built wing area sits a few
/// parts per million over `max_wing_area_m2`, and both tail-volume
/// coefficients sit a few percent under their configured minimum window: the
/// legacy weighted-penalty objective these defaults were tuned against
/// treats both as soft preferences, not hard bounds), so whether this
/// particular seed's population contains a candidate that clears every hard
/// residual is itself part of what the test exercises.
#[test]
fn a_trivial_de_run_returns_ok_or_names_the_failing_residuals() {
    let mut config = block_fuel_config();
    config.optimizer.solver.method = "differential_evolution".to_owned();
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.max_iterations = 0;
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
/// both modes on the baseline tree
/// (`outputs/mass-model-consolidation/after/mission-cases.csv`,
/// `B_baseline_sandbox_{sized_by_mission,unconstrained}_route`); it also
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
/// residual used to before this fix (`outputs/mass-model-consolidation/after/mission-cases.csv`,
/// `AVE,B_baseline_sandbox_unconstrained_route`, `landing_mass_limit_kg`
/// 245,155 against the declared-basis 329,976).
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
