// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass-coordinate placement and weight-and-balance orchestration.

use alas_config::{
    CabinConfig, ControlSurfacesConfig, DesignRequirements, GeometryConfig, LandingGearConfig,
    MassModelConfig,
};
use alas_geom::aircraft::airplane::Airplane;

use crate::analysis::complete_mass_analysis;
use crate::wing_centroid::WingCentroidError;

use super::{
    calculate_component_masses, calculate_component_masses_checked,
    calculate_component_masses_checked_product_with_gear,
    calculate_component_masses_checked_with_gear, mean, wing_named_or_first, ComponentMassError,
    MassBreakdown, MassCoordinateModel, MassCoordinates, PayloadLayoutSummary,
    AERODYNAMIC_CENTER_CHORD_FRACTION,
};

/// Determine the X, Y, Z physical locations of the centroid of each component
/// -- `define_mass_coordinates`.
///
/// Payload is placed at the centre of the *occupied* cabin length (payload
/// mass / `mass_model.cabin_payload_density_kg_m`, capped at the full
/// available cabin) rather than always the full available cabin. This means
/// that stretching the fuselage beyond what the required payload physically
/// needs does NOT shift the payload CG aft for free -- the optimizer must pay
/// a CG-mismatch penalty for unrealistic stretch.
pub fn define_mass_coordinates(
    plane: &Airplane,
    geometry_config: &GeometryConfig,
    requirements: Option<&DesignRequirements>,
    mass_model: Option<&MassModelConfig>,
) -> MassCoordinates {
    let default_mass_model = MassModelConfig::default();
    let mm = mass_model.unwrap_or(&default_mass_model);
    let default_requirements = DesignRequirements::default();
    let req = requirements.unwrap_or(&default_requirements);

    let wing = wing_named_or_first(&plane.wings, "Main Wing");
    let fus = &plane.fuselages[0];

    let last_xsec = fus.xsecs.len() - 1;
    let fus_len = fus.xsecs[last_xsec].xyz_c[0] - fus.xsecs[0].xyz_c[0];
    let fus_z = fus.xsecs[0].xyz_c[2];

    let w_ac = wing.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION);
    let w_root_z = wing.xsecs[0].xyz_le[2];

    let cabin_start = geometry_config.fuselage.cabin_start_x_m;
    let tailcone_len = geometry_config.fuselage.tailcone_length_m;
    let cabin_len = (fus_len - cabin_start - tailcone_len).max(1.0);

    // Occupied cabin length: how much of the available cabin the PAYLOAD
    // actually needs at the configured linear density, capped at what's
    // physically available. A fuselage stretched beyond that need does not
    // move the payload centroid (and therefore the CG) aft "for free". This
    // must stay scoped to Payload only -- Systems and Furnishings are OEW
    // (installed-equipment) components below and use the full cabin_len
    // instead: they are physically present over the whole installed cabin
    // regardless of how many of those seats a particular run happens to book,
    // so their position must NOT move when only num_passengers/payload_kg
    // changes (e.g. switching a cabin preset from all-economy to a lower-
    // density 3-class at the SAME fuselage length). Tying OEW-component
    // position to occupied_len instead would drag the entire OEW-component CG
    // forward whenever a preset books fewer passengers, purely as an artifact
    // of the shorter occupied length rather than any real change to where the
    // installed equipment sits, corrupting CG-envelope compliance for an
    // otherwise correct lower-density 3-class config.
    let occupied_len = cabin_len.min(req.payload_kg() / mm.cabin_payload_density_kg_m.max(1e-6));

    // Systems (avionics, ECS, APU) are concentrated in the forward equipment
    // bay and central cabin zone, including APU. Scaled with the full
    // installed cabin length (NOT occupied_len -- see note above).
    let x_systems = cabin_start + 0.45 * cabin_len;
    // Furnishings (seats, galleys, etc.) and operational items -- also
    // installed over the full cabin, not the currently-booked payload.
    let x_furn = cabin_start + 0.50 * cabin_len;
    // Payload CG at the centre of the occupied cabin section (the one place
    // occupied_len is the physically correct choice).
    let x_payload = cabin_start + 0.50 * occupied_len;

    let mut coords = MassCoordinates {
        fuselage: [fus_len * 0.46, 0.0, fus_z],
        wing: [w_ac[0] + wing.xsecs[0].chord * 0.2, 0.0, w_root_z],
        h_stab: [
            fus_len - geometry_config.empennage.hstab_offset_from_tail_m / 2.0,
            0.0,
            fus_z + geometry_config.empennage.hstab_z_m,
        ],
        v_stab: [
            fus_len - geometry_config.empennage.vstab_offset_from_tail_m / 2.0,
            0.0,
            fus_z + geometry_config.empennage.vstab_z_m,
        ],
        gear: [w_ac[0], 0.0, w_root_z - 2.5],
        propulsion: [w_ac[0], 0.0, w_root_z - 1.0],
        systems: [x_systems, 0.0, fus_z],
        furnishings: [x_furn, 0.0, fus_z],
        payload: [x_payload, 0.0, fus_z],
        fuel: [w_ac[0], 0.0, w_root_z],
    };

    // Upstream wraps this block in a bare `try/except Exception: pass`. Every
    // step here (filtering by substring, indexing the first/last xsec of a
    // non-empty nacelle) is bounds-respecting once the `!nacelles.is_empty()`
    // guard has been checked, so nothing in a direct translation can panic
    // and there is no Rust equivalent of the swallowed exception to write.
    let nacelles: Vec<&_> = plane
        .fuselages
        .iter()
        .filter(|f| f.name.contains("Nacelle"))
        .collect();
    if !nacelles.is_empty() {
        let mut x_engines = Vec::with_capacity(nacelles.len());
        let mut y_engines = Vec::with_capacity(nacelles.len());
        let mut z_engines = Vec::with_capacity(nacelles.len());
        for nacelle in &nacelles {
            let last = nacelle.xsecs.len() - 1;
            let x_start = nacelle.xsecs[0].xyz_c[0];
            let length = nacelle.xsecs[last].xyz_c[0] - x_start;
            x_engines.push(x_start + length * 0.5);
            y_engines.push(nacelle.xsecs[0].xyz_c[1]);
            z_engines.push(nacelle.xsecs[0].xyz_c[2]);
        }
        coords.propulsion = [mean(&x_engines), mean(&y_engines), mean(&z_engines)];
    }

    coords
}

/// Determine component coordinates using an explicit coordinate model.
///
/// The reference-compatible path is infallible and numerically identical to
/// [`define_mass_coordinates`]. The structural path reports invalid wingbox
/// geometry or material configuration as a typed error; it never silently
/// falls back to the legacy point, because that would make an apparently
/// physical CG depend on an unreported compatibility behavior.
pub fn define_mass_coordinates_with_model(
    plane: &Airplane,
    geometry_config: &GeometryConfig,
    requirements: Option<&DesignRequirements>,
    mass_model: Option<&MassModelConfig>,
    coordinate_model: MassCoordinateModel<'_>,
) -> Result<MassCoordinates, WingCentroidError> {
    let mut coordinates = define_mass_coordinates(plane, geometry_config, requirements, mass_model);
    if let MassCoordinateModel::StructuralWingbox(structures) = coordinate_model {
        let default_requirements = DesignRequirements::default();
        let requirements = requirements.unwrap_or(&default_requirements);
        let wing = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .or_else(|| plane.wings.first())
            .ok_or(WingCentroidError::InsufficientSections)?;
        coordinates.wing =
            crate::wing_centroid::wing_structural_centroid(wing, requirements, structures)?.xyz_m;
    }
    Ok(coordinates)
}

/// Calculate the global center of gravity location `[X, Y, Z]` in meters --
/// `calculate_physical_cg`.
pub fn calculate_physical_cg(masses: &MassBreakdown, coords: &MassCoordinates) -> [f64; 3] {
    let mut moment = [0.0; 3];
    let mut total_mass = 0.0;
    for ((name, reported_mass), (_, xyz)) in masses.as_pairs().into_iter().zip(coords.as_pairs()) {
        // Fuel is a signed MTOW-closure diagnostic in `MassBreakdown`. Only a
        // checked, nonnegative value is a physical load. Other legacy
        // component estimates retain the established nonnegative clamp.
        let mass = if name == super::FUEL {
            masses.physical_fuel_mass_kg().unwrap_or(0.0)
        } else {
            reported_mass.max(0.0)
        };
        for axis in 0..3 {
            moment[axis] += mass * xyz[axis];
        }
        total_mass += mass;
    }
    if total_mass == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    [
        moment[0] / total_mass,
        moment[1] / total_mass,
        moment[2] / total_mass,
    ]
}

/// Execute the full weight and balance analysis -- `run_mass_analysis`.
///
/// A positive [`PayloadLayoutSummary`] replaces lumped payload and recomputes
/// [`super::FUEL`], so the optimizer's final CG reflects the detailed layout.
pub fn run_mass_analysis(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
) -> (MassBreakdown, MassCoordinates, [f64; 3]) {
    let masses = calculate_component_masses(plane, requirements, geometry_config, mass_model);
    let coordinates =
        define_mass_coordinates(plane, geometry_config, Some(requirements), mass_model);
    complete_mass_analysis(masses, coordinates, requirements, payload_layout)
}

/// Execute weight and balance through the selected systems-mass method. The
/// legacy [`run_mass_analysis`] remains the frozen compatibility path.
pub fn run_mass_analysis_checked(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
) -> Result<(MassBreakdown, MassCoordinates, [f64; 3]), ComponentMassError> {
    let masses = calculate_component_masses_checked(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
    )?;
    let coordinates =
        define_mass_coordinates(plane, geometry_config, Some(requirements), mass_model);
    Ok(complete_mass_analysis(
        masses,
        coordinates,
        requirements,
        payload_layout,
    ))
}

/// Execute weight and balance with an explicit mass-coordinate model.
///
/// Use [`MassCoordinateModel::ReferenceCompatibility`] when replaying the
/// frozen Python fixture and [`MassCoordinateModel::StructuralWingbox`] for a
/// physical product analysis. For one fixed aircraft input, both coordinate
/// paths share that run's component masses and detailed-payload replacement;
/// only the main-wing coordinate differs. This does not mean different
/// presets have identical masses: systems/furnishings scale with their
/// selected mass model and MTOW (or with declared FLOPS architecture).
pub fn run_mass_analysis_with_model(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
    coordinate_model: MassCoordinateModel<'_>,
) -> Result<(MassBreakdown, MassCoordinates, [f64; 3]), WingCentroidError> {
    let masses = calculate_component_masses(plane, requirements, geometry_config, mass_model);
    let coordinates = define_mass_coordinates_with_model(
        plane,
        geometry_config,
        Some(requirements),
        mass_model,
        coordinate_model,
    )?;
    Ok(complete_mass_analysis(
        masses,
        coordinates,
        requirements,
        payload_layout,
    ))
}

/// Execute checked weight and balance with a selected main-wing coordinate model.
// The checked seam mirrors `run_mass_analysis_with_model` and must carry the
// same explicit physical inputs; bundling them would obscure compatibility
// with the existing public analysis API.
#[allow(clippy::too_many_arguments)]
pub fn run_mass_analysis_with_model_checked(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
    coordinate_model: MassCoordinateModel<'_>,
) -> Result<(MassBreakdown, MassCoordinates, [f64; 3]), ComponentMassError> {
    run_mass_analysis_with_model_checked_with_gear(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
        payload_layout,
        coordinate_model,
        &LandingGearConfig::default(),
    )
}

/// Execute checked weight and balance with a selected gear architecture.
#[allow(clippy::too_many_arguments)]
pub fn run_mass_analysis_with_model_checked_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
    coordinate_model: MassCoordinateModel<'_>,
    landing_gear: &LandingGearConfig,
) -> Result<(MassBreakdown, MassCoordinates, [f64; 3]), ComponentMassError> {
    let masses = calculate_component_masses_checked_with_gear(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
        landing_gear,
    )?;
    let coordinates = define_mass_coordinates_with_model(
        plane,
        geometry_config,
        Some(requirements),
        mass_model,
        coordinate_model,
    )
    .map_err(ComponentMassError::Geometry)?;
    Ok(complete_mass_analysis(
        masses,
        coordinates,
        requirements,
        payload_layout,
    ))
}

/// Execute the product mass path with configured high-lift area and landing
/// gear mounting, while retaining the selected systems-mass method's typed
/// validation.
#[allow(clippy::too_many_arguments)]
pub fn run_mass_analysis_with_model_checked_product_with_gear(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
    coordinate_model: MassCoordinateModel<'_>,
    landing_gear: &LandingGearConfig,
) -> Result<(MassBreakdown, MassCoordinates, [f64; 3]), ComponentMassError> {
    let masses = calculate_component_masses_checked_product_with_gear(
        plane,
        requirements,
        geometry_config,
        control_surfaces,
        mass_model,
        landing_gear,
        cabin_config,
    )?;
    let coordinates = define_mass_coordinates_with_model(
        plane,
        geometry_config,
        Some(requirements),
        mass_model,
        coordinate_model,
    )
    .map_err(ComponentMassError::Geometry)?;
    Ok(complete_mass_analysis(
        masses,
        coordinates,
        requirements,
        payload_layout,
    ))
}
