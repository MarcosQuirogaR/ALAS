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
        rotation_longitudinal_force_shift_pct_mac: 0.0,
        rotation_tail_lift_coefficient: -1.3,
        landing_trim_fwd_pct_mac: 9.5,
        usable_range_pct_mac: 80.0,
    }
}

fn passing_inputs() -> LoadingConstraintInputs {
    LoadingConstraintInputs {
        state: ModelCgLoadingState::AnalyzedTakeoff,
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
        tip_back_angle_deg: 20.0,
        required_tip_back_deg: 15.0,
        scrape_angle_deg: None,
        required_rotation_angle_deg: 12.0,
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
    assert_eq!(mtow_static_constraint.limit, 0.05);
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
    };
    assert_eq!(PhaseLimits::GROUND, phase(false, false, false));
    assert_eq!(PhaseLimits::TAKEOFF, phase(true, false, true));
    assert_eq!(PhaseLimits::FLIGHT, phase(false, true, true));
    assert_eq!(PhaseLimits::LANDING, phase(false, true, true));
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
/// carries its own phase-scoped limits, and its forward-limit constraint is
/// gated against exactly that state's own forward limit.
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
    // The landing state changes neither the envelope-wide values nor any
    // other state's verdicts.
    assert_eq!(
        assessment.configured_forward_limit_pct_mac,
        without_landing.configured_forward_limit_pct_mac
    );
    assert_eq!(
        &assessment.loading_states[..names.len() - 1],
        &without_landing.loading_states[..]
    );

    for state in &assessment.loading_states {
        let phase = PhaseLimits::for_state(state.state);
        let limits = state.physical_limits;
        assert_eq!(
            limits,
            limits.scoped(phase),
            "{:?} limits are not scoped",
            state.state
        );
        let (fwd, _) = limits.fwd_for(phase);
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
            _ => assert!(limits.fwd_limit_pct_mac >= limits.landing_trim_fwd_pct_mac - 1.0e-9),
        }
    }
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
fn preset_case(preset: &str) -> (AlasConfig, Airplane, MassBreakdown, MassCoordinates, f64) {
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
    // The ground minimum-nose-load aft boundary at the 78,000 kg takeoff
    // state reproduces the published aft CG there: 37.08 % MAC, linear
    // between WV007 (77,400 kg, 37.5 %) and WV017 (78,400 kg, 36.8 %)
    // [S Airbus A320 AC Jun 01/24, Figure 7-3-0-991-010-A01]. The band is
    // the model-frame MAC/LEMAC residual (1 % MAC).
    assert!(
        (assessment.ground_aft_limit_pct_mac - 37.08).abs() < 1.0,
        "ground aft limit {:.2} % MAC against the published 37.08 %",
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
    // less the physical static-margin floor.
    let np_pct = 100.0 * (x_np - mac_frame.x_lemac_m) / plane.c_ref;
    assert!(
        (assessment.aerodynamic_aft_limit_pct_mac
            - (np_pct - 100.0 * config.requirements.min_physical_static_margin))
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
    // The ground minimum nose load governs at the 78,000 kg takeoff state:
    // the published weight-variant splits (Airbus A320 AC Jun 01/24, Figure
    // 7-3-0-991-010-A01) give 7.0 % there, between WV007 (77,400 kg,
    // 37.5 % MAC) and WV017 (78,400 kg, 36.8 %). Tip-back, measured from the belly ground
    // plane, sits aft of it. The overhang is measured against the
    // aerodynamic boundary and is positive.
    let overhang = assessment.aft_limit_governance.overhang_pct_mac();
    assert!(
        matches!(
            assessment.aft_limit_governance,
            AftCgLimitGovernance::GroundMinimumNoseLoad { .. }
        ) && overhang > 0.0,
        "governance {:?}",
        assessment.aft_limit_governance
    );
    assert!(
        assessment.worst_aft_limit_pct_mac > 36.8 && assessment.worst_aft_limit_pct_mac < 40.0,
        "governing aft limit {:.2} % MAC against the published 36.8-40 % MAC",
        assessment.worst_aft_limit_pct_mac
    );
    // The scissor-plot estimate is diagnostic only and not a
    // governance candidate, so the governing forward limit need not
    // (and, on this layout, does not) match it; the rotation criterion
    // is more restrictive (more aft) than both the diagnostic scissor
    // estimate and the maximum-nose-load-handling candidate here.
    assert!(
        assessment.configured_forward_limit_pct_mac > assessment.scissor_plot_fwd_limit_pct_mac
    );
    assert!(
        assessment.configured_forward_limit_pct_mac > assessment.max_nose_load_fwd_limit_pct_mac
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
    // minimum is the published A320 nose share at that mass.
    let takeoff_mass_kg = assessment
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff)
        .expect("a takeoff state")
        .mass_kg;
    let minimum_nose_fraction = ledger_basis::minimum_nose_gear_fraction(
        &config,
        ledger_basis::registered_aft_cg_nose_load(&config),
        takeoff_mass_kg,
    );
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
