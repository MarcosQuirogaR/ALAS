// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mass_config.py
// Reference: alas @ rust-port-baseline.

//! Mass architecture and parameters of the production and comparison builds.
//!
//! The product default is the single, checked NASA FLOPS transport
//! architecture. The historical Torenbeek/fraction values remain serialized
//! for an explicit reference-compatible comparison and migration, but they
//! do not participate in production evaluation or silently fill missing FLOPS
//! inputs.

use serde::{Deserialize, Serialize};

use crate::{
    legacy_mass_model_schema_version, ConfigNode, FlopsStructureConfig, FlopsTransportConfig,
    FlopsTurbopropConfig, MassArchitecture, MassArchitectureMigration, PropulsionMassMethod,
    StructuralMassMethod, SystemsMassMethod, MASS_MODEL_SCHEMA_VERSION,
};

/// Tunable mass fractions and structural parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields, from = "MassModelConfigWire")]
pub struct MassModelConfig {
    /// Version of this node's own schema; see [`MASS_MODEL_SCHEMA_VERSION`].
    ///
    /// A saved file that predates the single mass architecture states no
    /// version, so it reads back as 1 and
    /// [`MassModelConfig::normalize_architecture`] migrates it.
    #[serde(default = "legacy_mass_model_schema_version")]
    #[config(skip)]
    pub schema_version: u32,

    /// Which method owns every production mass group.
    #[serde(default)]
    #[config(
        advanced,
        options = MassArchitecture,
        label = "Mass architecture",
        help = "The one method that owns every mass group. 'Pure FLOPS transport v1' is the product model: the NASA FLOPS conventional-transport equations own the wing, tails, fuselage, gear, nacelles, propulsion, systems, furnishings and operating items, and a missing input is reported rather than replaced. 'Legacy reference-compatible comparison' is the frozen Torenbeek/fraction buildup, kept only as a comparison and regression control, nothing falls back to it."
    )]
    pub mass_architecture: MassArchitecture,

    /// Method used for systems, equipment, and operating-item mass.
    ///
    /// Derived from [`Self::mass_architecture`]; not independently selectable
    /// and no longer written to saved files. Retained as a field because the
    /// mass buildup and the ledger labels read it per group.
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
        help = "FLOPS technology factors (composites, aeroelastic tailoring, strut bracing, variable sweep), landing-gear and landing-mass overrides, baseline engine scaling, and the empty-mass margin used by the FLOPS structural and propulsion methods."
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
        help = "Declared engine dry mass, propeller geometry and construction, nacelle area density, pylon coefficient and engine-installation mass used by the shaft-power propulsion group. NASA FLOPS parameterises every propulsion mass on rated thrust and has no turboprop branch, so these inputs replace that group for a propeller-driven aircraft."
    )]
    pub flops_turboprop: FlopsTurbopropConfig,

    /// Whether the product analysis places each mass group at its
    /// geometry-derived station.
    #[serde(default = "default_true", skip_serializing_if = "Clone::clone")]
    #[config(
        advanced,
        label = "Geometry-derived component stations",
        help = "Place every mass group at the station the built geometry gives it: the integrated wingbox centroid, the tails at 42 percent of their mean chord, the gear at its nose and main stations, the engines at their nacelles and the fuel in its tanks. Disable to keep the frozen point placement of the reference implementation."
    )]
    pub geometric_component_stations: bool,

    /// Legacy Torenbeek share of maximum takeoff weight the wing structure must carry.
    #[config(
        label = "Wing suspended-mass fraction",
        help = "Legacy comparison only: fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78."
    )]
    pub suspended_mass_fraction: f64,

    /// Legacy Torenbeek design airspeed with the flaps out.
    #[config(
        label = "Max airspeed with flaps extended",
        unit = "m/s",
        help = "Legacy comparison only: design airspeed with flaps extended, fed into the Torenbeek wing-mass formula."
    )]
    pub max_airspeed_for_flaps_ms: f64,

    /// Legacy Torenbeek maximum flap deflection.
    #[config(
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
        help = "How much payload mass occupies one metre of cabin length. Used only to derive the payload/systems CG position (the occupied cabin length), not the payload mass itself, so stretching the fuselage beyond what the payload needs doesn't shift the CG aft 'for free'."
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

    /// Least weight the nose gear needs for steering authority.
    #[config(
        label = "Min nose-gear load fraction",
        help = "Minimum fraction of weight that must be on the nose gear for adequate steering authority: sets the 'Min Nose Load' CG-envelope boundary (the aft-most safe CG at each weight)."
    )]
    pub pct_load_nlg_min: f64,

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
        help = "Fraction of the wing's geometric (Torenbeek) fuel volume that's actually usable tank capacity, after structure, ribs, systems and unusable-fuel allowance. Typical preliminary-design value: 0.85-0.95."
    )]
    pub fuel_tank_usable_fraction: f64,
}

/// Deserialization representation for [`MassModelConfig`].
///
/// The three pre-version-2 selectors are deliberately kept in this wire
/// object even though current files omit them.  A current file carries
/// `mass_architecture`, so the derived selectors can be repaired immediately
/// after decoding; a legacy file omits that field and the selectors remain
/// available to [`MassModelConfig::normalize_architecture`] during the
/// configuration migration path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct MassModelConfigWire {
    #[serde(default = "legacy_mass_model_schema_version")]
    schema_version: u32,
    mass_architecture: Option<MassArchitecture>,
    systems_mass_method: SystemsMassMethod,
    #[serde(default = "FlopsTransportConfig::working_default")]
    flops_transport: FlopsTransportConfig,
    structural_mass_method: StructuralMassMethod,
    propulsion_mass_method: PropulsionMassMethod,
    flops_structure: FlopsStructureConfig,
    flops_turboprop: FlopsTurbopropConfig,
    geometric_component_stations: bool,
    suspended_mass_fraction: f64,
    max_airspeed_for_flaps_ms: f64,
    flap_deflection_angle_deg: f64,
    landing_gear_mass_fraction: f64,
    propulsion_twr_factor: f64,
    propulsion_installation_factor: f64,
    propulsion_mass_fallback_fraction: f64,
    systems_mass_fraction: f64,
    furnishings_mass_fraction: f64,
    cabin_payload_density_kg_m: f64,
    nlg_x_fraction: f64,
    mlg_x_fraction_mac: f64,
    pct_load_nlg_max: f64,
    pct_load_mlg_max: f64,
    pct_load_nlg_min: f64,
    mlw_fraction_mtow: f64,
    fuel_density_kg_m3: f64,
    fuel_tank_usable_fraction: f64,
}

impl Default for MassModelConfigWire {
    fn default() -> Self {
        let defaults = MassModelConfig::default();
        Self {
            schema_version: defaults.schema_version,
            // None lets the conversion distinguish a current architecture
            // field from a legacy file that only has the three old selectors.
            mass_architecture: None,
            systems_mass_method: defaults.systems_mass_method,
            flops_transport: defaults.flops_transport,
            structural_mass_method: defaults.structural_mass_method,
            propulsion_mass_method: defaults.propulsion_mass_method,
            flops_structure: defaults.flops_structure,
            flops_turboprop: defaults.flops_turboprop,
            geometric_component_stations: default_true(),
            suspended_mass_fraction: defaults.suspended_mass_fraction,
            max_airspeed_for_flaps_ms: defaults.max_airspeed_for_flaps_ms,
            flap_deflection_angle_deg: defaults.flap_deflection_angle_deg,
            landing_gear_mass_fraction: defaults.landing_gear_mass_fraction,
            propulsion_twr_factor: defaults.propulsion_twr_factor,
            propulsion_installation_factor: defaults.propulsion_installation_factor,
            propulsion_mass_fallback_fraction: defaults.propulsion_mass_fallback_fraction,
            systems_mass_fraction: defaults.systems_mass_fraction,
            furnishings_mass_fraction: defaults.furnishings_mass_fraction,
            cabin_payload_density_kg_m: defaults.cabin_payload_density_kg_m,
            nlg_x_fraction: defaults.nlg_x_fraction,
            mlg_x_fraction_mac: defaults.mlg_x_fraction_mac,
            pct_load_nlg_max: defaults.pct_load_nlg_max,
            pct_load_mlg_max: defaults.pct_load_mlg_max,
            pct_load_nlg_min: defaults.pct_load_nlg_min,
            mlw_fraction_mtow: defaults.mlw_fraction_mtow,
            fuel_density_kg_m3: defaults.fuel_density_kg_m3,
            fuel_tank_usable_fraction: defaults.fuel_tank_usable_fraction,
        }
    }
}

impl From<MassModelConfigWire> for MassModelConfig {
    fn from(wire: MassModelConfigWire) -> Self {
        let architecture = wire.mass_architecture.unwrap_or_default();
        let has_current_architecture = wire.mass_architecture.is_some();
        let mut model = Self {
            schema_version: wire.schema_version,
            mass_architecture: architecture,
            systems_mass_method: wire.systems_mass_method,
            flops_transport: wire.flops_transport,
            structural_mass_method: wire.structural_mass_method,
            propulsion_mass_method: wire.propulsion_mass_method,
            flops_structure: wire.flops_structure,
            flops_turboprop: wire.flops_turboprop,
            geometric_component_stations: wire.geometric_component_stations,
            suspended_mass_fraction: wire.suspended_mass_fraction,
            max_airspeed_for_flaps_ms: wire.max_airspeed_for_flaps_ms,
            flap_deflection_angle_deg: wire.flap_deflection_angle_deg,
            landing_gear_mass_fraction: wire.landing_gear_mass_fraction,
            propulsion_twr_factor: wire.propulsion_twr_factor,
            propulsion_installation_factor: wire.propulsion_installation_factor,
            propulsion_mass_fallback_fraction: wire.propulsion_mass_fallback_fraction,
            systems_mass_fraction: wire.systems_mass_fraction,
            furnishings_mass_fraction: wire.furnishings_mass_fraction,
            cabin_payload_density_kg_m: wire.cabin_payload_density_kg_m,
            nlg_x_fraction: wire.nlg_x_fraction,
            mlg_x_fraction_mac: wire.mlg_x_fraction_mac,
            pct_load_nlg_max: wire.pct_load_nlg_max,
            pct_load_mlg_max: wire.pct_load_mlg_max,
            pct_load_nlg_min: wire.pct_load_nlg_min,
            mlw_fraction_mtow: wire.mlw_fraction_mtow,
            fuel_density_kg_m3: wire.fuel_density_kg_m3,
            fuel_tank_usable_fraction: wire.fuel_tank_usable_fraction,
        };
        // Current files carry one authoritative architecture.  Re-derive the
        // compatibility fields so direct serde round-trips cannot create a
        // hybrid in memory.  Legacy files intentionally retain their old
        // selectors until `from_value_with_migration` can report the change.
        if has_current_architecture || model.schema_version >= MASS_MODEL_SCHEMA_VERSION {
            model.apply_architecture();
        }
        model
    }
}

const fn default_true() -> bool {
    true
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
            pct_load_nlg_min: 0.02,
            mlw_fraction_mtow: 0.92,
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
