// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mass_config.py

//! Mass architecture and parameters of the production and comparison builds.
//!
//! The product default is the single, checked NASA FLOPS transport architecture.
//! The historical Torenbeek/fraction values stay serialized for an explicit
//! comparison and migration; they never enter production evaluation or fill
//! missing FLOPS inputs.

use serde::{Deserialize, Serialize};

use crate::{
    ConfigNode, FlopsStructureConfig, FlopsTransportConfig, FlopsTurbopropConfig, MassArchitecture,
    MassArchitectureMigration, PropulsionMassMethod, StructuralMassMethod, SystemsMassMethod,
    MASS_MODEL_SCHEMA_VERSION,
};

/// Tunable mass fractions and structural parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields, from = "MassModelConfigWire")]
pub struct MassModelConfig {
    /// Version of this node's own schema; see [`MASS_MODEL_SCHEMA_VERSION`].
    ///
    /// A file that predates the single mass architecture states none, reads back
    /// as 1 and is migrated by [`MassModelConfig::normalize_architecture`].
    #[serde(default = "legacy_mass_model_schema_version")]
    #[config(skip)]
    pub schema_version: u32,

    /// Which method owns every production mass group.
    #[serde(default)]
    #[config(
        advanced,
        options = MassArchitecture,
        label = "Mass architecture",
        help = "The method that owns every mass group.",
        long_help = "The one method that owns every mass group. 'Pure FLOPS transport v1' is the product model: the NASA FLOPS conventional-transport equations own the wing, tails, fuselage, gear, nacelles, propulsion, systems, furnishings and operating items, and a missing input is reported rather than replaced. 'Legacy reference-compatible comparison' is the frozen Torenbeek/fraction buildup, kept only as a comparison and regression control, nothing falls back to it."
    )]
    pub mass_architecture: MassArchitecture,

    /// Method used for systems, equipment, and operating-item mass.
    ///
    /// Derived from [`Self::mass_architecture`], not selectable and not written to
    /// files; kept as a field because the buildup and ledger labels read it.
    #[serde(default, skip_serializing)]
    #[config(skip)]
    pub systems_mass_method: SystemsMassMethod,

    /// Physical architecture required by the FLOPS transport method.
    #[serde(default, skip_serializing_if = "FlopsTransportConfig::is_unspecified")]
    #[config(
        nested,
        advanced,
        help = "Declared range, crew, cabin, hydraulic, engine-mounting, and fuel-system inputs required by FLOPS transport mass correlations."
    )]
    pub flops_transport: FlopsTransportConfig,

    /// Method used for the wing, tail, fuselage and landing-gear mass.
    ///
    /// Derived from [`Self::mass_architecture`]; see
    /// [`Self::systems_mass_method`].
    #[serde(default, skip_serializing)]
    #[config(skip)]
    pub structural_mass_method: StructuralMassMethod,

    /// Method used for the installed propulsion mass.
    ///
    /// Derived from [`Self::mass_architecture`]; see
    /// [`Self::systems_mass_method`].
    #[serde(default, skip_serializing)]
    #[config(skip)]
    pub propulsion_mass_method: PropulsionMassMethod,

    /// Technology factors and overrides for the FLOPS airframe equations.
    #[serde(default, skip_serializing_if = "FlopsStructureConfig::is_default")]
    #[config(
        nested,
        advanced,
        help = "FLOPS technology factors (composites, aeroelastic tailoring, strut bracing, variable sweep), gear and landing-mass overrides, engine scaling, empty-mass margin.",
        long_help = "FLOPS technology factors (composites, aeroelastic tailoring, strut bracing, variable sweep), landing-gear and landing-mass overrides, baseline engine scaling, and the empty-mass margin used by the FLOPS structural and propulsion methods."
    )]
    pub flops_structure: FlopsStructureConfig,

    /// Declared inputs of the shaft-power propulsion group.
    ///
    /// Read only when the installed engine is a turboprop; the FLOPS source
    /// has no propeller, gearbox or shaft-power mass equation, so a turboprop
    /// propulsion group is evaluated from this node instead of from equations
    /// 69 and 75-92.
    #[serde(default, skip_serializing_if = "FlopsTurbopropConfig::is_default")]
    #[config(
        nested,
        advanced,
        help = "Declared engine dry mass, propeller geometry and construction, nacelle area density, pylon coefficient and installation mass for the shaft-power group.",
        long_help = "Declared engine dry mass, propeller geometry and construction, nacelle area density, pylon coefficient and engine-installation mass used by the shaft-power propulsion group. NASA FLOPS parameterises every propulsion mass on rated thrust and has no turboprop branch, so these inputs replace that group for a propeller-driven aircraft."
    )]
    pub flops_turboprop: FlopsTurbopropConfig,

    /// Whether other product mass groups use geometric stations. The wing always
    /// uses its primary-box and secondary first moments, including when disabled.
    #[serde(default = "default_true", skip_serializing_if = "Clone::clone")]
    #[config(
        advanced,
        label = "Geometry-derived component stations",
        help = "Place mass groups at built geometry stations (tails at 42 % mean chord, gear, engines at nacelles, fuel in tanks).",
        long_help = "Place mass groups at the built geometry stations: tails at 42 percent of their mean chord, gear at its nose and main stations, engines at their nacelles and fuel in its tanks. Disable to keep the other groups at the reference points. The complete wing always uses the sized box plus secondary first moment, shared with the item ledger."
    )]
    pub geometric_component_stations: bool,

    /// Legacy Torenbeek share of maximum takeoff weight the wing structure must carry.
    #[config(
        advanced,
        label = "Wing suspended-mass fraction",
        help = "Legacy comparison only: fraction of MTOW treated as suspended mass in the Torenbeek wing structural formula [0-1]. Typical commercial transport 0.70-0.78.",
        long_help = "Legacy comparison only: fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78."
    )]
    pub suspended_mass_fraction: f64,

    /// Legacy Torenbeek design airspeed with the flaps out.
    #[config(
        advanced,
        label = "Max airspeed with flaps extended",
        unit = "m/s",
        help = "Legacy comparison only: design airspeed with flaps extended, fed into the Torenbeek wing-mass formula."
    )]
    pub max_airspeed_for_flaps_ms: f64,

    /// Legacy Torenbeek maximum flap deflection.
    #[config(
        advanced,
        label = "Max take-off flap deflection",
        unit = "deg",
        help = "Legacy comparison only: maximum flap deflection angle, fed into the Torenbeek wing-mass formula."
    )]
    pub flap_deflection_angle_deg: f64,

    /// Legacy landing gear mass as a share of maximum takeoff weight.
    #[config(
        label = "Landing-gear mass fraction",
        help = "Legacy comparison only: landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports."
    )]
    pub landing_gear_mass_fraction: f64,

    /// Legacy engine thrust-to-weight ratio used to size dry engine mass.
    #[config(
        label = "Engine thrust-to-weight factor",
        help = "Legacy comparison only: dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight ratios are ~5-7, so this factor is typically ~6."
    )]
    pub propulsion_twr_factor: f64,

    /// Legacy multiplier on dry engine mass for everything installed around it.
    #[config(
        label = "Propulsion installation overhead",
        help = "Legacy comparison only: multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories."
    )]
    pub propulsion_installation_factor: f64,

    /// Legacy propulsion mass fraction used when the engine is not in the database.
    #[config(
        label = "Propulsion mass fallback fraction",
        help = "Legacy comparison only: fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database."
    )]
    pub propulsion_mass_fallback_fraction: f64,

    /// Legacy systems and equipment share of maximum takeoff weight.
    #[config(
        label = "Systems & equipment mass fraction",
        help = "Legacy comparison only: avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports."
    )]
    pub systems_mass_fraction: f64,

    /// Legacy furnishings and operational items share of maximum takeoff
    /// weight.
    #[config(
        label = "Furnishings & operations mass fraction",
        help = "Legacy comparison only: passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports."
    )]
    pub furnishings_mass_fraction: f64,

    /// How much payload mass one metre of cabin holds.
    #[config(
        label = "Payload linear density",
        unit = "kg/m",
        help = "Payload mass per metre of occupied cabin length [kg/m]. Used only to place the payload/systems CG, not the payload mass.",
        long_help = "How much payload mass occupies one metre of cabin length. Used only to derive the payload/systems CG position (the occupied cabin length), not the payload mass itself, so stretching the fuselage beyond what the payload needs doesn't shift the CG aft 'for free'."
    )]
    pub cabin_payload_density_kg_m: f64,

    /// Where the nose gear sits along the fuselage.
    #[config(
        label = "Nose-gear X position",
        unit = "fraction of fuselage length",
        help = "Nose landing gear longitudinal position, as a fraction of total fuselage length from the nose."
    )]
    pub nlg_x_fraction: f64,

    /// Where the main gear sits along the mean aerodynamic chord.
    #[config(
        label = "Main-gear X position",
        unit = "fraction of MAC aft of MAC LE",
        help = "Main landing gear longitudinal position, as a fraction of the mean aerodynamic chord aft of the MAC leading edge."
    )]
    pub mlg_x_fraction_mac: f64,

    /// Most weight the nose gear is rated to carry.
    #[config(
        label = "Max nose-gear load fraction",
        help = "Maximum fraction of total aircraft weight the nose gear is rated to carry: sets the 'NLG Max Strength' CG-envelope boundary."
    )]
    pub pct_load_nlg_max: f64,

    /// Most weight the main gear is rated to carry.
    #[config(
        label = "Max main-gear load fraction",
        help = "Maximum fraction of total aircraft weight the main gear is rated to carry: sets the 'MLG Max Strength' CG-envelope boundary."
    )]
    pub pct_load_mlg_max: f64,

    /// Least weight the nose gear carries at the most-aft CG.
    #[config(
        label = "Min nose-gear load fraction",
        help = "Minimum fraction of weight on the nose gear at the most-aft CG (Min Nose Load CG boundary). Class value 0.0352 from 13 airliner families.",
        long_help = "Minimum fraction of weight that must be on the nose gear: sets the 'Min Nose Load' CG-envelope boundary (the aft-most CG at each weight, the main-gear station less this share of the wheelbase), the same for every aircraft. The class value, 3.52 %, is the lowest static nose-gear share at a certified most-aft CG in a sample of 13 airliner families read from the manufacturers' airport-planning documents (Boeing 717, 737NG, 737 MAX, 757, 767, 777, MD-80, MD-90, MD-11; Airbus A300-600, A310, A330; Embraer E190): the MD-90-30 at 157,000 lb. The nine wing-engine twins alone give 3.77 % (737-800 at 156,000 lb) and the median of the family minima is 4.99 %. No business jet is in the sample: none publishes the table. Raymer (6th ed., Sec. 11.2, p. 343) gives 5 % as the least nose-wheel load with enough traction to steer, a design guideline for placing the main gear and not a certification limit; see pct_load_nlg_max_handling for the pair's other end."
    )]
    pub pct_load_nlg_min: f64,

    /// Most weight the nose gear should carry for handling.
    #[config(
        label = "Max nose-gear handling-load fraction",
        help = "Maximum fraction of weight on the nose gear at the forward CG limit. Raymer 6th ed. Sec. 11.2: above 0.20 the main gear is too far aft.",
        long_help = "Maximum fraction of current weight that may sit on the nose gear at the forward CG limit, independent of the nose tires' rated capacity. Raymer (6th ed., Sec. 11.2) gives both nose-load bounds as one pair: over 20% of the weight on the nosewheel means the main gear is probably too far aft relative to the c.g., under 5% leaves too little traction to steer, and 8-15% is optimum (p. 343); M_f/B below 0.20 at the forward c.g. and M_a/B above 0.05 at the aft c.g., 0.15 and 0.08 preferred (Sec. 11.3, p. 345). pct_load_nlg_min, the aft end of the same pair, is a separate class value taken from certified practice (see its help)."
    )]
    pub pct_load_nlg_max_handling: f64,

    /// Maximum landing weight as a share of maximum takeoff weight.
    #[config(
        label = "Max landing weight fraction of MTOW",
        help = "Maximum Landing Weight (MLW) as a fraction of MTOW, shown as a reference line on the CG envelope."
    )]
    pub mlw_fraction_mtow: f64,

    /// Density used to turn tank volume into a fuel mass.
    #[config(
        label = "Fuel density",
        unit = "kg/m^3",
        help = "Jet-A/Jet-A1 density at 15C (~804 kg/m^3). Converts wing tank volume to a fuel-mass capacity for the payload-range diagram and the wing fuel-volume check."
    )]
    pub fuel_density_kg_m3: f64,

    /// Share of the wing's geometric volume that is usable tankage.
    #[config(
        label = "Usable fuel-tank volume fraction",
        help = "Fraction of the wing's geometric (Torenbeek) fuel volume that is usable tank capacity, after structure, ribs, systems and unusable fuel [0-1].",
        long_help = "Fraction of the wing's geometric (Torenbeek) fuel volume that's actually usable tank capacity, after structure, ribs, systems and unusable-fuel allowance. Typical preliminary-design value: 0.85-0.95."
    )]
    pub fuel_tank_usable_fraction: f64,
}

mod wire;
use wire::MassModelConfigWire;

const fn default_true() -> bool {
    true
}

/// Design ceiling of the static nose-gear share at the most-forward CG: 15 % (Jenkinson, Simpkin and Rhodes, Civil Jet
/// Aircraft Design, 1999, ch. 3, p. 47: "not more than 15% W static load on the nose-wheel"; Raymer 6th ed., Sec. 11.3,
/// p. 345: `M_f/B` below 0.20, 0.15 preferred). 0.20, Raymer's outer bound, stood here; the four published forward
/// plateaus of the registered types sit at 10.7 to 13.2 %, none near 20 %.
const fn default_pct_load_nlg_max_handling() -> f64 {
    0.15
}

impl MassModelConfig {
    /// Whether every group uses its frozen reference-compatible method, so
    /// the buildup needs none of the declared FLOPS architecture.
    ///
    /// This is the comparison architecture, not a product state.
    pub fn uses_reference_mass_methods(&self) -> bool {
        !self.mass_architecture.is_pure_flops()
    }

    /// Force the three group selectors to agree with [`Self::mass_architecture`].
    ///
    /// The selectors are derived, but they are still ordinary struct fields a
    /// caller can set directly, so the buildup asks for them here rather than
    /// trusting that nobody did.
    pub fn apply_architecture(&mut self) {
        self.systems_mass_method = self.mass_architecture.systems_method();
        self.structural_mass_method = self.mass_architecture.structural_method();
        self.propulsion_mass_method = self.mass_architecture.propulsion_method();
    }

    /// Whether the three group selectors agree with the declared architecture.
    ///
    /// A production analysis must never run on a `false` here: it would mean
    /// some group is being evaluated by a method the configuration does not
    /// claim, and the ledger's own labels would be wrong.
    pub fn architecture_is_coherent(&self) -> bool {
        self.systems_mass_method == self.mass_architecture.systems_method()
            && self.structural_mass_method == self.mass_architecture.structural_method()
            && self.propulsion_mass_method == self.mass_architecture.propulsion_method()
    }

    /// Bring a loaded configuration up to [`MASS_MODEL_SCHEMA_VERSION`] and
    /// report what that did to its mass method.
    ///
    /// A version-1 file carried three independent selectors. Reading them
    /// back and re-deriving an architecture is the only way to honour what
    /// the file actually asked for; a hybrid selection names no architecture
    /// and is migrated to pure FLOPS rather than silently reconstructed as
    /// one of its halves. The returned record is the thing a user interface
    /// or an export shows; this migration changes operating empty mass and
    /// must not be invisible.
    pub fn normalize_architecture(&mut self) -> MassArchitectureMigration {
        let migration = if self.schema_version >= MASS_MODEL_SCHEMA_VERSION {
            MassArchitectureMigration::None
        } else {
            let systems_was_flops = !self.systems_mass_method.is_reference_compatible();
            let structure_was_flops = !self.structural_mass_method.is_reference_compatible();
            let propulsion_was_flops = !self.propulsion_mass_method.is_reference_compatible();
            match MassArchitecture::from_group_selection(
                self.systems_mass_method,
                self.structural_mass_method,
                self.propulsion_mass_method,
            ) {
                Some(MassArchitecture::PureFlopsTransportV1) => {
                    MassArchitectureMigration::LegacyPureFlopsPreserved
                }
                Some(MassArchitecture::LegacyReferenceCompatibleComparison) => {
                    MassArchitectureMigration::LegacyDefaultsMovedToPureFlops
                }
                None => MassArchitectureMigration::LegacyHybridMigratedToPureFlops {
                    systems_was_flops,
                    structure_was_flops,
                    propulsion_was_flops,
                },
            }
        };
        if migration != MassArchitectureMigration::None {
            // Every version-1 selection lands on the production architecture.
            // The legacy buildup stays reachable, but only by asking for it
            // by name in a version-2 file, so an old default cannot quietly
            // keep a run on a method the product no longer publishes.
            self.mass_architecture = MassArchitecture::PureFlopsTransportV1;
        }
        self.schema_version = MASS_MODEL_SCHEMA_VERSION;
        self.apply_architecture();
        migration
    }
}

impl Default for MassModelConfig {
    fn default() -> Self {
        let mass_architecture = MassArchitecture::default();
        Self {
            schema_version: MASS_MODEL_SCHEMA_VERSION,
            mass_architecture,
            systems_mass_method: mass_architecture.systems_method(),
            // A product default must be runnable end to end under pure FLOPS.
            // The transport type's own `Default` remains the empty contract
            // so strict callers can detect missing declarations; this working
            // scenario is explicit user-declared input with uncertainty.
            flops_transport: FlopsTransportConfig::working_default(),
            structural_mass_method: mass_architecture.structural_method(),
            propulsion_mass_method: mass_architecture.propulsion_method(),
            flops_structure: FlopsStructureConfig::default(),
            flops_turboprop: FlopsTurbopropConfig::default(),
            geometric_component_stations: true,
            suspended_mass_fraction: 0.75,
            max_airspeed_for_flaps_ms: 90.0,
            flap_deflection_angle_deg: 40.0,
            landing_gear_mass_fraction: 0.04,
            propulsion_twr_factor: 6.0,
            propulsion_installation_factor: 1.30,
            propulsion_mass_fallback_fraction: 0.07,
            systems_mass_fraction: 0.11,
            furnishings_mass_fraction: 0.10,
            cabin_payload_density_kg_m: 800.0,
            nlg_x_fraction: 0.10,
            mlg_x_fraction_mac: 0.50,
            pct_load_nlg_max: 0.10,
            pct_load_mlg_max: 0.93,
            // The limit is a gate every certified design has to pass, so it is the lowest share certified practice
            // shows, not a central value: 3.52 % (field help). The design floors of the handbooks are higher (5 %:
            // Raymer 6th ed., p. 343; Sadraey 2013, pp. 503-507; 8 %: Jenkinson 1999, p. 47) and certified aircraft go
            // below them at their most-aft CG: 747-400 3.6 %, 787-9 4.3 %, 737-800 3.77 %, 777-300 3.79 %, A330 4.11 %.
            // By configuration in the 13-family sample: nine wing-engine twins 3.77 to 7.35 % (median 5.06 %), three
            // rear-engined types 3.52 to 4.39 %, the MD-11 5.67 %; no quad, no turboprop.
            pct_load_nlg_min: 0.0352,
            pct_load_nlg_max_handling: default_pct_load_nlg_max_handling(),
            mlw_fraction_mtow: crate::landing_mass_ratio::working_default_mlw_fraction_mtow(),
            fuel_density_kg_m3: 804.0,
            fuel_tank_usable_fraction: 0.85,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gear_load_bounds_leave_a_usable_cg_envelope() {
        // The three gear-load fractions are the envelope's boundaries, and an
        // ordering mistake among them describes an envelope with no interior:
        // every CG position would violate something.
        let config = MassModelConfig::default();
        assert!(config.pct_load_nlg_min < config.pct_load_nlg_max);
        assert!(config.pct_load_nlg_max + config.pct_load_mlg_max > 1.0);
    }

    #[test]
    fn the_handling_ceiling_is_a_separate_and_larger_bound_than_the_steering_floor() {
        // pct_load_nlg_max is a tire-capacity limit that an
        // auto-sized layout always clears; pct_load_nlg_max_handling is the
        // separate handling/steering ceiling this ordering exists to check.
        let config = MassModelConfig::default();
        assert!(config.pct_load_nlg_min < config.pct_load_nlg_max_handling);
        assert!((config.pct_load_nlg_max_handling - 0.15).abs() < 1e-12);
        assert!((config.pct_load_nlg_min - 0.0352).abs() < 1e-12);
    }

    #[test]
    fn the_mass_fractions_of_mtow_leave_room_for_structure_fuel_and_payload() {
        // Landing gear, systems and furnishings are each a share of the same
        // takeoff weight, and the wing, fuselage, engines, fuel and payload
        // have to fit in what is left.
        let config = MassModelConfig::default();
        let accounted = config.landing_gear_mass_fraction
            + config.systems_mass_fraction
            + config.furnishings_mass_fraction
            + config.propulsion_mass_fallback_fraction;
        assert!(
            accounted < 0.5,
            "the fixed fractions already take {accounted}"
        );
    }

    #[test]
    fn a_landing_weight_does_not_exceed_a_takeoff_weight() {
        assert!(MassModelConfig::default().mlw_fraction_mtow <= 1.0);
    }

    #[test]
    fn a_unit_the_field_name_cannot_express_is_stated_explicitly() {
        // `_kg_m3` is not one of the recognized suffixes and `_ms` is not
        // `_m_s`, so both of these would derive nothing without the explicit
        // unit, and a density shown without one is a number nobody can
        // check.
        let schema = MassModelConfig::default().schema();
        assert_eq!(schema.field("fuel_density_kg_m3").unwrap().unit, "kg/m^3");
        assert_eq!(
            schema.field("max_airspeed_for_flaps_ms").unwrap().unit,
            "m/s"
        );
    }
}
