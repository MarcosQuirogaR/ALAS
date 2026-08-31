// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Calculate the masses of all primary aircraft components in kg --
/// `calculate_component_masses`.
///
/// Uses Torenbeek empirical methods calibrated for CS-25/FAR-25 class
/// transports. All empirical fractions come from `mass_model` so the user can
/// tune them from Advanced Settings -> Mass model.
pub fn calculate_component_masses(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    mass_model: Option<&MassModelConfig>,
) -> MassBreakdown {
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);
    let mtow_target = requirements.mtow_kg;
    let n_ult = requirements.ultimate_load_factor;
    let v_dive = requirements.dive_speed_m_s;

    let wing = wing_named_or_first(&plane.wings, "Main Wing");
    let hstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Horizontal Stabilizer")
        .unwrap_or_else(|| plane.wings.get(1).unwrap_or(wing));
    let vstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Vertical Stabilizer")
        .unwrap_or_else(|| plane.wings.get(2).unwrap_or(wing));

    let fus = &plane.fuselages[0];

    let m_wing = mass_wing(
        wing,
        mtow_target,
        n_ult,
        mtow_target * mm.suspended_mass_fraction,
        v_dive,
        mm.max_airspeed_for_flaps_ms,
        false,
        mm.flap_deflection_angle_deg,
        None,
    );

    let m_hstab = mass_wing(
        hstab,
        mtow_target,
        n_ult,
        0.0,
        v_dive,
        0.0,
        false,
        0.0,
        None,
    );

    let m_vstab = mass_wing(
        vstab,
        mtow_target,
        n_ult,
        0.0,
        v_dive,
        0.0,
        false,
        0.0,
        None,
    );

    let l_tail = (hstab.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION)[0]
        - wing.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION)[0])
        .max(1.0);
    let m_fus = mass_fuselage_simple(fus, v_dive, l_tail);

    let m_gear = mm.landing_gear_mass_fraction * mtow_target;

    // Technology-specific installed propulsion mass. Turbofans deliberately
    // retain the historical operation order and coefficients exactly. A
    // turboprop's compatibility `thrust_kn` is zero by design, so it is never
    // treated as missing thrust or converted into a fictitious static rating.
    let n_engines = geometry_config.engine.spanwise_positions_m.len();
    let estimate = match geometry_config.engine.active_model() {
        Ok(alas_config::ActiveEngineModel::Turbofan(spec)) => {
            crate::propulsion_mass::turbofan_installed_mass(
                spec.rated_thrust_kn * 1_000.0,
                n_engines,
                mm.propulsion_twr_factor,
                mm.propulsion_installation_factor,
                requirements.gravity_m_s2,
            )
        }
        Ok(alas_config::ActiveEngineModel::Turboprop(spec)) => {
            crate::propulsion_mass::turboprop_installed_mass(spec, n_engines)
        }
        Err(_) => None,
    };
    let m_prop = estimate.map_or(
        mm.propulsion_mass_fallback_fraction * mtow_target,
        |estimate| estimate.total_kg,
    );

    let m_sys = mm.systems_mass_fraction * mtow_target;
    let m_furn = mm.furnishings_mass_fraction * mtow_target;
    let m_payload = requirements.payload_kg();

    // OEW, MZFW, fuel -- named the same as upstream's own intermediates so
    // the summation order (and therefore the last-bit rounding) matches.
    let m_str = m_wing + m_hstab + m_vstab + m_fus + m_gear;
    let m_oew = m_str + m_prop + m_sys + m_furn;
    let m_mzfw = m_oew + m_payload;
    let m_fuel = mtow_target - m_mzfw;

    MassBreakdown {
        wing: m_wing,
        h_stab: m_hstab,
        v_stab: m_vstab,
        fuselage: m_fus,
        gear: m_gear,
        propulsion: m_prop,
        systems: m_sys,
        furnishings: m_furn,
        payload: m_payload,
        fuel: m_fuel,
    }
}

/// Calculate component masses with the selected systems-mass method.
///
/// The reference-compatible fraction path remains available through
/// [`calculate_component_masses`]. A selected FLOPS method is evaluated only
/// when all of its architecture inputs are declared; an incomplete method is
/// returned as an error instead of silently reverting to the fractions.
pub fn calculate_component_masses_checked(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
) -> Result<MassBreakdown, ComponentMassError> {
    calculate_component_masses_checked_with_gear(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
        &LandingGearConfig::default(),
    )
}

/// Checked mass buildup with the selected landing-gear architecture.
#[allow(clippy::too_many_arguments)]
pub fn calculate_component_masses_checked_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    _cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    landing_gear: &LandingGearConfig,
) -> Result<MassBreakdown, ComponentMassError> {
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);
    if mm.systems_mass_method.is_reference_compatible() {
        return Ok(calculate_component_masses(
            plane,
            requirements,
            geometry_config,
            mass_model,
        ));
    }

    calculate_component_masses_checked_product_with_gear(
        plane,
        requirements,
        geometry_config,
        control_surfaces,
        mass_model,
        landing_gear,
        _cabin_config,
    )
}

/// Product mass buildup with configured flap area and gear architecture,
/// retaining the selected systems-mass method's validation semantics.
///
/// This is separate from [`calculate_component_masses_checked_with_gear`] so
/// the latter can remain the explicit reference-compatible API when the
/// frozen fraction method is selected. Product analyses opt into this seam
/// even when they intentionally retain those historical systems fractions.
#[allow(clippy::too_many_arguments)]
pub fn calculate_component_masses_checked_product_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    landing_gear: &LandingGearConfig,
    _cabin_config: &CabinConfig,
) -> Result<MassBreakdown, ComponentMassError> {
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);

    let mut masses = calculate_component_masses_with_product_configuration(
        plane,
        requirements,
        geometry_config,
        mass_model,
        control_surfaces,
        landing_gear,
    );
    if mm.systems_mass_method.is_reference_compatible() {
        return Ok(masses);
    }
    let evaluation = evaluate_product(
        plane,
        requirements,
        geometry_config,
        control_surfaces,
        &mm.flops_transport,
    );
    let breakdown = match evaluation {
        FlopsTransportEvaluation::Verified { breakdown, .. } => breakdown,
        FlopsTransportEvaluation::Unverified { reasons, partial } => {
            return Err(ComponentMassError::FlopsUnverified {
                reasons,
                partial: Box::new(partial),
            });
        }
    };
    masses.systems = breakdown.systems.total_kg;
    masses.furnishings = breakdown.systems.furnishings_kg + breakdown.operating_items.total_kg;
    let oew = OEW_KEYS
        .iter()
        .filter_map(|name| masses.get(name))
        .sum::<f64>();
    masses.fuel = requirements.mtow_kg - oew - masses.payload;
    Ok(masses)
}

#[cfg(test)]
#[path = "../breakdown_tests.rs"]
mod tests;

