// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Deserialization wire object of the mass model and its conversion.

use serde::Deserialize;

use super::{default_pct_load_nlg_max_handling, default_true, MassModelConfig};
use crate::{
    legacy_mass_model_schema_version, FlopsStructureConfig, FlopsTransportConfig,
    FlopsTurbopropConfig, MassArchitecture, PropulsionMassMethod, StructuralMassMethod,
    SystemsMassMethod, MASS_MODEL_SCHEMA_VERSION,
};

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
pub(super) struct MassModelConfigWire {
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
    #[serde(default = "default_pct_load_nlg_max_handling")]
    pct_load_nlg_max_handling: f64,
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
            pct_load_nlg_max_handling: defaults.pct_load_nlg_max_handling,
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
            pct_load_nlg_max_handling: wire.pct_load_nlg_max_handling,
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
