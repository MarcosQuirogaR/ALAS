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
    calculate_component_masses_checked_with_gear, calculate_flops_mass_buildup, mean,
    wing_named_or_first, ComponentMassError, FlopsMassBuildup, MassBreakdown, MassCoordinateModel,
    MassCoordinates, PayloadLayoutSummary, ProductMassBuildup, AERODYNAMIC_CENTER_CHORD_FRACTION,
};

/// Determine the X, Y, Z physical locations of the centroid of each component:
/// `define_mass_coordinates`.
///
/// The three cabin groups (systems, furnishings and the lumped planning
/// payload) are all placed as fractions of the **installed cabin**, because
/// all three are distributed over the same floor. See `x_payload` below for why
/// the payload no longer sits at the centre of a block beginning at the forward
/// bulkhead, and what that was worth per aircraft.
pub fn define_mass_coordinates(
    plane: &Airplane,
    geometry_config: &GeometryConfig,
    requirements: Option<&DesignRequirements>,
    mass_model: Option<&MassModelConfig>,
) -> MassCoordinates {
    // Retained so the signature and every call site stay unchanged while the
    // cabin groups are placed from geometry alone; the payload's linear density
    // is still what `alas_mass::stations` uses for the payload station's
    // spatial extent.
    let _ = (requirements, mass_model);

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

    // Systems (avionics, ECS, APU) are concentrated in the forward equipment
    // bay and central cabin zone, including APU. Scaled with the installed
    // cabin: they are physically present over the whole of it regardless of how
    // many of its seats a particular run happens to book, so their position
    // must not move when only `num_passengers`/`payload_kg` changes: switching
    // a cabin preset from all-economy to a lower-density three-class at the
    // same fuselage length, for instance.
    let x_systems = cabin_start + 0.45 * cabin_len;
    // Furnishings (seats, galleys, etc.) and operational items, also installed
    // over the whole cabin, for the same reason.
    let x_furn = cabin_start + 0.50 * cabin_len;
    // Payload CG at the centre of the cabin it is distributed over.
    //
    // This used to be `cabin_start + 0.50 * occupied_len`, the centre of a
    // block that always begins at the FORWARD BULKHEAD. Whenever the payload
    // does not fill the cabin that is the aircraft's forward loading extreme
    // applied as if it were the neutral case, and it is asymmetric against the
    // two lines directly above: `x_systems` and `x_furn` are placed as
    // fractions of the installed cabin, because that is where installed
    // equipment sits. A lumped planning payload is distributed over the same
    // floor and an operator trims it into the certified envelope; it is not
    // loaded nose first.
    //
    // Measured (`alas-mass/examples/payload_station_matrix.rs`), the forward-
    // bulkhead form put the centroid this far forward of the cabin centre, and
    // moved the centre of gravity at maximum take-off mass by:
    //
    //   ATR72-600  fill 0.495  4.583 m forward  ->  1.435 m of CG
    //   A220-300   fill 0.570  6.125 m          ->  1.178 m
    //   A320-200   fill 0.706  3.910 m          ->  0.752 m
    //   AVE        fill 0.771  6.485 m          ->  0.633 m
    //   A340-300   fill 0.785  4.955 m          ->  0.553 m
    //   B787-9     fill 0.800  4.530 m          ->  0.516 m
    //   DC-10      fill 0.811  3.650 m          ->  0.352 m
    //   A380-800   fill 1.000  0.000 m          ->  0.000 m
    //
    // On the ATR 72-600 that 1.435 m is 57.4 % of its 2.499 m mean aerodynamic
    // chord. The correction consults no centre-of-gravity target and adds no
    // coefficient: it is the one symmetric placement available, and on an
    // aircraft whose payload fills its cabin it is exactly the previous value.
    //
    // The retired comment's concern was that a fuselage stretched beyond the
    // payload's need would shift the payload centre of gravity aft "for free",
    // so the optimizer "must pay a CG-mismatch penalty for unrealistic
    // stretch". That is an optimizer-stability argument, not a physical one: a
    // longer cabin carrying the same payload over a uniformly loaded floor does
    // move its centroid aft, exactly as `x_systems` and `x_furn` already do. An
    // implausible stretch is the geometry plausibility windows' to reject (the
    // fuselage fineness window exists for it), not something to suppress by
    // placing mass where it is not.
    //
    // The occupied length itself is no longer needed here: it describes the
    // payload's spatial EXTENT, which these lumped coordinates do not carry.
    // `alas_mass::stations::ComponentStations::payload_fallback` still reports
    // it, as that station's `extent_m`, which is the field that means it.
    let x_payload = cabin_start + 0.50 * cabin_len;

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
/// The reference-compatible path preserves the frozen forward-loaded payload
/// convention. The product path uses the installed-cabin centroid and reports invalid wingbox
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
    if matches!(
        coordinate_model,
        MassCoordinateModel::ReferenceCompatibility
    ) {
        restore_reference_payload_coordinate(
            &mut coordinates,
            plane,
            geometry_config,
            requirements,
            mass_model,
        );
    }
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

/// Frozen Python replay only. Product planning payloads remain centered in
/// the installed cabin; detailed layouts replace this coordinate afterwards.
fn restore_reference_payload_coordinate(
    coordinates: &mut MassCoordinates,
    plane: &Airplane,
    geometry: &GeometryConfig,
    requirements: Option<&DesignRequirements>,
    mass_model: Option<&MassModelConfig>,
) {
    let default_requirements = DesignRequirements::default();
    let default_model = MassModelConfig::default();
    let requirements = requirements.unwrap_or(&default_requirements);
    let model = mass_model.unwrap_or(&default_model);
    let fuselage = &plane.fuselages[0];
    let length = fuselage.xsecs.last().map_or(0.0, |x| x.xyz_c[0])
        - fuselage.xsecs.first().map_or(0.0, |x| x.xyz_c[0]);
    let start = geometry.fuselage.cabin_start_x_m;
    let cabin_length = (length - start - geometry.fuselage.tailcone_length_m).max(1.0);
    let occupied_length =
        cabin_length.min(requirements.payload_kg() / model.cabin_payload_density_kg_m.max(1.0e-6));
    coordinates.payload[0] = start + 0.5 * occupied_length;
}

/// Calculate the global center of gravity location `[X, Y, Z]` in meters:
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

/// Execute the full weight and balance analysis: `run_mass_analysis`.
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
    let mut coordinates =
        define_mass_coordinates(plane, geometry_config, Some(requirements), mass_model);
    restore_reference_payload_coordinate(
        &mut coordinates,
        plane,
        geometry_config,
        Some(requirements),
        mass_model,
    );
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
/// the main-wing and fallback payload coordinates differ. This does not mean different
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
    let (masses, coordinates, cg, _) = run_product_mass_analysis_with_groups(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
        payload_layout,
        coordinate_model,
        landing_gear,
    )?;
    Ok((masses, coordinates, cg))
}

/// The grouped product analysis: the eight lumped slots, their stations,
/// the physical CG and the pure-FLOPS buildup they came from.
pub type GroupedProductMassAnalysis = (
    MassBreakdown,
    MassCoordinates,
    [f64; 3],
    Option<Box<FlopsMassBuildup>>,
);

/// [`run_mass_analysis_with_model_checked_product_with_gear`], also returning
/// the FLOPS component groups when the production architecture produced them.
///
/// The item-level ledger needs the groups. Building it from the eight lumped
/// slots alone forces it to label its rows from the configuration rather than
/// from what was evaluated, which is how a run could claim FLOPS provenance
/// for a single lumped systems row it had never been given the buildup for.
///
/// # Errors
///
/// As [`run_mass_analysis_with_model_checked_product_with_gear`].
#[allow(clippy::too_many_arguments)]
pub fn run_product_mass_analysis_with_groups(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
    coordinate_model: MassCoordinateModel<'_>,
    landing_gear: &LandingGearConfig,
) -> Result<GroupedProductMassAnalysis, ComponentMassError> {
    let built = calculate_flops_mass_buildup(
        plane,
        requirements,
        geometry_config,
        control_surfaces,
        mass_model,
        landing_gear,
        cabin_config,
    )?;
    let (masses, mut flops) = match built {
        ProductMassBuildup::PureFlops(flops) => (flops.masses, Some(flops)),
        ProductMassBuildup::LegacyComparison(masses) => (masses, None),
    };
    let coordinates = define_mass_coordinates_with_model(
        plane,
        geometry_config,
        Some(requirements),
        mass_model,
        coordinate_model,
    )
    .map_err(ComponentMassError::Geometry)?;
    let (masses, coordinates, cg) =
        complete_mass_analysis(masses, coordinates, requirements, payload_layout);
    // `complete_mass_analysis` may replace the lumped payload and fuel with
    // the detailed load-case values.  Keep the grouped result synchronized
    // with the lumped slots returned beside it; otherwise the report would
    // expose a pre-layout payload while its component map exposed the final
    // one, making the supposedly single ledger disagree across export and
    // analysis consumers.
    if let Some(buildup) = flops.as_deref_mut() {
        buildup.masses = masses;
    }
    Ok((masses, coordinates, cg, flops))
}
