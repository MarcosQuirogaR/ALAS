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
    CabinConfig, ControlSurfacesConfig, DesignRequirements, GeometryConfig, LandingGearConfig,
    MassModelConfig, StructuresConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use crate::flops_transport::{
    evaluate_product, FlopsTransportEvaluation, FlopsTransportUnverifiedReason,
    PartialFlopsTransportBreakdown,
};
use crate::torenbeek::{mass_fuselage_simple, mass_wing, mass_wing_with_control_surface_area};
use crate::wing_centroid::WingCentroidError;

mod coordinates;
pub use coordinates::{
    calculate_physical_cg, define_mass_coordinates, define_mass_coordinates_with_model,
    run_mass_analysis, run_mass_analysis_checked, run_mass_analysis_with_model,
    run_mass_analysis_with_model_checked, run_mass_analysis_with_model_checked_product_with_gear,
    run_mass_analysis_with_model_checked_with_gear,
};

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
/// The signed fuel-closure remainder: `MTOW - MZFW`.
///
/// A negative value diagnoses an overweight zero-fuel configuration; it is
/// not a physical negative fuel load.
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
    /// Signed [`FUEL`] closure, `MTOW - MZFW`, in kg.
    ///
    /// Negative values are retained so sizing and optimization callers can
    /// diagnose an overweight candidate. Use
    /// [`Self::physical_fuel_mass_kg`] before treating this value as a load or
    /// forming a mass moment.
    pub fuel: f64,
}

impl MassBreakdown {
    /// Signed MTOW-closure remainder, in kg.
    ///
    /// This diagnostic preserves negative closure for callers that must
    /// detect `MZFW > MTOW`; it does not assert that the value is a physical
    /// fuel load.
    pub fn signed_fuel_closure_kg(&self) -> f64 {
        self.fuel
    }

    /// Physically admissible fuel load, in kg.
    ///
    /// A finite, nonnegative closure is a usable mass value. Negative and
    /// non-finite closures return `None` so they cannot silently become a
    /// negative fuel mass or moment while remaining available through
    /// [`Self::signed_fuel_closure_kg`] for diagnostics.
    pub fn physical_fuel_mass_kg(&self) -> Option<f64> {
        (self.fuel.is_finite() && self.fuel >= 0.0).then_some(self.fuel)
    }

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

/// Planform area occupied by the configured trailing-edge flap run.
///
/// `Wing` intentionally carries no control-surface subgeometry, so deriving
/// this at the mass boundary keeps drawing inputs and structural mass inputs
/// consistent without changing the geometry/parity type. Chord is integrated
/// over the configured semi-span interval rather than approximated from the
/// total wing area, which handles tapered and multi-station wings.
fn configured_flap_area(wing: &Wing, control_surfaces: &ControlSurfacesConfig) -> f64 {
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
fn main_gear_mounted_to_wing(landing_gear: &LandingGearConfig) -> bool {
    landing_gear.n_mlg_struts == 0 || landing_gear.n_mlg_struts >= 2
}

/// Product mass buildup with the configured flap geometry and gear layout.
///
/// The public `calculate_component_masses` function remains the frozen
/// reference-compatible entry point. Checked product analyses call this seam
/// so the user-selected control surfaces and landing-gear architecture are
/// reflected in the physical wing mass.
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
#[path = "breakdown_tests.rs"]
mod tests;
