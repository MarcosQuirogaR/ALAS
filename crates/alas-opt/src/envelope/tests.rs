// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on inputs they constructed here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#![allow(clippy::expect_used)]

use super::{
    assess_loading_constraints, assess_model_cg_envelope, check_cg_envelope, ledger_basis,
    loading_states, AftCgLimitGovernance, AftLimitGovernance, ForwardLimitGovernance,
    LoadingConstraintInputs, ModelCgConstraint, ModelCgEnvelopeError, ModelCgLoadingState,
    PhaseLimits, PhysicalCgLimits, StationError,
};
use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_mass_analysis, MassBreakdown, MassCoordinates};
use ledger_basis::registered_mrw_kg;

/// Comfortably admissible physical boundaries: `cg_pct_mac` (25.0 in
/// [`passing_inputs`]) sits well inside `[fwd_limit_pct_mac,
/// aft_limit_pct_mac]` and the usable range clears the configured floor.
fn passing_physical_limits() -> PhysicalCgLimits {
    PhysicalCgLimits {
        aft_limit_pct_mac: 90.0,
        aft_limit_governance: AftLimitGovernance::Aerodynamic,
        clean_np_pct_mac: 95.0,
        aerodynamic_aft_pct_mac: 90.0,
        ground_aft_pct_mac: 92.0,
        tip_back_aft_pct_mac: 93.0,
        fwd_limit_pct_mac: 10.0,
        fwd_limit_governance: ForwardLimitGovernance::MaxNoseLoadHandling,
        max_nose_load_fwd_pct_mac: 10.0,
        scissor_plot_fwd_pct_mac: 8.0,
        // `fwd_limit_pct_mac` is still governed by `MaxNoseLoadHandling`.
        rotation_fwd_pct_mac: 9.0,
        takeoff_trim_fwd_pct_mac: f64::NEG_INFINITY,
        clean_trim_fwd_pct_mac: f64::NEG_INFINITY,
        landing_approach_fwd_pct_mac: f64::NEG_INFINITY,
        landing_flare_fwd_pct_mac: f64::NEG_INFINITY,
        rotation_longitudinal_force_shift_pct_mac: 0.0,
        rotation_tail_lift_coefficient: -1.3,
        rotation_stabilizer_nose_up_deg: None,
        landing_trim_fwd_pct_mac: 9.5,
        iced_tail_fwd_pct_mac: 9.0,
        landing_tail_lift_coefficient: -0.8,
        landing_tail_trim_limited: false,
        landing_tail_sourced: false,
        landing_trim_condition: "design rule",
        usable_range_pct_mac: 80.0,
    }
}

fn passing_inputs() -> LoadingConstraintInputs {
    LoadingConstraintInputs {
        state: ModelCgLoadingState::AnalyzedTakeoff,
        scope: PhaseLimits::TAKEOFF,
        mass_kg: 100_000.0,
        cg_x_m: 10.0,
        cg_pct_mac: 25.0,
        static_margin: 0.08,
        critical_static_margin: 0.08,
        static_margin_floor: 0.05,
        physical_limits: passing_physical_limits(),
        nose_gear_load_kg: 8_000.0,
        nose_gear_capacity_kg: 10_000.0,
        main_gear_load_kg: 92_000.0,
        main_gear_capacity_kg: 95_000.0,
        capacity_basis_declared: true,
        minimum_nose_gear_load_fraction: 0.02,
        maximum_nose_gear_load_fraction: 0.20,
        h_cg_m: 3.0,
        tip_back_angle_deg: 20.0,
        required_tip_back_deg: 15.0,
        scrape_angle_deg: None,
        required_rotation_angle_deg: 12.0,
        tail_scrape_rejects: true,
        cg_range_pct_mac: 30.0,
    }
}

fn assert_only_constraint_violates(inputs: LoadingConstraintInputs, expected: ModelCgConstraint) {
    let assessment = assess_loading_constraints(inputs);
    let violations = assessment
        .constraints
        .iter()
        .filter(|constraint| constraint.violated)
        .map(|constraint| constraint.constraint)
        .collect::<Vec<_>>();
    assert_eq!(violations, vec![expected]);
}

/// A loaded state resting on both legs keeps all five constraints and is
/// declared admissible; nothing about the ordinary case moved.
#[test]
fn a_state_resting_on_both_gear_groups_keeps_every_constraint() {
    let assessment = assess_loading_constraints(passing_inputs());
    assert!(assessment.ground_reactions_admissible);
    // StaticStabilityFloor, PhysicalForwardCgLimit, MinimumNoseGearLoad,
    // MaximumNoseGearLoadFraction, TipBack, MinimumUsableCgRange,
    // NoseGearStrength, MainGearStrength (no TailScrape: `passing_inputs`
    // carries no scrape angle).
    assert_eq!(assessment.constraints.len(), 8);
    assert!(assessment
        .constraints
        .iter()
        .any(|constraint| constraint.constraint == ModelCgConstraint::NoseGearStrength));
    assert!(assessment
        .constraints
        .iter()
        .any(|constraint| constraint.constraint == ModelCgConstraint::MainGearStrength));
    assert!(!assessment.constraints.iter().any(|c| c.violated));
}

/// The tail-sitting state the ATR 72-600 and the A380-800 both produced:
/// the centre of gravity aft of the effective main-gear station, so the
/// nose reaction is negative and the main reaction exceeds the whole
/// weight. Neither is a load the gear sees, so neither rated-capacity
/// comparison is reported; `MinimumNoseGearLoad` still rejects the state.
///
/// Before this, `-5 618 kg < 68 000 kg` printed as a *passing* nose-gear
/// strength margin on the A380-800 and `435 877 kg` printed as a passing
/// main-gear load on a 430 259 kg aeroplane.
#[test]
fn a_tail_sitting_state_reports_no_gear_strength_margin() {
    let mut inputs = passing_inputs();
    inputs.nose_gear_load_kg = -1_310.0;
    inputs.main_gear_load_kg = inputs.mass_kg - inputs.nose_gear_load_kg;
    let assessment = assess_loading_constraints(inputs);

    assert!(!assessment.ground_reactions_admissible);
    assert!((assessment.nose_gear_load_fraction - -0.0131).abs() < 1.0e-12);
    let present: Vec<ModelCgConstraint> = assessment
        .constraints
        .iter()
        .map(|constraint| constraint.constraint)
        .collect();
    assert!(!present.contains(&ModelCgConstraint::NoseGearStrength));
    assert!(!present.contains(&ModelCgConstraint::MainGearStrength));
    assert!(present.contains(&ModelCgConstraint::MinimumNoseGearLoad));
    assert_only_constraint_violates(inputs, ModelCgConstraint::MinimumNoseGearLoad);
}

/// Zero nose load is the tipping point itself, not past it: the split is
/// still a statement the model can make, so nothing is dropped. This pins
/// that the domain test carries no margin in either direction.
#[test]
fn the_admissibility_boundary_is_exact_and_carries_no_margin() {
    let mut inputs = passing_inputs();
    inputs.nose_gear_load_kg = 0.0;
    inputs.main_gear_load_kg = inputs.mass_kg;
    let at_boundary = assess_loading_constraints(inputs);
    assert!(at_boundary.ground_reactions_admissible);
    assert_eq!(at_boundary.constraints.len(), 8);

    inputs.nose_gear_load_kg = -f64::MIN_POSITIVE;
    let past_boundary = assess_loading_constraints(inputs);
    assert!(!past_boundary.ground_reactions_admissible);
    assert_eq!(past_boundary.constraints.len(), 6);
}

#[test]
fn loading_states_use_the_payload_mass_from_the_second_pass() {
    let states = loading_states(100.0, 10.0, 50.0, 20.0, 30.0, 200.0);

    assert_eq!(states[0], (10.0, 100.0));
    assert_eq!(states[1], (13.333333333333334, 150.0));
    assert_eq!(states[2], (30.0, 350.0));
}

#[test]
fn static_stability_floor_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.critical_static_margin = 0.049;
    assert_only_constraint_violates(inputs, ModelCgConstraint::StaticStabilityFloor);
}

#[test]
fn configured_forward_range_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.cg_pct_mac = 9.9;
    assert_only_constraint_violates(inputs, ModelCgConstraint::PhysicalForwardCgLimit);
}

#[test]
fn nose_gear_strength_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.nose_gear_load_kg = 10_001.0;
    assert_only_constraint_violates(inputs, ModelCgConstraint::NoseGearStrength);
}

#[test]
fn main_gear_strength_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.main_gear_load_kg = 95_001.0;
    assert_only_constraint_violates(inputs, ModelCgConstraint::MainGearStrength);
}

#[test]
fn minimum_nose_load_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.nose_gear_load_kg = 1_999.0;
    assert_only_constraint_violates(inputs, ModelCgConstraint::MinimumNoseGearLoad);
}

#[test]
fn maximum_nose_load_fraction_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.nose_gear_capacity_kg = 50_000.0;
    inputs.nose_gear_load_kg = 20_001.0;
    inputs.main_gear_load_kg = inputs.mass_kg - inputs.nose_gear_load_kg;
    assert_only_constraint_violates(inputs, ModelCgConstraint::MaximumNoseGearLoadFraction);
}

#[test]
fn tip_back_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.tip_back_angle_deg = 14.9;
    assert_only_constraint_violates(inputs, ModelCgConstraint::TipBack);
}

#[test]
fn tail_scrape_is_omitted_with_no_scrape_angle_and_present_with_one() {
    let mut inputs = passing_inputs();
    assert!(!assess_loading_constraints(inputs)
        .constraints
        .iter()
        .any(|constraint| constraint.constraint == ModelCgConstraint::TailScrape));
    inputs.scrape_angle_deg = Some(11.9);
    assert_only_constraint_violates(inputs, ModelCgConstraint::TailScrape);
}

#[test]
fn minimum_usable_cg_range_governs_independently() {
    let mut inputs = passing_inputs();
    inputs.physical_limits.usable_range_pct_mac = 29.9;
    assert_only_constraint_violates(inputs, ModelCgConstraint::MinimumUsableCgRange);
}

#[test]
fn preferred_static_margin_does_not_replace_the_physical_floor() {
    let mut config = AlasConfig::default();
    config.requirements.min_physical_static_margin = 0.05;
    config.requirements.target_static_margin = 0.20;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), false)
        .expect("default geometry");
    let (masses, coordinates, cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let x_np = cg[0] + 0.10 * plane.c_ref;
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg[0],
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .expect("model assessment");
    let mtow_static_constraint = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
        .expect("analyzed-TOW state")
        .constraints
        .iter()
        .find(|constraint| constraint.constraint == ModelCgConstraint::StaticStabilityFloor)
        .expect("analyzed-TOW static constraint");

    assert!(!mtow_static_constraint.violated);
    // The floor is the configured one or Torenbeek's 0.02 tail arms, whichever
    // is larger, never the preferred margin.
    assert_eq!(
        mtow_static_constraint.limit,
        assessment.minimum_physical_static_margin
    );
    assert!((0.05..0.20).contains(&mtow_static_constraint.limit));
    assert!(!assessment.target_static_margin.met_or_exceeded());
    assert_eq!(assessment.target_static_margin.target, 0.20);
}

/// The phase of each named state, as a matrix.
#[test]
fn each_state_maps_to_its_phase_mechanisms() {
    let cases = [
        (ModelCgLoadingState::OperatingEmpty, PhaseLimits::GROUND),
        (ModelCgLoadingState::AnalyzedZeroFuel, PhaseLimits::FLIGHT),
        (
            ModelCgLoadingState::OperationalMidMission,
            PhaseLimits::FLIGHT,
        ),
        (ModelCgLoadingState::OperationalReserve, PhaseLimits::FLIGHT),
        (ModelCgLoadingState::AnalyzedTakeoff, PhaseLimits::TAKEOFF),
        (ModelCgLoadingState::AnalyzedLanding, PhaseLimits::LANDING),
    ];
    for (state, expected) in cases {
        assert_eq!(PhaseLimits::for_state(state), expected, "{state:?}");
    }
    // Rotation is scoped to takeoff alone; landing trim to flight and
    // landing; the static-margin floor to flight, takeoff and landing.
    let phase = |rotation, landing_trim, static_margin| PhaseLimits {
        rotation,
        landing_trim,
        static_margin,
        clean_trim: static_margin,
    };
    assert_eq!(PhaseLimits::GROUND, phase(false, false, false));
    assert_eq!(PhaseLimits::TAKEOFF, phase(true, false, true));
    assert_eq!(PhaseLimits::FLIGHT, phase(false, true, true));
    assert_eq!(PhaseLimits::LANDING, phase(false, true, true));
    assert_eq!(PhaseLimits::EN_ROUTE_ABOVE_MLW, phase(false, false, true));
}

/// The mass scoping against the design MLW. Rotation is a ground take-off
/// event: it bounds the take-off state alone, at any mass. An en-route state
/// heavier than the MLW cannot land, so it loses landing trim and gains no
/// rotation boundary; at or below the MLW it keeps landing trim. The OEW and
/// landing states keep their own phase at any mass.
#[test]
fn mass_scoping_drops_landing_trim_above_the_mlw_and_never_adds_rotation() {
    let mlw_kg = 60_000.0;
    let all_states = [
        ModelCgLoadingState::OperatingEmpty,
        ModelCgLoadingState::AnalyzedZeroFuel,
        ModelCgLoadingState::OperationalMidMission,
        ModelCgLoadingState::OperationalReserve,
        ModelCgLoadingState::AnalyzedTakeoff,
        ModelCgLoadingState::AnalyzedLanding,
    ];
    for mass_kg in [30_000.0, mlw_kg, 90_000.0] {
        for state in all_states {
            let scope = PhaseLimits::for_state_at_mass(state, mass_kg, mlw_kg);
            assert_eq!(
                scope.rotation,
                state == ModelCgLoadingState::AnalyzedTakeoff,
                "{state:?} at {mass_kg} kg"
            );
            assert_eq!(
                scope.static_margin,
                PhaseLimits::for_state(state).static_margin,
                "{state:?} at {mass_kg} kg"
            );
        }
    }
    for state in [
        ModelCgLoadingState::AnalyzedZeroFuel,
        ModelCgLoadingState::OperationalMidMission,
        ModelCgLoadingState::OperationalReserve,
    ] {
        for at_or_below in [30_000.0, mlw_kg] {
            assert_eq!(
                PhaseLimits::for_state_at_mass(state, at_or_below, mlw_kg),
                PhaseLimits::FLIGHT,
                "{state:?}"
            );
        }
        assert_eq!(
            PhaseLimits::for_state_at_mass(state, mlw_kg + 1.0, mlw_kg),
            PhaseLimits::EN_ROUTE_ABOVE_MLW,
            "{state:?}"
        );
    }
    assert_eq!(
        PhaseLimits::for_state_at_mass(ModelCgLoadingState::AnalyzedLanding, 90_000.0, mlw_kg),
        PhaseLimits::LANDING
    );
    assert_eq!(
        PhaseLimits::for_state_at_mass(ModelCgLoadingState::AnalyzedTakeoff, 30_000.0, mlw_kg),
        PhaseLimits::TAKEOFF
    );
    assert_eq!(
        PhaseLimits::for_state_at_mass(ModelCgLoadingState::OperatingEmpty, 90_000.0, mlw_kg),
        PhaseLimits::GROUND
    );

    // Distinct forward candidates: rotation 14, landing trim 12, maximum nose
    // load 10 %MAC. Each scope picks its own, and above the MLW only the
    // ground maximum nose load is left (not the rotation boundary).
    let limits = PhysicalCgLimits {
        rotation_fwd_pct_mac: 14.0,
        landing_trim_fwd_pct_mac: 12.0,
        ..passing_physical_limits()
    };
    assert_eq!(
        limits.fwd_for(PhaseLimits::TAKEOFF),
        (14.0, ForwardLimitGovernance::RotationNoseWheelLiftoff)
    );
    assert_eq!(
        limits.fwd_for(PhaseLimits::FLIGHT),
        (12.0, ForwardLimitGovernance::LandingTrimDesignRule)
    );
    assert_eq!(
        limits.fwd_for(PhaseLimits::EN_ROUTE_ABOVE_MLW),
        (10.0, ForwardLimitGovernance::MaxNoseLoadHandling)
    );
    // The aft side is the in-flight one: the static-margin floor still bounds.
    assert_eq!(
        limits.aft_for(PhaseLimits::EN_ROUTE_ABOVE_MLW),
        limits.aft_for(PhaseLimits::FLIGHT)
    );
}

#[test]
fn a_ground_only_phase_keeps_gear_checks_and_excludes_flight_cg_checks() {
    for constraint in [
        ModelCgConstraint::StaticStabilityFloor,
        ModelCgConstraint::PhysicalForwardCgLimit,
        ModelCgConstraint::MinimumUsableCgRange,
    ] {
        assert!(!PhaseLimits::GROUND.admits(constraint), "{constraint:?}");
        assert!(PhaseLimits::FLIGHT.admits(constraint), "{constraint:?}");
        assert!(PhaseLimits::TAKEOFF.admits(constraint), "{constraint:?}");
    }
    assert!(PhaseLimits::LANDING.admits(ModelCgConstraint::StaticStabilityFloor));
    assert!(PhaseLimits::LANDING.admits(ModelCgConstraint::PhysicalForwardCgLimit));
    for constraint in [
        ModelCgConstraint::NoseGearStrength,
        ModelCgConstraint::MainGearStrength,
        ModelCgConstraint::MinimumNoseGearLoad,
        ModelCgConstraint::MaximumNoseGearLoadFraction,
        ModelCgConstraint::TipBack,
        ModelCgConstraint::TailScrape,
    ] {
        for phase in [
            PhaseLimits::GROUND,
            PhaseLimits::FLIGHT,
            PhaseLimits::TAKEOFF,
            PhaseLimits::LANDING,
        ] {
            assert!(phase.admits(constraint), "{phase:?} {constraint:?}");
        }
    }
}

/// Per-state matrix on a real assessment with a landing state: every state
/// carries its own limits, scoped to its phase at its mass against the MLW
/// (the declared one where there is one), and its forward-limit
/// constraint is gated against exactly that state's own forward limit. Only
/// the take-off state is bounded by rotation; an en-route state above the MLW
/// is bounded by the ground maximum nose load, not by a rotation boundary at
/// its own mass.
#[test]
fn every_state_is_gated_against_its_own_phase_scoped_limits() {
    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), false)
        .expect("default geometry");
    let (masses, coordinates, cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let (oew_mass, oew_cg_x) = alas_payload::oew::oew_and_cg(&masses, &coordinates);
    let oew_cg_z = super::support::oew_cg_z(&masses, &coordinates);
    let states = super::operational_loading_states_with_z(
        oew_mass,
        oew_cg_x,
        oew_cg_z,
        masses.payload,
        coordinates.payload[0],
        coordinates.payload[2],
        masses.fuel,
        coordinates.fuel[0],
        coordinates.fuel[2],
        cg[0],
    );
    let point = |wanted: ModelCgLoadingState| {
        states
            .iter()
            .copied()
            .find(|(state, _, _, _)| *state == wanted)
            .expect("named state")
    };
    let (_, oew_x, oew_z, oew_m) = point(ModelCgLoadingState::OperatingEmpty);
    let (_, zfw_x, zfw_z, zfw_m) = point(ModelCgLoadingState::AnalyzedZeroFuel);
    let (_, tow_x, tow_z, tow_m) = point(ModelCgLoadingState::AnalyzedTakeoff);
    let (_, ldg_x, ldg_z, ldg_m) = point(ModelCgLoadingState::OperationalReserve);
    let basis = super::LedgerLoadingBasis {
        oew_mass_kg: oew_m,
        oew_cg_x_m: oew_x,
        oew_cg_z_m: oew_z,
        zero_fuel_mass_kg: zfw_m,
        zero_fuel_cg_x_m: zfw_x,
        zero_fuel_cg_z_m: zfw_z,
        takeoff_mass_kg: tow_m,
        takeoff_cg_x_m: tow_x,
        takeoff_cg_z_m: tow_z,
        takeoff_pitch_inertia_kg_m2: f64::NAN,
        oew_pitch_inertia_kg_m2: f64::NAN,
        zero_fuel_pitch_inertia_kg_m2: f64::NAN,
    };
    let x_np = cg[0] + 0.10 * plane.c_ref;
    let landing = super::LedgerLandingState {
        mass_kg: ldg_m,
        cg_x_m: ldg_x,
        cg_z_m: ldg_z,
    };
    let assessment = super::assess_model_cg_envelope_with_ledger_and_landing(
        &plane,
        basis,
        Some(landing),
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .expect("assessment with a landing state");
    let without_landing = super::assess_model_cg_envelope_with_ledger(
        &plane,
        basis,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .expect("assessment without a landing state");

    let names: Vec<_> = assessment.loading_states.iter().map(|s| s.state).collect();
    assert_eq!(names.last(), Some(&ModelCgLoadingState::AnalyzedLanding));
    assert_eq!(names.len(), without_landing.loading_states.len() + 1);
    // The landing state changes neither the envelope-wide values (forward
    // limit, gear sizing) nor any other state's verdicts.
    assert_eq!(
        assessment.configured_forward_limit_pct_mac,
        without_landing.configured_forward_limit_pct_mac
    );
    assert_eq!(
        assessment.gear_design_main_reaction_kg,
        without_landing.gear_design_main_reaction_kg
    );
    assert_eq!(
        &assessment.loading_states[..names.len() - 1],
        &without_landing.loading_states[..]
    );

    // One MLW: the configuration's own mode-aware landing-mass limit at its
    // candidate MTOW, the one the mass model uses.
    let design_landing_mass_kg = ledger_basis::maximum_landing_mass_kg(&config, tow_m);
    assert_eq!(
        design_landing_mass_kg,
        config.landing_mass_limit_kg(config.requirements.mtow_kg)
    );
    assert_eq!(assessment.design_landing_mass_kg, design_landing_mass_kg);
    assert_eq!(
        without_landing.design_landing_mass_kg,
        design_landing_mass_kg
    );
    for state in &assessment.loading_states {
        // The constraint set is the state's phase; the limits are that phase
        // at the state's mass, and the state reports that scope.
        let phase = PhaseLimits::for_state(state.state);
        let scope =
            PhaseLimits::for_state_at_mass(state.state, state.mass_kg, design_landing_mass_kg);
        assert_eq!(state.scope, scope, "{:?}", state.state);
        assert_eq!(
            scope.rotation,
            state.state == ModelCgLoadingState::AnalyzedTakeoff,
            "{:?}: only the take-off state is bounded by rotation",
            state.state
        );
        let limits = state.physical_limits;
        assert_eq!(
            limits,
            limits.scoped(scope),
            "{:?} limits are not scoped",
            state.state
        );
        let (fwd, _) = limits.fwd_for(scope);
        assert_eq!(limits.fwd_limit_pct_mac, fwd);
        // The envelope-wide limit is at least as aft as every scoped one.
        assert!(limits.fwd_limit_pct_mac <= assessment.configured_forward_limit_pct_mac + 1.0e-9);
        let forward = state
            .constraints
            .iter()
            .find(|c| c.constraint == ModelCgConstraint::PhysicalForwardCgLimit);
        match forward {
            Some(constraint) => {
                assert!(phase.rotation || phase.landing_trim);
                assert_eq!(constraint.limit, limits.fwd_limit_pct_mac);
            }
            None => assert_eq!(phase, PhaseLimits::GROUND),
        }
        let has_floor = state
            .constraints
            .iter()
            .any(|c| c.constraint == ModelCgConstraint::StaticStabilityFloor);
        assert_eq!(has_floor, phase.static_margin, "{:?}", state.state);
        for constraint in &state.constraints {
            assert!(phase.admits(constraint.constraint));
        }
        match state.state {
            ModelCgLoadingState::AnalyzedTakeoff => assert!(
                limits.fwd_limit_pct_mac >= limits.rotation_fwd_pct_mac - 1.0e-9
                    && limits.fwd_limit_pct_mac >= limits.max_nose_load_fwd_pct_mac - 1.0e-9
            ),
            ModelCgLoadingState::OperatingEmpty => {
                assert_eq!(limits.fwd_limit_pct_mac, limits.max_nose_load_fwd_pct_mac);
            }
            _ if scope == PhaseLimits::EN_ROUTE_ABOVE_MLW => {
                // Cannot land: no landing trim, and no rotation boundary at
                // its own mass either.
                assert!(state.mass_kg > design_landing_mass_kg);
                assert_eq!(limits.fwd_limit_pct_mac, limits.max_nose_load_fwd_pct_mac);
                assert_eq!(
                    limits.fwd_limit_governance,
                    ForwardLimitGovernance::MaxNoseLoadHandling
                );
            }
            _ => assert!(limits.fwd_limit_pct_mac >= limits.landing_trim_fwd_pct_mac - 1.0e-9),
        }
    }

    // A declared MLW governs the gate, not the fraction: certified at the
    // zero-fuel mass, every heavier en-route state cannot land and is bounded
    // forward by the maximum nose load alone; the zero-fuel state at the MLW
    // may still land.
    let mut certified = config.clone();
    certified.requirements.mlw_kg = Some(zfw_m);
    let gated = super::assess_model_cg_envelope_with_ledger(
        &plane,
        basis,
        x_np,
        x_np,
        plane.c_ref,
        &certified,
    )
    .expect("assessment at a declared MLW");
    assert_eq!(gated.design_landing_mass_kg, zfw_m);
    let mut above = 0;
    for state in &gated.loading_states {
        let en_route = matches!(
            state.state,
            ModelCgLoadingState::OperationalMidMission | ModelCgLoadingState::OperationalReserve
        );
        if en_route && state.mass_kg > zfw_m * (1.0 + 1e-9) {
            above += 1;
            assert_eq!(
                state.scope,
                PhaseLimits::EN_ROUTE_ABOVE_MLW,
                "{:?}",
                state.state
            );
            let limits = state.physical_limits;
            assert_eq!(limits.fwd_limit_pct_mac, limits.max_nose_load_fwd_pct_mac);
        }
        if state.state == ModelCgLoadingState::AnalyzedZeroFuel {
            assert_eq!(state.scope, PhaseLimits::FLIGHT);
        }
    }
    assert!(above >= 1, "no en-route state above the declared MLW");
}

/// Bare OEW must not fail `PhysicalForwardCgLimit`, a flight constraint,
/// because OEW is a ground-only reference condition.
/// This asserts the exact constraint set kept per state, so
/// reintroducing the flight constraints on OEW (or dropping ground
/// checks from it) both fail immediately regardless of the numeric
/// CG/mass values in play.
#[test]
fn bare_oew_keeps_ground_checks_but_not_flight_cg_constraints() {
    let mut config = AlasConfig::default();
    config.requirements.min_physical_static_margin = 0.05;
    config.requirements.target_static_margin = 0.20;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), false)
        .expect("default geometry");
    let (masses, coordinates, cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let x_np = cg[0] + 0.10 * plane.c_ref;
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg[0],
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .expect("model assessment");

    let oew = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::OperatingEmpty)
        .expect("OEW state");
    let oew_constraint_kinds = oew
        .constraints
        .iter()
        .map(|constraint| constraint.constraint)
        .collect::<Vec<_>>();
    assert!(
        oew_constraint_kinds
            .iter()
            .all(|constraint| PhaseLimits::GROUND.admits(*constraint)),
        "bare OEW must keep only ground-reaction/gear-capacity constraints, got {oew_constraint_kinds:?}"
    );
    assert!(oew_constraint_kinds.contains(&ModelCgConstraint::MinimumNoseGearLoad));

    for state in [
        ModelCgLoadingState::AnalyzedZeroFuel,
        ModelCgLoadingState::OperationalMidMission,
        ModelCgLoadingState::OperationalReserve,
        ModelCgLoadingState::AnalyzedTakeoff,
    ] {
        let flight_state = assessment
            .loading_states
            .iter()
            .find(|candidate| candidate.state == state)
            .unwrap_or_else(|| panic!("{state:?} state present"));
        // The exact count is preset-dependent (declared tire capacity,
        // fuselage scrape geometry), so this checks every flight
        // constraint this model can raise is present rather than a
        // fixed total.
        for constraint in [
            ModelCgConstraint::StaticStabilityFloor,
            ModelCgConstraint::PhysicalForwardCgLimit,
            ModelCgConstraint::MinimumNoseGearLoad,
            ModelCgConstraint::MaximumNoseGearLoadFraction,
            ModelCgConstraint::TipBack,
            ModelCgConstraint::MinimumUsableCgRange,
        ] {
            assert!(
                flight_state
                    .constraints
                    .iter()
                    .any(|candidate| candidate.constraint == constraint),
                "{state:?} is flight-eligible and must keep {constraint:?}"
            );
        }
    }
}

/// Excluding OEW's flight constraints
/// must not accidentally waive its ground-reaction checks. Dragging bare
/// OEW's own components aft to the tail cone starves the nose gear, which
/// must still fail `MinimumNoseGearLoad` and the overall envelope.
#[test]
fn oew_ground_reaction_violation_still_fails_the_envelope() {
    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), false)
        .expect("default geometry");
    let (masses, coordinates, cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let x_np = cg[0] + 0.10 * plane.c_ref;
    let fus_end_x = plane.fuselages[0]
        .xsecs
        .last()
        .expect("fuselage section")
        .xyz_c[0];

    let mut aft_oew_coordinates = coordinates;
    for x in [
        &mut aft_oew_coordinates.wing[0],
        &mut aft_oew_coordinates.h_stab[0],
        &mut aft_oew_coordinates.v_stab[0],
        &mut aft_oew_coordinates.fuselage[0],
        &mut aft_oew_coordinates.gear[0],
        &mut aft_oew_coordinates.propulsion[0],
        &mut aft_oew_coordinates.systems[0],
        &mut aft_oew_coordinates.furnishings[0],
    ] {
        *x = fus_end_x;
    }

    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &aft_oew_coordinates,
        cg[0],
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .expect("model assessment with an aft-dragged bare OEW");

    let oew = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::OperatingEmpty)
        .expect("OEW state");
    assert!(
        oew.nose_gear_load_fraction < config.mass_model.pct_load_nlg_min,
        "fixture must actually starve the nose gear to exercise the ground check"
    );
    assert!(
        oew.constraints.iter().any(|constraint| {
            constraint.constraint == ModelCgConstraint::MinimumNoseGearLoad && constraint.violated
        }),
        "the ground-reaction check must not be waived for a ground-only state"
    );
    assert!(!assessment.hard_constraints_pass());
}

/// A registered aircraft and the lumped mass state both envelope paths
/// are evaluated at. The lumped analysis places its own gear point and
/// never consults `alas_mass::stations`, so this builds the same inputs
/// for an aircraft whose main-gear station the station model refuses.
#[allow(clippy::expect_used)]
pub(super) fn preset_case(
    preset: &str,
) -> (AlasConfig, Airplane, MassBreakdown, MassCoordinates, f64) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{error}"));
    if preset == "ATR72-600" {
        // The refusal-path figure tests deliberately clear the source
        // anchors; the registered ATR now carries measured stations.
        config.landing_gear.reference_station_fuselage_length_m = None;
        config.landing_gear.reference_nlg_x_fraction = None;
        config.landing_gear.reference_mlg_x_fractions = None;
    }
    let registered = alas_config::presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&registered.design_vector), true)
        .expect("a registered aircraft builds");
    let (masses, coordinates, cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    (config, plane, masses, coordinates, cg[0])
}

/// The A320-200 preset registers a maximum ramp weight
/// (78,400 kg) above its MTOW (78,000 kg); `registered_mrw_kg` must
/// read exactly that reference value, and an unregistered (clean-sheet)
/// preset name must fall back to `None` rather than a stale or guessed
/// weight.
#[test]
fn registered_mrw_kg_reads_the_a320_200_reference_and_is_none_for_a_clean_sheet_preset() {
    let a320 = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(registered_mrw_kg(&a320), Some(78_400.0));

    let mut clean_sheet = a320;
    clean_sheet.preset = String::new();
    assert_eq!(registered_mrw_kg(&clean_sheet), None);
}

/// The A320-200's own assessment must actually be sized at the
/// registered MRW, not silently drop back to MTOW: with `mrw_kg` wired
/// through, `size_landing_gear_at_design_state`'s reported nose/main
/// gear design loads at the design (most-aft) state must match a
/// direct call at the heavier of the two weights, not at MTOW alone.
#[test]
fn the_a320_200_assessment_sizes_gear_at_mrw_not_mtow_alone() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
    let x_np = cg_x + 0.10 * plane.c_ref;
    let mrw_kg = registered_mrw_kg(&config);
    assert_eq!(mrw_kg, Some(78_400.0));
    assert!(mrw_kg.unwrap() > config.requirements.mtow_kg);

    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("the A320-200 must be assessed: {error}"));
    let takeoff = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
        .expect("an analyzed takeoff state");
    // The nose/main gear strength constraints carry the rated tire
    // capacity as their `limit`, sized against the design weight
    // (max(MRW, MTOW)); both must still be finite and positive
    // with `mrw_kg` wired through, a genuine sizing result rather than
    // a silently dropped or non-finite MRW input.
    for constraint in [
        ModelCgConstraint::NoseGearStrength,
        ModelCgConstraint::MainGearStrength,
    ] {
        let result = takeoff
            .constraints
            .iter()
            .find(|item| item.constraint == constraint)
            .unwrap_or_else(|| panic!("{constraint:?} is reported for the analyzed takeoff"));
        assert!(
            result.limit.is_finite() && result.limit > 0.0,
            "{constraint:?} limit {} should be a finite positive rated capacity",
            result.limit
        );
    }
}

/// One MLW, mode-aware: the gate and the landing-trim evaluation take the
/// configuration's own `landing_mass_limit_kg` at the candidate MTOW, the
/// limit the mass model sizes the structure with. An A320-200 with a 90 t
/// candidate keeps its certified 66,000 kg MLW as a fixed aircraft and takes
/// its landing fraction (66,000/78,000) of the candidate as a clean sheet.
#[test]
fn the_gate_mlw_is_the_configurations_mode_aware_landing_mass_limit() {
    use alas_config::DesignMode;
    let (mut config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
    let x_np = cg_x + 0.10 * plane.c_ref;
    config.requirements.mtow_kg = 90_000.0;
    for (mode, expected_kg) in [
        (DesignMode::BaselineSandbox, 66_000.0),
        (DesignMode::ReferenceAdaptation, 66_000.0),
        (DesignMode::CleanSheet, 90_000.0 * 66_000.0 / 78_000.0),
    ] {
        config.optimizer.design_space.mode = mode;
        let assessment = assess_model_cg_envelope(
            &plane,
            &masses,
            &coordinates,
            cg_x,
            x_np,
            x_np,
            plane.c_ref,
            &config,
        )
        .unwrap_or_else(|error| panic!("{mode:?}: the A320-200 must be assessed: {error}"));
        assert_eq!(
            assessment.design_landing_mass_kg,
            config.landing_mass_limit_kg(90_000.0),
            "{mode:?}"
        );
        assert!(
            (assessment.design_landing_mass_kg - expected_kg).abs() < 1e-6,
            "{mode:?}: {}",
            assessment.design_landing_mass_kg
        );
        for state in &assessment.loading_states {
            assert_eq!(
                state.scope,
                PhaseLimits::for_state_at_mass(
                    state.state,
                    state.mass_kg,
                    assessment.design_landing_mass_kg
                ),
                "{mode:?} {:?}",
                state.state
            );
        }
    }
}

/// The ATR 72-600 is the registered high-wing, fuselage-sponson aircraft
/// that registers no gear-station anchor, so
/// `alas_mass::stations::main_gear_station` refuses to place its main
/// gear. This module rebuilds that same wing-mounted fallback itself, and
/// before this gate it computed the wheelbase, both gear-strength
/// capacities and `min_nose_gear_load` from the refused station: the
/// aircraft was reported with ground reactions about a point the mass
/// model declined to supply. The assessment must now refuse as a whole
/// and carry the two heights that decided it.
#[test]
fn an_aircraft_with_no_measured_main_gear_station_gets_no_model_cg_assessment() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("ATR72-600");
    let x_np = cg_x + 0.10 * plane.c_ref;

    match assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    ) {
        Err(ModelCgEnvelopeError::MainGearStationNotMeasured(
            StationError::MainGearStationNotMeasured {
                wing_root_z_m,
                fuselage_crown_z_m,
            },
        )) => {
            assert!(
                wing_root_z_m > fuselage_crown_z_m,
                "the refusal must carry the heights that decided it: root {wing_root_z_m} m \
                 against crown {fuselage_crown_z_m} m"
            );
        }
        other => panic!(
            "an aircraft with no measured main-gear station must refuse the whole model CG \
             assessment, got {other:?}"
        ),
    }
}

/// The frozen reference-compatibility path returns a Boolean rather than
/// a typed error, so it fails closed on the same refusal: no compliant
/// state is reported for an aircraft whose main-gear station was refused.
/// No exceedance magnitude is claimed, because none was measured.
#[test]
fn the_frozen_reference_path_fails_closed_without_a_measured_main_gear_station() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("ATR72-600");
    let x_np = cg_x + 0.10 * plane.c_ref;

    let result = check_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        plane.c_ref,
        &config,
    );

    assert!(
        result.violation,
        "the compatibility path must not report a missing main-gear datum as compliant"
    );
    assert_eq!(result.worst_exceedance, 0.0);
}

/// The refusal is scoped to the layout the wing-mounted fallback rule
/// excludes, not to the absence of a registered anchor. The DC-10
/// registers no anchor either and must still be assessed through that
/// fallback, at the same station, with every gear constraint,
/// `min_nose_gear_load` included, still evaluated.
///
/// B787-9 is intentionally not in this list: its preset
/// (`crates/alas-config/src/presets/widebody/american.rs`) gives it
/// `reference_nlg_x_fraction`/`reference_mlg_x_fractions`, so it is now
/// source-scaled rather than a wing-mounted fallback case; unrelated to
/// this module's MAC-06 fix.
#[test]
fn low_wing_fallback_aircraft_keep_their_assessment_and_their_gear_constraints() {
    {
        let preset = "AVE";
        let (config, plane, masses, coordinates, cg_x) = preset_case(preset);
        let x_np = cg_x + 0.10 * plane.c_ref;
        let assessment = assess_model_cg_envelope(
            &plane,
            &masses,
            &coordinates,
            cg_x,
            x_np,
            x_np,
            plane.c_ref,
            &config,
        )
        .unwrap_or_else(|error| panic!("{preset} must still be assessed: {error}"));

        // The station the assessment used is still the wing-mounted
        // fallback, unchanged: MAC leading edge plus the configured
        // fraction of the MAC.
        let mac = plane.c_ref;
        let x_mac_le = plane.wings[0].aerodynamic_center(0.25)[0] - 0.25 * mac;
        let expected_x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
        let fus = &plane.fuselages[0];
        let fus_start_x = fus.xsecs[0].xyz_c[0];
        let fus_len = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0] - fus_start_x;
        let stations = config.landing_gear.resolved_station_positions(
            fus_start_x + fus_len * config.mass_model.nlg_x_fraction,
            expected_x_mlg,
            fus_start_x,
            fus_len,
        );
        assert!(
            !stations.source_scaled,
            "{preset} is expected to reach the fallback, not a source anchor"
        );
        assert!((stations.x_mlg_m - expected_x_mlg).abs() < 1.0e-12);

        for state in &assessment.loading_states {
            assert!(
                state
                    .constraints
                    .iter()
                    .any(|constraint| constraint.constraint
                        == ModelCgConstraint::MinimumNoseGearLoad),
                "{preset}/{:?} must keep its minimum nose-gear load gate",
                state.state
            );
        }
        let flight_state = assessment
            .loading_states
            .iter()
            .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
            .unwrap_or_else(|| panic!("{preset} analyzed-TOW state"));
        assert!(
            flight_state.constraints.len() >= 6,
            "{preset} must keep every hard constraint, static margin included"
        );
        assert!(flight_state
            .constraints
            .iter()
            .all(|constraint| constraint.limit.is_finite()));
    }
}

/// The same aircraft with a complete source anchor is assessed normally,
/// so the refusal reads as the missing datum it is and not as a rule that
/// high-wing aircraft cannot be assessed.
#[test]
fn a_registered_station_anchor_restores_the_assessment_on_the_same_aircraft() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("ATR72-600");
    let x_np = cg_x + 0.10 * plane.c_ref;
    // Illustrative fractions of the active fuselage length, not ATR data:
    // this asserts the plumbing and asserts nothing about where the ATR's
    // gear actually is.
    let anchored = AlasConfig {
        landing_gear: alas_config::LandingGearConfig {
            reference_station_fuselage_length_m: Some(27.166),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.45, 0.45]),
            ..config.landing_gear.clone()
        },
        ..config
    };

    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &anchored,
    )
    .unwrap_or_else(|error| panic!("a registered anchor must restore the assessment: {error}"));
    assert!(!assessment.loading_states.is_empty());
}

/// The A320-200's registered main-gear station, Airbus' 17.71 m
/// nose-tip drawing dimension under a 12.64 m EASA A.064 wheelbase, sits
/// **forward** of the aerodynamic aft boundary this envelope derives from
/// the static-margin floor. That is the root cause of the baseline's
/// `minimum nose-gear load` residual on this preset: the envelope's aft
/// end is not the boundary that governs, so it admits centres of gravity
/// the gear cannot carry and the nose-load constraint fires as a symptom
/// at whichever loading state lands there.
///
/// The assessment must now name that, and must do so **without** moving
/// either published limit.
#[test]
fn the_a320_200_aerodynamic_aft_limit_is_not_the_governing_one() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
    // The neutral point measured for this preset, m aft of the nose tip
    // (`A320-200.model_audit.aerodynamics.neutral_point_x_m` of the preset
    // acceptance matrix). The shared
    // fixture's `cg_x + 0.10 * c_ref` stand-in is a synthetic margin about
    // a lumped centre of gravity and lands 1.7 m forward of it, which is
    // not the aeroplane whose aft boundary is in question.
    let x_np = 18.417_621_430_740_57;
    assert!(
        x_np > cg_x,
        "the recorded neutral point must still be aft of this fixture's centre of gravity"
    );
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("the A320-200 must be assessed: {error}"));

    assert!(
        assessment.main_gear_station_pct_mac < assessment.aerodynamic_aft_limit_pct_mac,
        "the registered main gear at {:.3} % MAC is expected forward of the aerodynamic aft \
         limit at {:.3} % MAC",
        assessment.main_gear_station_pct_mac,
        assessment.aerodynamic_aft_limit_pct_mac
    );
    // Both non-aerodynamic mechanisms sit forward of, and are not, the
    // aerodynamic boundary this test's own name is about.
    match assessment.aft_limit_governance {
        AftCgLimitGovernance::GroundMinimumNoseLoad { margin_pct_mac }
        | AftCgLimitGovernance::TipBack { margin_pct_mac } => assert!(
            margin_pct_mac > 0.0,
            "the overhang must be positive, got {margin_pct_mac} % MAC"
        ),
        other @ AftCgLimitGovernance::Aerodynamic | other @ AftCgLimitGovernance::NotEvaluated => {
            panic!("a non-aerodynamic boundary must govern this layout, got {other:?}")
        }
    }

    // Published and physical references, not copies of model output. Frame:
    // x metres aft of the nose tip, % MAC from LEMAC.
    let mac_frame = plane.mac_frame().expect("a main wing");
    let to_m = |pct_mac: f64| mac_frame.x_lemac_m + pct_mac / 100.0 * plane.c_ref;

    // The registered main-gear station is Airbus' 17.71 m nose-tip drawing
    // dimension [S Airbus A320 Aircraft Characteristics AC, section 2].
    assert!(
        (to_m(assessment.main_gear_station_pct_mac) - 17.71).abs() < 0.30,
        "main gear at {:.2} m against the published 17.71 m",
        to_m(assessment.main_gear_station_pct_mac)
    );
    // The ground minimum-nose-load aft boundary is the class share (3.52 %,
    // the lowest certified one), not the A320's own 7.0 % at the 78,000 kg
    // takeoff state (37.08 % MAC, linear between WV007 and WV017 [S Airbus
    // A320 AC Jun 01/24, Figure 7-3-0-991-010-A01]): finding F-NOSE-1, the
    // ground limit lies more than 5 % MAC aft of the published aft CG.
    assert!(
        assessment.ground_aft_limit_pct_mac > 37.08 + 5.0,
        "finding F-NOSE-1 no longer holds: ground aft limit {:.2} % MAC against the published 37.08 %",
        assessment.ground_aft_limit_pct_mac
    );
    // The forward limit against the published 17 % MAC is its own test,
    // `the_a320_200_forward_limit_is_near_the_published_acap_value`.
    // Thrust and rolling friction push the rotation limit aft of the bare
    // moment balance: a nose-down pitching moment of the thrust line and
    // ground friction, so the shift carries a negative sign here.
    let shift = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
        .expect("analyzed takeoff state")
        .physical_limits
        .rotation_longitudinal_force_shift_pct_mac;
    assert!(
        shift < 0.0,
        "thrust and rolling-friction shift {shift} % MAC"
    );
    // By definition the aerodynamic aft limit is the critical neutral point
    // less the stability margin (the configured floor or 0.02 tail arms).
    let np_pct = 100.0 * (x_np - mac_frame.x_lemac_m) / plane.c_ref;
    assert!(
        assessment.minimum_physical_static_margin >= config.requirements.min_physical_static_margin
    );
    assert!(
        (assessment.aerodynamic_aft_limit_pct_mac
            - (np_pct - 100.0 * assessment.minimum_physical_static_margin))
            .abs()
            < 1.0e-6,
        "aerodynamic aft limit {:.3} % MAC, neutral point {np_pct:.3} % MAC",
        assessment.aerodynamic_aft_limit_pct_mac
    );
    // The forward limit sits forward of every aft limit.
    assert!(
        assessment.configured_forward_limit_pct_mac < assessment.ground_aft_limit_pct_mac,
        "the usable CG range must be positive"
    );
    // With the class nose share the ground boundary lies aft of tip-back,
    // measured from the belly ground plane, which governs. The overhang is
    // measured against the aerodynamic boundary and is positive.
    let overhang = assessment.aft_limit_governance.overhang_pct_mac();
    assert!(
        matches!(
            assessment.aft_limit_governance,
            AftCgLimitGovernance::TipBack { .. }
        ) && overhang > 0.0,
        "governance {:?}",
        assessment.aft_limit_governance
    );
    assert!(
        assessment.worst_aft_limit_pct_mac < assessment.ground_aft_limit_pct_mac,
        "governing aft limit {:.2} % MAC against the ground limit {:.2} % MAC",
        assessment.worst_aft_limit_pct_mac,
        assessment.ground_aft_limit_pct_mac
    );
    // The scissor-plot estimate is diagnostic only and not a
    // governance candidate, so the governing forward limit need not
    // (and, on this layout, does not) match it. With the Torenbeek
    // wing/fuselage/nacelle terms on the lattice wing aerodynamic centre
    // (eq. E-36) and the clean `C_L0` in eq. E-40 the landing-trim boundary
    // sits aft of the maximum-nose-load-handling candidate and governs.
    assert!(
        assessment.configured_forward_limit_pct_mac > assessment.scissor_plot_fwd_limit_pct_mac
    );
    assert!(
        assessment.configured_forward_limit_pct_mac > assessment.max_nose_load_fwd_limit_pct_mac,
        "forward {:.2} % MAC, nose load {:.2} % MAC",
        assessment.configured_forward_limit_pct_mac,
        assessment.max_nose_load_fwd_limit_pct_mac
    );

    assert!(assessment.governing_aft_limit_pct_mac() < assessment.aerodynamic_aft_limit_pct_mac);
}

/// The A320-200 envelope-wide forward limit against the published 17 % MAC
/// ACAP most-forward CG [S Airbus A320 AC, Fig. 7-3-0-991-010], within a
/// 3 % MAC band [E].
///
/// Finding F-ROT-1 (rotation authority exceeds the published forward
/// limit). With the tail lift at rotation derived from the tail geometry
/// and full up-elevator (CL_h about -0.97 after the plain-flap
/// large-deflection correction) and the pitch inertia transferred to the
/// main-gear contact, nose-wheel lift-off is possible forward of 17 % MAC; the
/// envelope-wide forward
/// limit is then more than 3 % MAC forward of
/// the ACAP value. A published forward limit is the worst of several
/// criteria (FAA AC 25-7D, sec. 42.11); the ones that set the A320's are
/// not all modelled. The band is kept, not widened: the test asserts the
/// finding, and fails when the model's limit returns inside the band, which
/// is the point to assert the band again.
#[test]
fn the_a320_200_forward_limit_is_forward_of_the_published_acap_band() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
    let x_np = 18.417_621_430_740_57;
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("the A320-200 must be assessed: {error}"));
    assert!(
        assessment.configured_forward_limit_pct_mac < 17.0 - 3.0,
        "finding F-ROT-1 no longer holds: forward limit {:.2} % MAC is within 3 % MAC of \
         the published 17 %; assert the band again",
        assessment.configured_forward_limit_pct_mac
    );
}

/// The contrast case that keeps the diagnostic from being a blanket
/// verdict on every aircraft: the A340-300's wheel-count-weighted
/// main-gear centroid sits far enough aft that its stability boundary is
/// reached first, so the aerodynamic limit does govern and no overhang is
/// reported.
#[test]
fn the_a340_300_aerodynamic_aft_limit_governs_its_own_layout() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("A340-300");
    // The same baseline's recorded neutral point for this preset, m aft of
    // the nose tip, for the same reason as the A320-200 case above.
    let x_np = 30.944_143_305_533_34;
    assert!(x_np > cg_x);
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("the A340-300 must be assessed: {error}"));

    assert_eq!(
        assessment.aft_limit_governance,
        AftCgLimitGovernance::Aerodynamic,
        "ground aft limit {:.3} % MAC against aerodynamic {:.3} % MAC",
        assessment.ground_aft_limit_pct_mac,
        assessment.aerodynamic_aft_limit_pct_mac
    );
    assert_eq!(assessment.aft_limit_governance.overhang_pct_mac(), 0.0);
    assert!(
        (assessment.governing_aft_limit_pct_mac() - assessment.aerodynamic_aft_limit_pct_mac).abs()
            < 1.0e-12
    );
}

/// The ground boundary is the same two-point split the loading states
/// use, so it must reproduce the nose-load constraint exactly: a centre
/// of gravity placed on it carries precisely that state's minimum nose
/// load, and the boundary sits forward of the main-gear station by the
/// nose-load fraction of the wheelbase.
#[test]
fn the_ground_aft_boundary_is_the_state_nose_load_split_read_backwards() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
    let x_np = cg_x + 0.10 * plane.c_ref;
    let assessment = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("the A320-200 must be assessed: {error}"));

    let fus = &plane.fuselages[0];
    let fus_start_x = fus.xsecs[0].xyz_c[0];
    let fus_len = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0] - fus_start_x;
    let mac = plane.c_ref;
    // Mirrors the production `mac_frame()` LEMAC used inside
    // `assess_model_cg_envelope`, so this bit-for-bit comparison is against
    // the same frame production uses.
    let x_mac_le = plane.mac_frame().expect("a main wing").x_lemac_m;
    let stations = config.landing_gear.resolved_station_positions(
        fus_start_x + fus_len * config.mass_model.nlg_x_fraction,
        x_mac_le + config.mass_model.mlg_x_fraction_mac * mac,
        fus_start_x,
        fus_len,
    );
    assert!(
        stations.source_scaled,
        "the A320-200 registers published gear stations"
    );
    let wheelbase_m = stations.x_mlg_m - stations.x_nlg_m;
    // The top-level ground boundary is the analyzed takeoff state's, whose
    // minimum is the class nose share, the same for every aircraft.
    let minimum_nose_fraction = config.mass_model.pct_load_nlg_min;
    let expected_pct_mac =
        100.0 * (stations.x_mlg_m - minimum_nose_fraction * wheelbase_m - x_mac_le) / mac;
    assert!(
        (assessment.ground_aft_limit_pct_mac - expected_pct_mac).abs() < 1.0e-9,
        "{} against {expected_pct_mac}",
        assessment.ground_aft_limit_pct_mac
    );

    // A state balanced on the boundary carries exactly the minimum.
    let x_cg = x_mac_le + assessment.ground_aft_limit_pct_mac / 100.0 * mac;
    let nose_fraction = (stations.x_mlg_m - x_cg) / wheelbase_m;
    assert!(
        (nose_fraction - minimum_nose_fraction).abs() < 1.0e-9,
        "{nose_fraction} against {minimum_nose_fraction}"
    );
}

/// A pure change of the reference frame (the published MAC and LEMAC, the
/// built airplane unchanged) moves no physical CG boundary in metres: the
/// wing-body centre, its moment and every ground boundary are stations and
/// moments of the airplane, and only their %MAC changes. The aerodynamic aft
/// boundary is left out: its static-margin floor is a fraction of the
/// reference chord by definition.
#[test]
fn a_reference_frame_change_moves_no_physical_cg_limit_in_metres() {
    let (config, plane, masses, coordinates, cg_x) = preset_case("B747-400");
    let frame = plane.mac_frame().expect("a frame");
    let mut moved = config.clone();
    moved.geometry.wing.reference_wing.published_mac_m = Some(1.2 * frame.chord_m);
    moved.geometry.wing.reference_wing.published_mac_le_x_m = Some(frame.x_lemac_m - 1.0);
    let registered = alas_config::presets::get("B747-400").expect("a registered preset");
    let other = AircraftBuilder::new(Some(moved.geometry.clone()))
        .build(Some(&registered.design_vector), true)
        .expect("the same aircraft builds");
    // The frame lives on the main wing's reference datum; the lofted
    // sections, the lattice and every other surface are the same.
    assert_eq!(other.wings.len(), plane.wings.len());
    for (wa, wb) in plane.wings.iter().zip(&other.wings) {
        assert_eq!(
            wa.xsecs, wb.xsecs,
            "a frame change must not move the {} sections",
            wa.name
        );
    }
    assert_eq!(other.fuselages, plane.fuselages);
    let other_frame = other.mac_frame().expect("a frame");
    assert!(
        (other_frame.chord_m - frame.chord_m).abs() > 1.0,
        "{other_frame:?}"
    );
    assert!(
        (other_frame.x_lemac_m - frame.x_lemac_m).abs() > 0.5,
        "{other_frame:?}"
    );
    // The neutral point (clean, cruise and high-lift conditions, rigid: the
    // flexibility trend is a fraction of the reference chord by its source)
    // is a station of the airplane too.
    let np = |plane: &Airplane| {
        let input = alas_stab::neutral_point::NpConditionsInput {
            high_lift: alas_stab::neutral_point::HighLiftDevices::declared(
                config.performance.high_lift_chord_extension,
                &config.control_surfaces,
            ),
            include_flexibility: false,
            wing_stiffness: None,
            tail_flexibility: None,
            ..alas_stab::neutral_point::NpConditionsInput::default()
        };
        alas_stab::neutral_point::neutral_point_conditions(plane, &config.analysis, &input)
            .expect("the B747-400 neutral point")
    };
    let (np_a, np_b) = (np(&plane), np(&other));
    for (label, xa, xb) in [
        ("clean", np_a.clean_low_speed, np_b.clean_low_speed),
        ("cruise", np_a.cruise, np_b.cruise),
        ("high lift", np_a.high_lift, np_b.high_lift),
    ] {
        assert!(
            (xa - xb).abs() < 1.0e-6,
            "{label} neutral point {xa:.4} m against {xb:.4} m"
        );
    }
    let x_np = cg_x + 0.10 * frame.chord_m;
    let assess = |plane: &Airplane, config: &AlasConfig| {
        assess_model_cg_envelope(
            plane,
            &masses,
            &coordinates,
            cg_x,
            x_np,
            x_np,
            plane.c_ref,
            config,
        )
        .expect("the B747-400 is assessed")
    };
    let (a, b) = (assess(&plane, &config), assess(&other, &moved));
    let metres = |f: alas_geom::aircraft::mac_frame::MacFrame, pct: f64| {
        f.x_lemac_m + pct / 100.0 * f.chord_m
    };
    for (sa, sb) in a.loading_states.iter().zip(&b.loading_states) {
        let (la, lb) = (&sa.physical_limits, &sb.physical_limits);
        for (label, pa, pb) in [
            ("forward", la.fwd_limit_pct_mac, lb.fwd_limit_pct_mac),
            (
                "landing trim",
                la.landing_trim_fwd_pct_mac,
                lb.landing_trim_fwd_pct_mac,
            ),
            ("rotation", la.rotation_fwd_pct_mac, lb.rotation_fwd_pct_mac),
            (
                "scissor",
                la.scissor_plot_fwd_pct_mac,
                lb.scissor_plot_fwd_pct_mac,
            ),
            (
                "nose load",
                la.max_nose_load_fwd_pct_mac,
                lb.max_nose_load_fwd_pct_mac,
            ),
            ("ground aft", la.ground_aft_pct_mac, lb.ground_aft_pct_mac),
            ("tip-back", la.tip_back_aft_pct_mac, lb.tip_back_aft_pct_mac),
        ] {
            if !(pa.is_finite() || pb.is_finite()) {
                continue;
            }
            let (xa, xb) = (metres(frame, pa), metres(other_frame, pb));
            assert!(
                (xa - xb).abs() < 1.0e-6,
                "{:?} {label}: {xa:.4} m against {xb:.4} m ({pa:.3} / {pb:.3} %MAC)",
                sa.state
            );
        }
    }
}

/// Each loading state's pitch inertia is its own composition about its own
/// centre of gravity: exact at the ledger's three points, and at half the
/// take-off fuel equal to the operating-empty tensor, the payload and half
/// the fuel (half its own inertia) moved to that state's centre of gravity.
#[test]
fn the_state_pitch_inertia_composes_the_ledger_parts_about_the_state_cg() {
    // Parts: (mass, x, z, own I_yy about the part's centroid).
    let parts = [
        (100.0, 10.0, 1.0, 5_000.0),
        (30.0, 12.0, 1.5, 400.0),
        (50.0, 9.0, 0.5, 800.0),
    ];
    let compose = |parts: &[(f64, f64, f64, f64)]| {
        let mass: f64 = parts.iter().map(|p| p.0).sum();
        let x = parts.iter().map(|p| p.0 * p.1).sum::<f64>() / mass;
        let z = parts.iter().map(|p| p.0 * p.2).sum::<f64>() / mass;
        let inertia: f64 = parts
            .iter()
            .map(|p| p.3 + p.0 * ((p.1 - x).powi(2) + (p.2 - z).powi(2)))
            .sum();
        (mass, x, z, inertia)
    };
    let oew = compose(&parts[..1]);
    let zfw = compose(&parts[..2]);
    let tow = compose(&parts);
    let basis = super::LedgerLoadingBasis {
        oew_mass_kg: oew.0,
        oew_cg_x_m: oew.1,
        oew_cg_z_m: oew.2,
        zero_fuel_mass_kg: zfw.0,
        zero_fuel_cg_x_m: zfw.1,
        zero_fuel_cg_z_m: zfw.2,
        takeoff_mass_kg: tow.0,
        takeoff_cg_x_m: tow.1,
        takeoff_cg_z_m: tow.2,
        takeoff_pitch_inertia_kg_m2: tow.3,
        oew_pitch_inertia_kg_m2: oew.3,
        zero_fuel_pitch_inertia_kg_m2: zfw.3,
    };
    for point in [oew, zfw, tow] {
        let inertia = basis.pitch_inertia_about_kg_m2(point.0, point.1, point.2);
        assert!(
            (inertia - point.3).abs() < 1e-9 * point.3,
            "{inertia} vs {}",
            point.3
        );
    }
    let half = compose(&[parts[0], parts[1], (25.0, 9.0, 0.5, 400.0)]);
    let inertia = basis.pitch_inertia_about_kg_m2(half.0, half.1, half.2);
    assert!(
        (inertia - half.3).abs() < 1e-9 * half.3,
        "{inertia} vs {}",
        half.3
    );
    // Without the empty and zero-fuel tensors: the take-off radius of
    // gyration at the state's mass.
    let lumped = super::LedgerLoadingBasis {
        oew_pitch_inertia_kg_m2: f64::NAN,
        ..basis
    };
    let scaled = lumped.pitch_inertia_about_kg_m2(half.0, half.1, half.2);
    assert!((scaled - tow.3 / tow.0 * half.0).abs() < 1e-9 * scaled);
}
