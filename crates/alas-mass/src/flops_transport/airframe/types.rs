// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The groups a caller asks the airframe resolver for, and where the
//! quantities it estimated came from.

/// What the caller wants evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlopsAirframeSelection {
    /// Evaluate the structural group.
    pub structure: bool,
    /// Evaluate the propulsion group.
    pub propulsion: bool,
}

/// How the quantities FLOPS estimates when not declared were obtained.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlopsAirframeSources {
    /// `declared` or `mlw_fraction_of_mtow`.
    #[serde(deserialize_with = "alas_config::static_text::text")]
    pub landing_mass: alas_config::static_text::Text,
    /// `declared`, `fuselage_mounted_ground_datum` or `flops_equation_66`.
    #[serde(deserialize_with = "alas_config::static_text::text")]
    pub main_gear_length: alas_config::static_text::Text,
    /// `declared` or `flops_equation_67`.
    #[serde(deserialize_with = "alas_config::static_text::text")]
    pub nose_gear_length: alas_config::static_text::Text,
    /// `declared` or `flops_equation_76`.
    #[serde(deserialize_with = "alas_config::static_text::text")]
    pub baseline_engine_mass: alas_config::static_text::Text,
    /// `declared` (a `flops_structure.design_gross_mass_kg` override, which
    /// is also how a fixed-aircraft closure pins the basis) or
    /// `requirements_mtow`.
    #[serde(deserialize_with = "alas_config::static_text::text")]
    pub design_gross_mass: alas_config::static_text::Text,
    /// `flops_equations_63_64`, or `beltramo_cr151970_military` when the
    /// military fuselage-mounted gear relation replaced them
    /// (`flops_transport::military_gear`).
    #[serde(
        default = "flops_gear_equations",
        deserialize_with = "alas_config::static_text::text"
    )]
    pub gear_mass: alas_config::static_text::Text,
}

/// Gear-mass source of a stored report written before the field existed.
fn flops_gear_equations() -> alas_config::static_text::Text {
    super::super::military_gear::FLOPS_GEAR_SOURCE
}
