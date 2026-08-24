// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/mass.py
// Reference: alas @ rust-port-baseline.

//! Weight & balance: component mass buildup and centre-of-gravity estimation.
//!
//! [`calculate_component_masses`] estimates each component's mass from
//! [`crate::torenbeek`]'s empirical methods and mass fractions from
//! [`MassModelConfig`]; [`define_mass_coordinates`] places each centroid;
//! [`calculate_physical_cg`] combines them into a mass-weighted CG; and
//! [`run_mass_analysis`] orchestrates all three, with an optional payload
//! layout override.
//!
//! [`WING`] through [`FUEL`] are the ten names upstream's `Dict[str, float]`
//! masses and `Dict[str, List[float]]` coordinates use as keys. Both become a
//! struct with one named field per component -- compile-time key safety over a
//! hashmap -- while the constants and the `as_pairs` methods give back the
//! name-keyed iteration the dict-shaped callers ([`calculate_physical_cg`],
//! [`OEW_KEYS`]'s summation) need. [`OEW_KEYS`] is the canonical OEW component
//! list upstream's module doc says every other consumer imports rather than
//! redefines, so it is `pub` here too.
//!
//! [`PayloadLayoutSummary`] is the seam to the not-yet-ported
//! `alas/physics/payload.py`; it reproduces the three fields read upstream.

use alas_config::{
    CabinConfig, ControlSurfacesConfig, DesignRequirements, GeometryConfig, MassModelConfig,
    StructuresConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use crate::analysis::complete_mass_analysis;
use crate::flops_transport::{
    evaluate_product, FlopsTransportEvaluation, FlopsTransportUnverifiedReason,
    PartialFlopsTransportBreakdown,
};
use crate::torenbeek::{mass_fuselage_simple, mass_wing};
use crate::wing_centroid::{wing_structural_centroid, WingCentroidError};

/// The wing structure.
pub const WING: &str = "Wing";
/// The horizontal stabilizer.
pub const H_STAB: &str = "H-Stab";
/// The vertical stabilizer.
pub const V_STAB: &str = "V-Stab";
/// The fuselage structure.
pub const FUSELAGE: &str = "Fuselage";
/// The landing gear.
pub const GEAR: &str = "Gear";
/// Engines, pylons and installation accessories.
pub const PROPULSION: &str = "Propulsion";
/// Avionics, electrical, ECS, APU and the like.
pub const SYSTEMS: &str = "Systems";
/// Seats, galleys, lavatories, insulation, crew and operational items.
pub const FURNISHINGS: &str = "Furnishings";
/// Passengers and/or cargo.
pub const PAYLOAD: &str = "Payload";
/// The fuel remainder: `MTOW - MZFW`.
pub const FUEL: &str = "Fuel";

/// The components that make up the Operating Empty Weight -- everything
/// except payload and fuel. This is the single canonical definition; every
/// other module that needs the OEW component set imports it from here rather
/// than redefining its own copy (upstream's module doc names
/// `optimization/objective.py`, `physics/payload.py`, reporting and the GUI).
pub const OEW_KEYS: [&str; 8] = [
    WING,
    H_STAB,
    V_STAB,
    FUSELAGE,
    GEAR,
    PROPULSION,
    SYSTEMS,
    FURNISHINGS,
];

/// Failure returned when a selected physical mass method cannot be verified.
///
/// The frozen compatibility method never returns this error. FLOPS does when
/// a required range, cabin, or installed-architecture datum is absent, so a
/// caller cannot mistake a missing physical input for a valid mass buildup.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ComponentMassError {
    /// The selected structural coordinate model could not be resolved.
    #[error("mass-coordinate geometry could not be resolved: {0}")]
    Geometry(#[source] WingCentroidError),
    /// NASA FLOPS inputs are incomplete or internally inconsistent.
    #[error("FLOPS transport mass method is unverified")]
    FlopsUnverified {
        /// Stable blockers that must be resolved before using the mass.
        reasons: Vec<FlopsTransportUnverifiedReason>,
        /// Independently available component projections, never a replacement
        /// for the complete verified buildup.
        partial: Box<PartialFlopsTransportBreakdown>,
    },
}

/// Every native aerodynamic model `Wing::aerodynamic_center` call in this module reads the
/// quarter-chord point -- upstream's `mass.py` never passes a
/// `chord_fraction` of its own, and native aerodynamic model's own default is 0.25.
const AERODYNAMIC_CENTER_CHORD_FRACTION: f64 = 0.25;

/// Which main-wing mass-coordinate model an analysis uses.
///
/// [`Self::ReferenceCompatibility`] is the translated Python coordinate and
/// remains available so the frozen parity fixture keeps testing the reference
/// implementation rather than an improvement. [`Self::StructuralWingbox`]
/// replaces only the main-wing point with a first moment integrated from the
/// configured spars, skins, ribs, materials, and ultimate maneuver load.
#[derive(Debug, Clone, Copy)]
pub enum MassCoordinateModel<'a> {
    /// Exact `alas/physics/mass.py` coordinate behavior.
    ReferenceCompatibility,
    /// Geometry- and structure-derived main-wing mass coordinate.
    StructuralWingbox(&'a StructuresConfig),
}

/// The mass of each primary component, in kg -- upstream's `Dict[str, float]`
/// with one field per canonical component name (see the module doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    /// [`WING`]'s mass.
    pub wing: f64,
    /// [`H_STAB`]'s mass.
    pub h_stab: f64,
    /// [`V_STAB`]'s mass.
    pub v_stab: f64,
    /// [`FUSELAGE`]'s mass.
    pub fuselage: f64,
    /// [`GEAR`]'s mass.
    pub gear: f64,
    /// [`PROPULSION`]'s mass.
    pub propulsion: f64,
    /// [`SYSTEMS`]'s mass.
    pub systems: f64,
    /// [`FURNISHINGS`]'s mass.
    pub furnishings: f64,
    /// [`PAYLOAD`]'s mass.
    pub payload: f64,
    /// [`FUEL`]'s mass.
    pub fuel: f64,
}

impl MassBreakdown {
    /// Every component paired with its canonical name, in the order upstream's
    /// dict literal writes them -- the generic iteration
    /// [`calculate_physical_cg`] and [`OEW_KEYS`]'s summation need.
    pub fn as_pairs(&self) -> [(&'static str, f64); 10] {
        [
            (WING, self.wing),
            (H_STAB, self.h_stab),
            (V_STAB, self.v_stab),
            (FUSELAGE, self.fuselage),
            (GEAR, self.gear),
            (PROPULSION, self.propulsion),
            (SYSTEMS, self.systems),
            (FURNISHINGS, self.furnishings),
            (PAYLOAD, self.payload),
            (FUEL, self.fuel),
        ]
    }

    /// The mass named `name`, or `None` if it is not one of the ten canonical
    /// components -- `dict.get`, for a caller (such as [`run_mass_analysis`]'s
    /// [`OEW_KEYS`] summation) that only has the name.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.as_pairs()
            .into_iter()
            .find(|&(candidate, _)| candidate == name)
            .map(|(_, mass)| mass)
    }
}

/// The `[x, y, z]` centroid of each primary component, in meters -- upstream's
/// `Dict[str, List[float]]` with one field per canonical component name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassCoordinates {
    /// [`WING`]'s centroid.
    pub wing: [f64; 3],
    /// [`H_STAB`]'s centroid.
    pub h_stab: [f64; 3],
    /// [`V_STAB`]'s centroid.
    pub v_stab: [f64; 3],
    /// [`FUSELAGE`]'s centroid.
    pub fuselage: [f64; 3],
    /// [`GEAR`]'s centroid.
    pub gear: [f64; 3],
    /// [`PROPULSION`]'s centroid.
    pub propulsion: [f64; 3],
    /// [`SYSTEMS`]'s centroid.
    pub systems: [f64; 3],
    /// [`FURNISHINGS`]'s centroid.
    pub furnishings: [f64; 3],
    /// [`PAYLOAD`]'s centroid.
    pub payload: [f64; 3],
    /// [`FUEL`]'s centroid.
    pub fuel: [f64; 3],
}

impl MassCoordinates {
    /// Every component's centroid paired with its canonical name, in the same
    /// order [`MassBreakdown::as_pairs`] uses.
    pub fn as_pairs(&self) -> [(&'static str, [f64; 3]); 10] {
        [
            (WING, self.wing),
            (H_STAB, self.h_stab),
            (V_STAB, self.v_stab),
            (FUSELAGE, self.fuselage),
            (GEAR, self.gear),
            (PROPULSION, self.propulsion),
            (SYSTEMS, self.systems),
            (FURNISHINGS, self.furnishings),
            (PAYLOAD, self.payload),
            (FUEL, self.fuel),
        ]
    }
}

/// The three attributes `run_mass_analysis` reads off upstream's
/// `PayloadLayout` (`alas/physics/payload.py`, ported separately as
/// `alas-payload::payload`) -- see the module doc for why this is a small
/// local type rather than a dependency on that unported crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadLayoutSummary {
    /// The layout's total mass, kg.
    pub total_mass: f64,
    /// The layout's longitudinal centre of gravity, m.
    pub cg_x: f64,
    /// The layout's lateral centre of gravity, m.
    pub cg_y: f64,
}

/// The wing named `name`, or the first wing if none matches -- the
/// `next((w for w in plane.wings if w.name == name), fallback)` pattern
/// `calculate_component_masses` and `define_mass_coordinates` both use to find
/// the main wing.
fn wing_named_or_first<'a>(wings: &'a [Wing], name: &str) -> &'a Wing {
    wings
        .iter()
        .find(|wing| wing.name == name)
        .unwrap_or(&wings[0])
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

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

    // Propulsion mass: dry engine weight + pylons + accessories. Reads the
    // engine's live, editable design thrust (`EngineConfig::thrust_kn`) --
    // kept in sync with the preset registry by `apply_engine_spec`, and
    // directly editable from the Engine Designer tab -- rather than
    // re-looking the engine up by name, so a hand-tuned thrust value is
    // reflected here too.
    let thrust_n = geometry_config.engine.thrust_kn * 1000.0;
    let m_prop = if thrust_n > 0.0 {
        let n_engines = geometry_config.engine.spanwise_positions_m.len() as f64;
        n_engines
            * (thrust_n / (mm.propulsion_twr_factor * requirements.gravity_m_s2))
            * mm.propulsion_installation_factor
    } else {
        mm.propulsion_mass_fallback_fraction * mtow_target
    };

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
    _cabin_config: &CabinConfig,
    control_surfaces: &ControlSurfacesConfig,
    mass_model: Option<&MassModelConfig>,
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

    let mut masses = calculate_component_masses(plane, requirements, geometry_config, mass_model);
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
        coordinates.wing = wing_structural_centroid(wing, requirements, structures)?.xyz_m;
    }
    Ok(coordinates)
}

/// Calculate the global center of gravity location `[X, Y, Z]` in meters --
/// `calculate_physical_cg`.
pub fn calculate_physical_cg(masses: &MassBreakdown, coords: &MassCoordinates) -> [f64; 3] {
    let mut moment = [0.0; 3];
    let mut total_mass = 0.0;
    for ((_, mass), (_, xyz)) in masses.as_pairs().into_iter().zip(coords.as_pairs()) {
        let mass = mass.max(0.0);
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
/// [`FUEL`], so the optimizer's final CG reflects the detailed layout.
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
    let masses = calculate_component_masses_checked(
        plane,
        requirements,
        geometry_config,
        cabin_config,
        control_surfaces,
        mass_model,
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

#[cfg(test)]
#[path = "breakdown_tests.rs"]
mod tests;
