// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Whether `state` is, under this model's applicability contract, a loaded
/// flight/dispatch case to which the flight center-of-gravity range and
/// static-margin-floor constraints apply.
///
/// The contract treats bare OEW ([`ModelCgLoadingState::OperatingEmpty`]) as
/// a ground-only reference condition (it carries no payload or fuel, so it
/// is not one of the loaded flight/dispatch cases this envelope bounds)
/// and it is the only state excluded here. Every other named state (analyzed
/// ZFW, the explicit mid-mission/reserve fuel cases, and analyzed TOW) is a
/// loaded flight/dispatch case and remains fully gated.
fn is_flight_eligible_state(state: ModelCgLoadingState) -> bool {
    !matches!(state, ModelCgLoadingState::OperatingEmpty)
}

/// Whether `constraint` is a static ground-reaction/gear-capacity check that
/// stays meaningful for a state resting on its gear, independent of whether
/// that state is flight-eligible per [`is_flight_eligible_state`].
///
/// This is the complement of the flight CG-range/static-margin constraints;
/// it is what a ground-only state such as bare OEW keeps.
fn ground_reaction_constraint(constraint: ModelCgConstraint) -> bool {
    matches!(
        constraint,
        ModelCgConstraint::NoseGearStrength
            | ModelCgConstraint::MainGearStrength
            | ModelCgConstraint::MinimumNoseGearLoad
    )
}

/// Assess hard model constraints across OEW, analyzed ZFW/TOW, and explicit
/// mid-mission/reserve fuel cases.
///
/// The aerodynamic aft boundary uses
/// [`alas_config::DesignRequirements::min_physical_static_margin`]. The
/// optimizer's `target_static_margin` is carried only as a reported
/// preference and cannot cause a hard violation.
///
/// This model's applicability contract treats bare operating-empty weight
/// ([`ModelCgLoadingState::OperatingEmpty`]) as a ground-only reference
/// condition (it carries no payload or fuel) while the loaded
/// flight/dispatch cases (analyzed ZFW, the mid-mission/reserve fuel cases,
/// and analyzed TOW) retain the flight constraints. OEW's flight
/// center-of-gravity range and static-margin-floor constraints are
/// therefore not evaluated (see [`is_flight_eligible_state`]); its ground
/// static-reaction constraints (gear strength and minimum nose-gear load,
/// see [`ground_reaction_constraint`]) remain in force, so an OEW state that
/// fails ground equilibrium is still caught. See
/// `.agent/reports/2026-09-09-a220-five-state-cg-replay.html`, which traced
/// a `ConfiguredForwardCgRange` finding to bare OEW alone: a loaded-flight
/// constraint this ground-only reference condition was not meant to be
/// gated by.
#[allow(clippy::too_many_arguments)] // mirrors the reference-compatible seam beside it
pub fn assess_model_cg_envelope(
    plane: &Airplane,
    masses: &MassBreakdown,
    coords: &MassCoordinates,
    cg_x: f64,
    x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let wing = plane
        .wings
        .first()
        .ok_or(ModelCgEnvelopeError::MissingMainWing)?;
    let fuselage = plane
        .fuselages
        .first()
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;
    let fus_start_x = fuselage
        .xsecs
        .first()
        .map(|section| section.xyz_c[0])
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;
    let fus_end_x = fuselage
        .xsecs
        .last()
        .map(|section| section.xyz_c[0])
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;

    let (oew_mass, oew_cg_x) = oew_and_cg(masses, coords);
    let states = operational_loading_states(
        oew_mass,
        oew_cg_x,
        masses.payload,
        coords.payload[0],
        masses.fuel,
        coords.fuel[0],
        cg_x,
    );
    let values_are_finite = [
        cg_x,
        x_np,
        mac,
        fus_start_x,
        fus_end_x,
        config.requirements.min_physical_static_margin,
        config.requirements.target_static_margin,
        config.requirements.cg_range_pct_mac,
        config.mass_model.pct_load_nlg_min,
        coords.fuel[0],
    ]
    .into_iter()
    .all(f64::is_finite)
        && states
            .iter()
            .all(|(_, state_cg_x, mass_kg)| state_cg_x.is_finite() && mass_kg.is_finite());
    if !values_are_finite
        || mac <= 0.0
        || states.iter().any(|(_, _, mass_kg)| *mass_kg <= 0.0)
        || fus_end_x <= fus_start_x
    {
        return Err(ModelCgEnvelopeError::InvalidInput);
    }

    let x_wing_ac = wing.aerodynamic_center(0.25)[0];
    let x_mac_le = x_wing_ac - 0.25 * mac;
    let to_pct_mac = |x_m: f64| 100.0 * (x_m - x_mac_le) / mac;
    let neutral_point_pct_mac = to_pct_mac(x_np);
    let aerodynamic_aft_limit_pct_mac =
        neutral_point_pct_mac - 100.0 * config.requirements.min_physical_static_margin;
    let configured_forward_limit_pct_mac =
        aerodynamic_aft_limit_pct_mac - config.requirements.cg_range_pct_mac;

    let fuselage_length_m = fus_end_x - fus_start_x;
    let fallback_x_nose_gear = fus_start_x + fuselage_length_m * config.mass_model.nlg_x_fraction;
    let fallback_x_main_gear = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let gear_stations = config.landing_gear.resolved_station_positions(
        fallback_x_nose_gear,
        fallback_x_main_gear,
        fus_start_x,
        fuselage_length_m,
    );
    let x_nose_gear = gear_stations.x_nlg_m;
    let x_main_gear = gear_stations.x_mlg_m;
    let wheelbase_m = x_main_gear - x_nose_gear;
    if !x_wing_ac.is_finite()
        || !x_mac_le.is_finite()
        || !x_nose_gear.is_finite()
        || !x_main_gear.is_finite()
        || wheelbase_m <= 0.0
    {
        return Err(ModelCgEnvelopeError::InvalidInput);
    }

    let mtow_mass_kg = states
        .last()
        .map(|(_, _, mass_kg)| *mass_kg)
        .unwrap_or_default();
    let physical_forward_x_m = x_mac_le + configured_forward_limit_pct_mac / 100.0 * mac;
    let physical_aft_x_m = x_mac_le + aerodynamic_aft_limit_pct_mac / 100.0 * mac;
    let fuselage_diameter_m = config.geometry.fuselage.diameter_m;
    let gear = size_landing_gear_with_group_stations(
        mtow_mass_kg,
        x_nose_gear,
        x_main_gear,
        physical_forward_x_m,
        physical_aft_x_m,
        fuselage_diameter_m,
        fuselage_diameter_m * 1.1,
        &gear_stations.main_gear_x_m,
        &config.landing_gear,
    );
    let nose_gear_capacity_kg = mtow_mass_kg * gear.pct_load_nlg_max;
    let main_gear_capacity_kg = mtow_mass_kg * gear.pct_load_mlg_max;

    let mut loading_assessments = Vec::with_capacity(states.len());
    for (state, state_cg_x, mass_kg) in states {
        let nose_gear_load_kg = mass_kg * (x_main_gear - state_cg_x) / wheelbase_m;
        let main_gear_load_kg = mass_kg - nose_gear_load_kg;
        let mut assessment = assess_loading_constraints(LoadingConstraintInputs {
            state,
            mass_kg,
            cg_x_m: state_cg_x,
            cg_pct_mac: to_pct_mac(state_cg_x),
            static_margin: (x_np - state_cg_x) / mac,
            static_margin_floor: config.requirements.min_physical_static_margin,
            configured_forward_limit_pct_mac,
            nose_gear_load_kg,
            nose_gear_capacity_kg,
            main_gear_load_kg,
            main_gear_capacity_kg,
            minimum_nose_gear_load_fraction: config.mass_model.pct_load_nlg_min,
        });
        if !is_flight_eligible_state(state) {
            // This model's applicability contract treats bare OEW as a
            // ground-only reference condition, not a loaded flight/dispatch
            // case: keep only the ground-reaction constraints and drop the
            // flight CG-range/static-margin verdicts that do not apply to it.
            assessment
                .constraints
                .retain(|constraint| ground_reaction_constraint(constraint.constraint));
        }
        loading_assessments.push(assessment);
    }
    let mtow_static_margin = loading_assessments
        .iter()
        .find(|assessment| assessment.state == ModelCgLoadingState::AnalyzedTakeoff)
        .map(|assessment| assessment.static_margin)
        .ok_or(ModelCgEnvelopeError::InvalidInput)?;

    Ok(ModelCgEnvelopeAssessment {
        loading_states: loading_assessments,
        minimum_physical_static_margin: config.requirements.min_physical_static_margin,
        aerodynamic_aft_limit_pct_mac,
        configured_forward_limit_pct_mac,
        target_static_margin: StaticMarginPreferenceAssessment {
            actual: mtow_static_margin,
            target: config.requirements.target_static_margin,
            deviation: mtow_static_margin - config.requirements.target_static_margin,
        },
    })
}

/// Reproduce the frozen reference CG-envelope Boolean and exceedance.
///
/// This compatibility path retains the upstream use of the preferred static
/// margin as a boundary so the exact fixture remains meaningful. Product
/// analyses call [`assess_model_cg_envelope`] instead.
pub fn check_cg_envelope(
    plane: &Airplane,
    masses: &MassBreakdown,
    coords: &MassCoordinates,
    cg_x: f64,
    x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> CgEnvelopeResult {
    let req = &config.requirements;
    let mm = &config.mass_model;

    let (oew_mass, oew_cg_x) = oew_and_cg(masses, coords);

    // The second mass-analysis pass may replace the requested/lumped payload
    // with the detailed cabin or cargo layout. The envelope is checked against
    // that same physical loading state in Python; using the request here
    // gives MZFW and MTOW a different payload from the CG being checked.
    let payload_mass = masses.payload;
    let fuel_mass = masses.fuel;
    let loading_states = loading_states(
        oew_mass,
        oew_cg_x,
        payload_mass,
        coords.payload[0],
        cg_x,
        fuel_mass,
    );
    let mtow_mass = loading_states[2].1;

    let x_wing_ac_val = if !plane.wings.is_empty() {
        plane.wings[0].aerodynamic_center(0.25)[0]
    } else {
        0.0
    };
    let x_mac_le = x_wing_ac_val - 0.25 * mac;

    let to_pct = |x_val: f64| -> f64 { ((x_val - x_mac_le) / mac.max(0.001)) * 100.0 };

    let np_pct = to_pct(x_np);
    let aero_aft_lim = np_pct - req.target_static_margin * 100.0;
    let aero_fwd_lim = aero_aft_lim - req.cg_range_pct_mac;

    let nlg_x_frac = mm.nlg_x_fraction;
    let mlg_x_frac_mac = mm.mlg_x_fraction_mac;
    let pct_nlg_min = mm.pct_load_nlg_min;

    let fus = &plane.fuselages[0];
    let fus_start_x = fus.xsecs.first().map(|x| x.xyz_c[0]).unwrap_or(0.0);
    let fus_end_x = fus.xsecs.last().map(|x| x.xyz_c[0]).unwrap_or(0.0);
    let fus_len = fus_end_x - fus_start_x;

    let fallback_x_nlg = fus_start_x + fus_len * nlg_x_frac;
    let fallback_x_mlg = x_mac_le + mlg_x_frac_mac * mac;
    let gear_stations = config.landing_gear.resolved_station_positions(
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start_x,
        fus_len,
    );
    let x_nlg = gear_stations.x_nlg_m;
    let x_mlg = gear_stations.x_mlg_m;
    let wheelbase = x_mlg - x_nlg;

    let aero_fwd_lim_x = x_mac_le + aero_fwd_lim / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aero_aft_lim / 100.0 * mac;
    let fus_diam = config.geometry.fuselage.diameter_m;

    let gear_layout = size_landing_gear_with_group_stations(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
        &gear_stations.main_gear_x_m,
        &config.landing_gear,
    );
    let pct_nlg_max = gear_layout.pct_load_nlg_max;
    let pct_mlg_max = gear_layout.pct_load_mlg_max;

    let load_nlg_max = mtow_mass * pct_nlg_max;
    let load_mlg_max = mtow_mass * pct_mlg_max;
    let load_nlg_min = mtow_mass * pct_nlg_min;

    let mut worst_exc: f64 = 0.0;
    let mut violation = false;

    for (cg_val, w_state) in loading_states {
        let w_safe = w_state.max(1.0);
        let cg_pct = to_pct(cg_val);

        let nlg_strength_limit = to_pct(x_mlg - (load_nlg_max * wheelbase / w_safe));
        let mlg_strength_limit = to_pct(x_nlg + (load_mlg_max * wheelbase / w_safe));
        let min_nose_load_limit = to_pct(x_mlg - (load_nlg_min * wheelbase / w_safe));

        let fwd_lim_dynamic = aero_fwd_lim.max(nlg_strength_limit);
        let aft_lim_dynamic = aero_aft_lim
            .min(mlg_strength_limit)
            .min(min_nose_load_limit);

        let mut exc = 0.0;
        if cg_pct < fwd_lim_dynamic - 0.01 {
            exc = (fwd_lim_dynamic - cg_pct) / 100.0;
        } else if cg_pct > aft_lim_dynamic + 0.01 {
            exc = (cg_pct - aft_lim_dynamic) / 100.0;
        }

        if exc > 0.0 {
            violation = true;
            worst_exc = worst_exc.max(exc);
        }
    }

    CgEnvelopeResult {
        violation,
        worst_exceedance: worst_exc,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        assess_loading_constraints, assess_model_cg_envelope, ground_reaction_constraint,
        is_flight_eligible_state, loading_states, LoadingConstraintInputs, ModelCgConstraint,
        ModelCgLoadingState,
    };
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;
    use alas_mass::breakdown::run_mass_analysis;

    fn passing_inputs() -> LoadingConstraintInputs {
        LoadingConstraintInputs {
            state: ModelCgLoadingState::AnalyzedTakeoff,
            mass_kg: 100_000.0,
            cg_x_m: 10.0,
            cg_pct_mac: 25.0,
            static_margin: 0.08,
            static_margin_floor: 0.05,
            configured_forward_limit_pct_mac: 10.0,
            nose_gear_load_kg: 8_000.0,
            nose_gear_capacity_kg: 10_000.0,
            main_gear_load_kg: 92_000.0,
            main_gear_capacity_kg: 95_000.0,
            minimum_nose_gear_load_fraction: 0.02,
        }
    }

    fn assert_only_constraint_violates(
        inputs: LoadingConstraintInputs,
        expected: ModelCgConstraint,
    ) {
        let assessment = assess_loading_constraints(inputs);
        let violations = assessment
            .constraints
            .iter()
            .filter(|constraint| constraint.violated)
            .map(|constraint| constraint.constraint)
            .collect::<Vec<_>>();
        assert_eq!(violations, vec![expected]);
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
        inputs.static_margin = 0.049;
        assert_only_constraint_violates(inputs, ModelCgConstraint::StaticStabilityFloor);
    }

    #[test]
    fn configured_forward_range_governs_independently() {
        let mut inputs = passing_inputs();
        inputs.cg_pct_mac = 9.9;
        assert_only_constraint_violates(inputs, ModelCgConstraint::ConfiguredForwardCgRange);
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

    #[test]
    fn oew_is_the_only_state_excluded_from_flight_eligibility() {
        assert!(!is_flight_eligible_state(ModelCgLoadingState::OperatingEmpty));
        for state in [
            ModelCgLoadingState::AnalyzedZeroFuel,
            ModelCgLoadingState::OperationalMidMission,
            ModelCgLoadingState::OperationalReserve,
            ModelCgLoadingState::AnalyzedTakeoff,
        ] {
            assert!(
                is_flight_eligible_state(state),
                "{state:?} must remain flight-eligible"
            );
        }
    }

    #[test]
    fn ground_reaction_constraint_keeps_gear_checks_and_excludes_flight_cg_checks() {
        assert!(!ground_reaction_constraint(
            ModelCgConstraint::StaticStabilityFloor
        ));
        assert!(!ground_reaction_constraint(
            ModelCgConstraint::ConfiguredForwardCgRange
        ));
        assert!(ground_reaction_constraint(ModelCgConstraint::NoseGearStrength));
        assert!(ground_reaction_constraint(ModelCgConstraint::MainGearStrength));
        assert!(ground_reaction_constraint(
            ModelCgConstraint::MinimumNoseGearLoad
        ));
    }

    /// Regression for the 2026-09-09 A220 replay
    /// (`.agent/reports/2026-09-09-a220-five-state-cg-replay.html`): bare OEW
    /// was failing `ConfiguredForwardCgRange`, a flight constraint, even
    /// though OEW is a ground-only reference condition. This asserts the
    /// exact constraint set kept per state, so reintroducing the flight
    /// constraints on OEW (or dropping ground checks from it) both fail
    /// immediately regardless of the numeric CG/mass values in play.
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
        assert_eq!(
            oew_constraint_kinds,
            vec![
                ModelCgConstraint::NoseGearStrength,
                ModelCgConstraint::MainGearStrength,
                ModelCgConstraint::MinimumNoseGearLoad,
            ],
            "bare OEW must keep only ground-reaction/gear-capacity constraints"
        );

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
            assert_eq!(
                flight_state.constraints.len(),
                5,
                "{state:?} is flight-eligible and must keep every hard constraint"
            );
        }
    }

    /// Companion to the previous test: excluding OEW's flight constraints
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
                constraint.constraint == ModelCgConstraint::MinimumNoseGearLoad
                    && constraint.violated
            }),
            "the ground-reaction check must not be waived for a ground-only state"
        );
        assert!(!assessment.hard_constraints_pass());
    }
}
