// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Assess hard model constraints across OEW, analyzed ZFW/TOW, and explicit
/// mid-mission/reserve fuel cases.
///
/// The aerodynamic aft boundary uses
/// [`alas_config::DesignRequirements::min_physical_static_margin`]. The
/// optimizer's `target_static_margin` is carried only as a reported
/// preference and cannot cause a hard violation.
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
    let x_nose_gear = fus_start_x + fuselage_length_m * config.mass_model.nlg_x_fraction;
    let x_main_gear = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
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
    let gear = size_landing_gear(
        mtow_mass_kg,
        x_nose_gear,
        x_main_gear,
        physical_forward_x_m,
        physical_aft_x_m,
        fuselage_diameter_m,
        fuselage_diameter_m * 1.1,
        &config.landing_gear,
    );
    let nose_gear_capacity_kg = mtow_mass_kg * gear.pct_load_nlg_max;
    let main_gear_capacity_kg = mtow_mass_kg * gear.pct_load_mlg_max;

    let mut loading_assessments = Vec::with_capacity(states.len());
    for (state, state_cg_x, mass_kg) in states {
        let nose_gear_load_kg = mass_kg * (x_main_gear - state_cg_x) / wheelbase_m;
        let main_gear_load_kg = mass_kg - nose_gear_load_kg;
        loading_assessments.push(assess_loading_constraints(LoadingConstraintInputs {
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
        }));
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

    let x_nlg = fus_start_x + fus_len * nlg_x_frac;
    let x_mlg = x_mac_le + mlg_x_frac_mac * mac;
    let wheelbase = x_mlg - x_nlg;

    let aero_fwd_lim_x = x_mac_le + aero_fwd_lim / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aero_aft_lim / 100.0 * mac;
    let fus_diam = config.geometry.fuselage.diameter_m;

    let gear_layout = size_landing_gear(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
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
        assess_loading_constraints, assess_model_cg_envelope, loading_states,
        LoadingConstraintInputs, ModelCgConstraint, ModelCgLoadingState,
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
}

