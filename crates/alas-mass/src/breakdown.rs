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
//! [`run_mass_analysis`] orchestrates all three, with an optional detailed
//! payload-layout override.
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
//! `alas/physics/payload.py`: upstream's `run_mass_analysis` takes an optional
//! `PayloadLayout` and reads only `.total_mass`/`.cg_x`/`.cg_y` off it, so this
//! reproduces that duck-typed usage concretely rather than depending on the
//! unported crate. When `alas-payload::payload` lands its layout type can
//! convert into this one.

use alas_config::{DesignRequirements, GeometryConfig, MassModelConfig};
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::Wing;

use crate::torenbeek::{mass_fuselage_simple, mass_wing};

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

/// Every AeroSandbox `Wing::aerodynamic_center` call in this module reads the
/// quarter-chord point -- upstream's `mass.py` never passes a
/// `chord_fraction` of its own, and AeroSandbox's own default is 0.25.
const AERODYNAMIC_CENTER_CHORD_FRACTION: f64 = 0.25;

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
/// `payload_layout`, when given with a positive `total_mass`, replaces the
/// lumped [`PAYLOAD`] mass/coordinate with the layout's true mass and centre
/// of gravity, and recomputes the [`FUEL`] remainder. Every caller (the
/// optimizer loop included) calls this function twice per evaluation: once
/// without `payload_layout` to get a cheap OEW/x_oew estimate, then again
/// with the detailed layout built from that estimate, so the CG this function
/// returns always reflects the real cabin/cargo layout rather than the lumped
/// `cabin_payload_density_kg_m` estimate, which only ever seeds that first
/// pass.
pub fn run_mass_analysis(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry_config: &GeometryConfig,
    mass_model: Option<&MassModelConfig>,
    payload_layout: Option<&PayloadLayoutSummary>,
) -> (MassBreakdown, MassCoordinates, [f64; 3]) {
    let mut masses = calculate_component_masses(plane, requirements, geometry_config, mass_model);
    let mut coords =
        define_mass_coordinates(plane, geometry_config, Some(requirements), mass_model);

    if let Some(layout) = payload_layout {
        if layout.total_mass > 0.0 {
            let m_oew: f64 = OEW_KEYS
                .iter()
                .map(|&key| masses.get(key).unwrap_or(0.0))
                .sum();
            masses.payload = layout.total_mass;
            masses.fuel = requirements.mtow_kg - (m_oew + masses.payload);
            let z_payload = coords.payload[2];
            coords.payload = [layout.cg_x, layout.cg_y, z_payload];
        }
    }

    let cg = calculate_physical_cg(&masses, &coords);
    (masses, coords, cg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::asb::airfoil::Airfoil;
    use alas_geom::asb::fuselage::{Fuselage, FuselageXSec};
    use alas_geom::asb::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn simple_wing(name: &str) -> Wing {
        Wing::new(
            name,
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca2412")),
                WingXSec::new([1.0, 15.0, 0.0], 1.0, 0.0, naca("naca2412")),
            ],
            true,
        )
    }

    fn simple_fuselage(name: &str, x0: f64, x1: f64) -> Fuselage {
        Fuselage::new(
            name,
            vec![
                FuselageXSec::new([x0, 0.0, 0.0], Some(1.0), None, None, 2.0)
                    .expect("radius alone is valid"),
                FuselageXSec::new([x1, 0.0, 0.0], Some(0.5), None, None, 2.0)
                    .expect("radius alone is valid"),
            ],
        )
    }

    fn all_zero_breakdown() -> MassBreakdown {
        MassBreakdown {
            wing: 0.0,
            h_stab: 0.0,
            v_stab: 0.0,
            fuselage: 0.0,
            gear: 0.0,
            propulsion: 0.0,
            systems: 0.0,
            furnishings: 0.0,
            payload: 0.0,
            fuel: 0.0,
        }
    }

    fn all_zero_coordinates() -> MassCoordinates {
        MassCoordinates {
            wing: [1.0, 2.0, 3.0],
            h_stab: [4.0, 5.0, 6.0],
            v_stab: [7.0, 8.0, 9.0],
            fuselage: [10.0, 11.0, 12.0],
            gear: [13.0, 14.0, 15.0],
            propulsion: [16.0, 17.0, 18.0],
            systems: [19.0, 20.0, 21.0],
            furnishings: [22.0, 23.0, 24.0],
            payload: [25.0, 26.0, 27.0],
            fuel: [28.0, 29.0, 30.0],
        }
    }

    #[test]
    fn an_all_zero_mass_input_returns_the_origin_rather_than_dividing_by_zero() {
        let cg = calculate_physical_cg(&all_zero_breakdown(), &all_zero_coordinates());
        assert_eq!(cg, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_negative_mass_is_clamped_to_zero_rather_than_pulling_the_cg_the_wrong_way() {
        let mut masses = all_zero_breakdown();
        masses.wing = -1000.0;
        masses.fuselage = 100.0;
        let coords = all_zero_coordinates();
        let cg = calculate_physical_cg(&masses, &coords);
        // Only the fuselage mass (positive) should contribute; the negative
        // wing mass is dropped rather than subtracted.
        assert_eq!(cg, coords.fuselage);
    }

    #[test]
    fn the_cg_of_one_component_is_that_components_own_coordinate() {
        let mut masses = all_zero_breakdown();
        masses.gear = 500.0;
        let coords = all_zero_coordinates();
        let cg = calculate_physical_cg(&masses, &coords);
        assert_eq!(cg, coords.gear);
    }

    #[test]
    fn get_resolves_every_canonical_name_and_nothing_else() {
        let masses = MassBreakdown {
            wing: 1.0,
            h_stab: 2.0,
            v_stab: 3.0,
            fuselage: 4.0,
            gear: 5.0,
            propulsion: 6.0,
            systems: 7.0,
            furnishings: 8.0,
            payload: 9.0,
            fuel: 10.0,
        };
        assert_eq!(masses.get(WING), Some(1.0));
        assert_eq!(masses.get(FUEL), Some(10.0));
        assert_eq!(masses.get("Not a component"), None);
    }

    #[test]
    fn oew_keys_excludes_exactly_payload_and_fuel() {
        assert!(!OEW_KEYS.contains(&PAYLOAD));
        assert!(!OEW_KEYS.contains(&FUEL));
        assert_eq!(OEW_KEYS.len(), 8);
    }

    fn plane_with_fuselages(fuselages: Vec<Fuselage>) -> Airplane {
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![simple_wing("Main Wing")],
            fuselages,
            s_ref: 100.0,
            c_ref: 5.0,
            b_ref: 30.0,
        }
    }

    #[test]
    fn nacelle_fuselages_overwrite_the_propulsion_coordinate_with_their_mean_position() {
        let geometry = GeometryConfig::default();
        let plane = plane_with_fuselages(vec![
            simple_fuselage("Fuselage", 0.0, 76.72),
            simple_fuselage("Nacelle L", 10.0, 18.0).translate([0.0, -9.8, -2.0]),
            simple_fuselage("Nacelle R", 10.0, 18.0).translate([0.0, 9.8, -2.0]),
        ]);
        let coords = define_mass_coordinates(&plane, &geometry, None, None);
        // Mean X of the two nacelles' (start + half length): both at
        // x_start=10, length=8, so midpoint 14 for each -- mean is 14.
        assert!((coords.propulsion[0] - 14.0).abs() < 1e-9);
        assert!((coords.propulsion[1] - 0.0).abs() < 1e-9); // symmetric L/R
        assert!((coords.propulsion[2] - (-2.0)).abs() < 1e-9);
    }

    #[test]
    fn no_nacelle_fuselages_leaves_the_wing_relative_propulsion_coordinate() {
        let geometry = GeometryConfig::default();
        let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);
        let coords = define_mass_coordinates(&plane, &geometry, None, None);
        let wing = &plane.wings[0];
        let w_ac = wing.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION);
        let w_root_z = wing.xsecs[0].xyz_le[2];
        assert_eq!(coords.propulsion, [w_ac[0], 0.0, w_root_z - 1.0]);
    }

    #[test]
    fn a_positive_payload_layout_replaces_the_lumped_payload_and_recomputes_fuel() {
        let geometry = GeometryConfig::default();
        let requirements = DesignRequirements::default();
        let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);

        let (baseline_masses, _, _) =
            run_mass_analysis(&plane, &requirements, &geometry, None, None);

        let layout = PayloadLayoutSummary {
            total_mass: 40_000.0,
            cg_x: 33.0,
            cg_y: 0.5,
        };
        let (masses, coords, _) =
            run_mass_analysis(&plane, &requirements, &geometry, None, Some(&layout));

        assert_eq!(masses.payload, 40_000.0);
        assert_eq!(coords.payload[0], 33.0);
        assert_eq!(coords.payload[1], 0.5);

        let m_oew: f64 = OEW_KEYS
            .iter()
            .map(|&key| baseline_masses.get(key).unwrap_or(0.0))
            .sum();
        let expected_fuel = requirements.mtow_kg - (m_oew + 40_000.0);
        assert!((masses.fuel - expected_fuel).abs() < 1e-9);
    }

    #[test]
    fn a_zero_mass_payload_layout_is_ignored_like_the_python_falsy_check() {
        let geometry = GeometryConfig::default();
        let requirements = DesignRequirements::default();
        let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);

        let (without, _, _) = run_mass_analysis(&plane, &requirements, &geometry, None, None);
        let layout = PayloadLayoutSummary {
            total_mass: 0.0,
            cg_x: 99.0,
            cg_y: 99.0,
        };
        let (with_zero_layout, _, _) =
            run_mass_analysis(&plane, &requirements, &geometry, None, Some(&layout));
        assert_eq!(without, with_zero_layout);
    }
}
