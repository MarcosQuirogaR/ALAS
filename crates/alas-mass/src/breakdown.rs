// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Weight & balance: product mass buildup and centre-of-gravity estimation.
//!
//! [`calculate_flops_mass_buildup`] is the pure product entry point: it
//! evaluates all declared NASA FLOPS transport groups or returns explicit
//! blockers. The grouped analysis seam dispatches from the authoritative
//! [`MassModelConfig`] architecture; [`calculate_component_masses`] and
//! [`run_mass_analysis`] evaluate the Torenbeek comparison architecture
//! directly. [`define_mass_coordinates`] places each centroid;
//! [`calculate_physical_cg`] combines them into a mass-weighted CG; and the
//! checked grouped analysis orchestrates all three with an optional payload
//! layout override.
//!
//! [`WING`] through [`FUEL`] are the ten component names. Masses and
//! coordinates are structs with one named field per component, which gives
//! compile-time key safety; the constants and the `as_pairs` methods give
//! back name-keyed iteration for [`calculate_physical_cg`] and the
//! [`OEW_KEYS`] summation. [`OEW_KEYS`] is the canonical operating-empty
//! component list; every other consumer imports it rather than redefining it.
//!
//! [`PayloadLayoutSummary`] carries the three payload-layout fields the
//! analysis reads: total mass and the lateral and longitudinal CG.

mod components;
mod coordinates;
mod error;
mod flops_methods;
mod types;

pub use components::{
    calculate_component_masses, calculate_component_masses_checked_with_gear,
    calculate_flops_mass_buildup, ProductMassBuildup,
};
pub use coordinates::{
    calculate_physical_cg, define_mass_coordinates, define_mass_coordinates_with_model,
    run_mass_analysis, run_mass_analysis_with_model,
    run_mass_analysis_with_model_checked_product_with_gear,
    run_mass_analysis_with_model_checked_with_gear, run_product_mass_analysis_with_groups,
};
pub use error::ComponentMassError;
pub use flops_methods::FlopsMassBuildup;
pub use types::{MassBreakdown, MassCoordinateModel, MassCoordinates, PayloadLayoutSummary};

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

/// The components that make up the Operating Empty Weight, everything
/// except payload and fuel. This is the single canonical definition; every
/// other module that needs the OEW component set imports it from here rather
/// than redefining its own copy.
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

#[cfg(test)]
mod tests;
