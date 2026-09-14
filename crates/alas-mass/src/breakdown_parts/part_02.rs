// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

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
    if !mm.architecture_is_coherent() {
        return Err(ComponentMassError::IncoherentMassArchitecture {
            architecture: mm.mass_architecture,
            systems: mm.systems_mass_method,
            structure: mm.structural_mass_method,
            propulsion: mm.propulsion_mass_method,
        });
    }
    if mm.uses_reference_mass_methods() {
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

/// Product mass buildup with configured flap area and gear architecture.
///
/// Under the production architecture this is the pure FLOPS buildup and the
/// frozen Torenbeek/fraction relations are not evaluated at all. Under the
/// comparison architecture it is the frozen buildup with the configured
/// control surfaces and gear layout reflected in the wing.
#[allow(clippy::too_many_arguments)]
pub fn calculate_component_masses_checked_product_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    landing_gear: &LandingGearConfig,
    cabin_config: &CabinConfig,
) -> Result<MassBreakdown, ComponentMassError> {
    calculate_flops_mass_buildup(
        plane,
        requirements,
        geometry_config,
        control_surfaces,
        mass_model,
        landing_gear,
        cabin_config,
    )
    .map(|built| match built {
        ProductMassBuildup::PureFlops(flops) => flops.masses,
        ProductMassBuildup::LegacyComparison(masses) => masses,
    })
}

/// Which architecture produced a product mass buildup, with its groups.
///
/// The FLOPS variant carries the component groups the item-level ledger
/// needs. Handing the ledger only the eight lumped slots is what let it label
/// rows "FLOPS" from the configuration while placing a single lumped systems
/// row it had never been given the buildup for.
#[derive(Debug, Clone, PartialEq)]
pub enum ProductMassBuildup {
    /// Every group is a FLOPS equation.
    PureFlops(Box<FlopsMassBuildup>),
    /// The frozen Torenbeek/fraction control, for comparison evidence only.
    LegacyComparison(MassBreakdown),
}

impl ProductMassBuildup {
    /// The eight operating-empty slots plus payload and fuel.
    pub fn masses(&self) -> &MassBreakdown {
        match self {
            Self::PureFlops(flops) => &flops.masses,
            Self::LegacyComparison(masses) => masses,
        }
    }

    /// The FLOPS groups, when this is a FLOPS buildup.
    pub fn flops(&self) -> Option<&FlopsMassBuildup> {
        match self {
            Self::PureFlops(flops) => Some(flops),
            Self::LegacyComparison(_) => None,
        }
    }
}

/// The product mass buildup, with the FLOPS component groups when the
/// production architecture produced it.
///
/// # Errors
///
/// [`ComponentMassError`] when a selected FLOPS input is missing or the
/// airframe evaluation is incomplete. There is no fallback to the comparison
/// architecture.
#[allow(clippy::too_many_arguments)]
pub fn calculate_flops_mass_buildup(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    landing_gear: &LandingGearConfig,
    _cabin_config: &CabinConfig,
) -> Result<ProductMassBuildup, ComponentMassError> {
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);

    if !mm.architecture_is_coherent() {
        return Err(ComponentMassError::IncoherentMassArchitecture {
            architecture: mm.mass_architecture,
            systems: mm.systems_mass_method,
            structure: mm.structural_mass_method,
            propulsion: mm.propulsion_mass_method,
        });
    }

    if mm.mass_architecture.is_pure_flops() {
        // The frozen buildup is not evaluated here at all: under the pure
        // architecture there is nothing for it to contribute, and computing
        // it first is how a legacy value used to survive into a group FLOPS
        // was supposed to own.
        return flops_methods::build_pure_flops(
            plane,
            requirements,
            geometry_config,
            control_surfaces,
            mm,
        )
        .map(|built| ProductMassBuildup::PureFlops(Box::new(built)));
    }

    Ok(ProductMassBuildup::LegacyComparison(
        calculate_component_masses_with_product_configuration(
            plane,
            requirements,
            geometry_config,
            mass_model,
            control_surfaces,
            landing_gear,
        ),
    ))
}

#[cfg(test)]
#[path = "../breakdown_tests.rs"]
mod tests;
