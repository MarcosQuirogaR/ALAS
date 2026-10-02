// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component mass buildup: the Torenbeek comparison relations and the product entry point.

use alas_config::{
    CabinConfig, ControlSurfacesConfig, DesignRequirements, GeometryConfig, LandingGearConfig,
    MassModelConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use super::types::AERODYNAMIC_CENTER_CHORD_FRACTION;
use super::{flops_methods, ComponentMassError, FlopsMassBuildup, MassBreakdown};
use crate::torenbeek::{mass_fuselage_simple, mass_wing, mass_wing_with_control_surface_area};

/// The wing named `name`, or the first wing if none matches: how
/// `calculate_component_masses` and `define_mass_coordinates` both find the
/// main wing.
pub(super) fn wing_named_or_first<'a>(wings: &'a [Wing], name: &str) -> &'a Wing {
    wings
        .iter()
        .find(|wing| wing.name == name)
        .unwrap_or(&wings[0])
}

/// Planform area occupied by the configured trailing-edge flap run.
///
/// `Wing` intentionally carries no control-surface subgeometry, so deriving
/// this at the mass boundary keeps drawing inputs and structural mass inputs
/// consistent without changing the geometry type. Chord is integrated
/// over the configured semi-span interval rather than approximated from the
/// total wing area, which handles tapered and multi-station wings.
pub(super) fn configured_flap_area(wing: &Wing, control_surfaces: &ControlSurfacesConfig) -> f64 {
    if wing.xsecs.len() < 2 {
        return 0.0;
    }
    let start = control_surfaces.flap_span_start_frac.clamp(0.0, 1.0);
    let end = control_surfaces.flap_span_end_frac.clamp(0.0, 1.0);
    let chord_fraction = control_surfaces.flap_chord_fraction.clamp(0.0, 1.0);
    if end <= start || chord_fraction <= 0.0 {
        return 0.0;
    }

    let section_lengths: Vec<f64> = wing
        .xsecs
        .windows(2)
        .map(|pair| {
            let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
            let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
            (dy * dy + dz * dz).sqrt()
        })
        .collect();
    let half_span: f64 = section_lengths.iter().sum();
    if !half_span.is_finite() || half_span <= 0.0 {
        return 0.0;
    }

    let mut area = 0.0;
    let mut station_start = 0.0;
    for (pair, segment_length) in wing.xsecs.windows(2).zip(section_lengths) {
        let station_end = station_start + segment_length / half_span;
        let overlap_start = start.max(station_start);
        let overlap_end = end.min(station_end);
        if overlap_end > overlap_start && station_end > station_start {
            let at = |fraction: f64| {
                let t = (fraction - station_start) / (station_end - station_start);
                pair[0].chord + t * (pair[1].chord - pair[0].chord)
            };
            let chord_start = at(overlap_start);
            let chord_end = at(overlap_end);
            area += half_span * (overlap_end - overlap_start) * (chord_start + chord_end) * 0.5;
        }
        station_start = station_end;
    }
    area * if wing.symmetric { 2.0 } else { 1.0 } * chord_fraction
}

/// Resolve whether the main gear has a wing attachment for the Torenbeek
/// wing-structure knockdown. One explicitly configured strut denotes
/// centreline/body gear; zero is the normal auto-sized transport layout,
/// which includes the left/right wing gear legs.
pub(super) fn main_gear_mounted_to_wing(landing_gear: &LandingGearConfig) -> bool {
    landing_gear.n_mlg_struts == 0 || landing_gear.n_mlg_struts >= 2
}

/// Product mass buildup with the configured flap geometry and gear layout.
///
/// `calculate_component_masses` is the plain Torenbeek/fraction entry point.
/// Checked product analyses call this seam so the user-selected control
/// surfaces and landing-gear architecture are reflected in the physical wing
/// mass.
fn calculate_component_masses_with_product_configuration(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    mass_model: Option<&MassModelConfig>,
    control_surfaces: &ControlSurfacesConfig,
    landing_gear: &LandingGearConfig,
) -> MassBreakdown {
    let mut masses = calculate_component_masses(plane, requirements, geometry_config, mass_model);
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);
    let wing = wing_named_or_first(&plane.wings, "Main Wing");
    masses.wing = mass_wing_with_control_surface_area(
        wing,
        requirements.mtow_kg,
        requirements.ultimate_load_factor,
        requirements.mtow_kg * mm.suspended_mass_fraction,
        requirements.dive_speed_m_s,
        mm.max_airspeed_for_flaps_ms,
        main_gear_mounted_to_wing(landing_gear),
        mm.flap_deflection_angle_deg,
        None,
        configured_flap_area(wing, control_surfaces),
    );
    let oew_without_fuel = masses.wing
        + masses.h_stab
        + masses.v_stab
        + masses.fuselage
        + masses.gear
        + masses.propulsion
        + masses.systems
        + masses.furnishings;
    masses.fuel = requirements.mtow_kg - oew_without_fuel - masses.payload;
    masses
}

pub(super) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

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

    // OEW, MZFW, fuel. The summation order is fixed so the result is
    // bit-reproducible.
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

/// Checked mass buildup with the selected mass architecture and landing-gear
/// layout.
///
/// The comparison architecture evaluates the Torenbeek/fraction relations
/// through [`calculate_component_masses`]. Any other architecture is the pure
/// FLOPS buildup, evaluated only when all of its architecture inputs are
/// declared; an incomplete input set is returned as an error instead of
/// reverting to fractions.
#[allow(clippy::too_many_arguments)]
pub fn calculate_component_masses_checked_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
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
    /// The Torenbeek/fraction buildup, selected by name as a comparison
    /// control; the product never falls back to it.
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
#[allow(clippy::too_many_arguments)] // one argument per configured mass input
pub fn calculate_flops_mass_buildup(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    landing_gear: &LandingGearConfig,
    cabin_config: &CabinConfig,
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
        // The Torenbeek/fraction buildup is not evaluated here at all: under
        // the pure architecture it has nothing to contribute, and computing
        // it first would let a fraction-based value survive into a group
        // that FLOPS owns.
        return flops_methods::build_pure_flops(
            plane,
            requirements,
            geometry_config,
            control_surfaces,
            cabin_config,
            mm,
            landing_gear,
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
