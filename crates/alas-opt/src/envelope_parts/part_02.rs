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

/// The typed refusal when this aircraft has no main-gear longitudinal station
/// the mass model can supply, `None` when it has one.
///
/// Both envelope paths rebuild the mass model's own gear fallbacks
/// (`nlg_x_fraction` of fuselage length, `mlg_x_fraction_mac` aft of the MAC
/// leading edge) so they can feed `resolved_station_positions` the same
/// numbers `alas_mass::stations` would. The main-gear half of that pair is a
/// wing-mounted gear rule with a stated domain, and the one place that domain
/// is stated is `alas_mass::stations::main_gear_station`. This asks that owner
/// whether a station exists rather than restating the test here, so the two
/// crates cannot drift apart into accepting different aircraft.
///
/// A source-scaled resolution is a published station scaled onto the active
/// fuselage and is admissible on any layout, so the question is only asked
/// when the fallback is what would otherwise stand in, which is also what
/// keeps this off the search's hot path for every registered aircraft that
/// carries an anchor.
///
/// Only [`StationError::MainGearStationNotMeasured`] is reported. The other
/// station failures are degenerate-geometry conditions that both callers
/// already detect through their own missing-surface and finiteness checks.
fn unmeasured_main_gear_station(
    plane: &Airplane,
    config: &AlasConfig,
    gear_stations: &alas_config::LandingGearStationPositions,
) -> Option<StationError> {
    if gear_stations.source_scaled {
        return None;
    }
    match alas_mass::stations::component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    ) {
        Err(error @ StationError::MainGearStationNotMeasured { .. }) => Some(error),
        _ => None,
    }
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
/// fails ground equilibrium is still caught. See an internal A220 five-state
/// CG replay (2026-09-09), which traced
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
    // One declared MAC reference in the primary aircraft body frame (x aft
    // from the fuselage nose tip, metres; percent MAC dimensionless), used by
    // every percent-MAC number this function reports so a station and a limit
    // cannot end up referred to different chords.
    let mac_frame = MacFrame::new(x_mac_le, mac).ok_or(ModelCgEnvelopeError::InvalidInput)?;
    let to_pct_mac = move |x_m: f64| mac_frame.pct_mac(x_m).unwrap_or(f64::NAN);
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
    // `fallback_x_main_gear` above is the wing-mounted rule, admissible only
    // on the layouts `alas_mass::stations` states it for. Refuse the whole
    // assessment when this aircraft is outside that domain and registers no
    // station anchor: the wheelbase, both gear-strength capacities and
    // `min_nose_gear_load` below are all moments about that station, so a
    // refused station cannot be allowed to produce reported reactions.
    if let Some(error) = unmeasured_main_gear_station(plane, config, &gear_stations) {
        return Err(ModelCgEnvelopeError::MainGearStationNotMeasured(error));
    }
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

    // The second, independent aft boundary: the centre of gravity at which
    // the two-point static split leaves exactly `pct_load_nlg_min` of the
    // weight on the nose wheel. Aft of it the aeroplane runs out of steering
    // authority and then tips back; it is the tip-back/nose-load limit a real
    // aft CG limit is the more forward of, and this assessment has never
    // placed it. Same frame, same chord reference, same wheelbase as the
    // reactions above, so the comparison is like for like.
    let ground_aft_limit_x_m = x_main_gear - config.mass_model.pct_load_nlg_min * wheelbase_m;
    let ground_aft_limit_pct_mac = to_pct_mac(ground_aft_limit_x_m);
    let main_gear_station_pct_mac = to_pct_mac(x_main_gear);
    let aft_limit_governance = if !ground_aft_limit_pct_mac.is_finite() {
        AftCgLimitGovernance::NotEvaluated
    } else if aerodynamic_aft_limit_pct_mac > ground_aft_limit_pct_mac {
        AftCgLimitGovernance::GroundMinimumNoseLoad {
            margin_pct_mac: aerodynamic_aft_limit_pct_mac - ground_aft_limit_pct_mac,
        }
    } else {
        AftCgLimitGovernance::Aerodynamic
    };

    Ok(ModelCgEnvelopeAssessment {
        loading_states: loading_assessments,
        minimum_physical_static_margin: config.requirements.min_physical_static_margin,
        aerodynamic_aft_limit_pct_mac,
        configured_forward_limit_pct_mac,
        main_gear_station_pct_mac,
        ground_aft_limit_pct_mac,
        aft_limit_governance,
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
    // The same refusal as [`assess_model_cg_envelope`]. This path reports a
    // Boolean and an exceedance rather than a typed error, so it fails closed:
    // an aircraft with no measured main-gear station has no compliant state
    // to report. No exceedance magnitude is claimed, because none was
    // measured (a missing datum is not a distance past a limit) and the
    // violation Boolean is what marks the candidate rejected. The frozen
    // reference fixture is a low-wing aircraft whose fallback stands, so its
    // replayed values are unchanged.
    if unmeasured_main_gear_station(plane, config, &gear_stations).is_some() {
        return CgEnvelopeResult {
            violation: true,
            worst_exceedance: 0.0,
        };
    }
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

// Tests assert on inputs they constructed here, so a failed expect or panic is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::{
        assess_loading_constraints, assess_model_cg_envelope, check_cg_envelope,
        ground_reaction_constraint, is_flight_eligible_state, loading_states, AftCgLimitGovernance,
        LoadingConstraintInputs, ModelCgConstraint, ModelCgEnvelopeError, ModelCgLoadingState,
        StationError,
    };
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::builder::AircraftBuilder;
    use alas_mass::breakdown::{run_mass_analysis, MassBreakdown, MassCoordinates};

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

    /// A loaded state resting on both legs keeps all five constraints and is
    /// declared admissible; nothing about the ordinary case moved.
    #[test]
    fn a_state_resting_on_both_gear_groups_keeps_every_constraint() {
        let assessment = assess_loading_constraints(passing_inputs());
        assert!(assessment.ground_reactions_admissible);
        assert_eq!(assessment.constraints.len(), 5);
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
        assert_eq!(at_boundary.constraints.len(), 5);

        inputs.nose_gear_load_kg = -f64::MIN_POSITIVE;
        let past_boundary = assess_loading_constraints(inputs);
        assert!(!past_boundary.ground_reactions_admissible);
        assert_eq!(past_boundary.constraints.len(), 3);
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
        assert!(!is_flight_eligible_state(
            ModelCgLoadingState::OperatingEmpty
        ));
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
        assert!(ground_reaction_constraint(
            ModelCgConstraint::NoseGearStrength
        ));
        assert!(ground_reaction_constraint(
            ModelCgConstraint::MainGearStrength
        ));
        assert!(ground_reaction_constraint(
            ModelCgConstraint::MinimumNoseGearLoad
        ));
    }

    /// Regression for the 2026-09-09 A220 replay (an internal five-state CG
    /// replay): bare OEW was failing `ConfiguredForwardCgRange`, a flight
    /// constraint, even though OEW is a ground-only reference condition.
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

    /// A registered aircraft and the lumped mass state both envelope paths
    /// are evaluated at. The lumped analysis places its own gear point and
    /// never consults `alas_mass::stations`, so this builds the same inputs
    /// for an aircraft whose main-gear station the station model refuses.
    #[allow(clippy::expect_used)] // a registered preset that fails to build is the test failing
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
        let registered =
            alas_config::presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
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
    /// excludes, not to the absence of a registered anchor. The B787-9 and
    /// the DC-10 register no anchor either and must still be assessed through
    /// that fallback, at the same station, with every gear constraint,
    /// `min_nose_gear_load` included, still evaluated.
    #[test]
    fn low_wing_fallback_aircraft_keep_their_assessment_and_their_gear_constraints() {
        for preset in ["B787-9", "DC-10"] {
            let (config, plane, masses, coordinates, cg_x) = preset_case(preset);
            let x_np = cg_x + 0.10 * plane.c_ref;
            let assessment = assess_model_cg_envelope(
                &plane,
                &masses,
                &coordinates,
                cg_x,
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
            assert_eq!(
                flight_state.constraints.len(),
                5,
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
    /// `minimum nose-gear load` finding on this preset: the envelope's aft
    /// end is not the boundary that governs, so it admits centres of gravity
    /// the gear cannot carry and the nose-load constraint fires as a symptom
    /// at whichever loading state lands there.
    ///
    /// The assessment must now name that, and must do so **without** moving
    /// either published limit.
    #[test]
    fn the_a320_200_aerodynamic_aft_limit_is_not_the_governing_one() {
        let (config, plane, masses, coordinates, cg_x) = preset_case("A320-200");
        // The neutral point the 2026-09-22 eight-preset baseline measured for
        // this preset, m aft of the nose tip
        // (`preset_acceptance_matrix.json`,
        // `A320-200.model_audit.aerodynamics.neutral_point_x_m`). The shared
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
        match assessment.aft_limit_governance {
            AftCgLimitGovernance::GroundMinimumNoseLoad { margin_pct_mac } => assert!(
                margin_pct_mac > 0.0,
                "the overhang must be positive, got {margin_pct_mac} % MAC"
            ),
            other => panic!("the ground boundary must govern this layout, got {other:?}"),
        }

        // The measured band, pinned so a future geometry, gear-anchor or
        // neutral-point change that closes or widens it is a visible result
        // rather than a silent one. The tolerance is loose enough to survive
        // floating-point drift and tight enough that a centimetre-scale
        // station change shows up.
        for (measured, expected, name) in [
            (assessment.main_gear_station_pct_mac, 33.917, "main gear"),
            (
                assessment.aerodynamic_aft_limit_pct_mac,
                45.782,
                "aerodynamic aft limit",
            ),
            (
                assessment.ground_aft_limit_pct_mac,
                27.892,
                "ground minimum-nose-load aft boundary",
            ),
            (
                assessment.configured_forward_limit_pct_mac,
                15.782,
                "configured forward limit",
            ),
            (
                assessment.aft_limit_governance.overhang_pct_mac(),
                17.890,
                "overhang",
            ),
        ] {
            assert!(
                (measured - expected).abs() < 5.0e-3,
                "{name}: {measured:.4} % MAC against the pinned {expected:.3} % MAC"
            );
        }

        // The diagnostic tightens nothing and loosens nothing: the governing
        // limit is the more forward of the two, and the forward limit is
        // still measured from the aerodynamic boundary, not from it.
        assert!(
            assessment.governing_aft_limit_pct_mac() < assessment.aerodynamic_aft_limit_pct_mac
        );
        assert!(
            (assessment.configured_forward_limit_pct_mac
                - (assessment.aerodynamic_aft_limit_pct_mac
                    - config.requirements.cg_range_pct_mac))
                .abs()
                < 1.0e-9,
            "the forward limit must stay anchored on the aerodynamic aft boundary"
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
            (assessment.governing_aft_limit_pct_mac() - assessment.aerodynamic_aft_limit_pct_mac)
                .abs()
                < 1.0e-12
        );
    }

    /// The ground boundary is the same two-point split the loading states
    /// use, so it must reproduce the nose-load constraint exactly: a centre
    /// of gravity placed on it carries precisely the configured minimum nose
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
            plane.c_ref,
            &config,
        )
        .unwrap_or_else(|error| panic!("the A320-200 must be assessed: {error}"));

        let fus = &plane.fuselages[0];
        let fus_start_x = fus.xsecs[0].xyz_c[0];
        let fus_len = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0] - fus_start_x;
        let mac = plane.c_ref;
        let x_mac_le = plane.wings[0].aerodynamic_center(0.25)[0] - 0.25 * mac;
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
        let expected_pct_mac = 100.0
            * (stations.x_mlg_m - config.mass_model.pct_load_nlg_min * wheelbase_m - x_mac_le)
            / mac;
        assert!(
            (assessment.ground_aft_limit_pct_mac - expected_pct_mac).abs() < 1.0e-9,
            "{} against {expected_pct_mac}",
            assessment.ground_aft_limit_pct_mac
        );

        // A state balanced on the boundary carries exactly the minimum.
        let x_cg = x_mac_le + assessment.ground_aft_limit_pct_mac / 100.0 * mac;
        let nose_fraction = (stations.x_mlg_m - x_cg) / wheelbase_m;
        assert!(
            (nose_fraction - config.mass_model.pct_load_nlg_min).abs() < 1.0e-9,
            "{nose_fraction} against {}",
            config.mass_model.pct_load_nlg_min
        );
    }
}
