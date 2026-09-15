// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/mass.py
// Reference: alas @ rust-port-baseline.

//! Weight & balance: product mass buildup and centre-of-gravity estimation.
//!
//! [`calculate_flops_mass_buildup`] is the pure product entry point: it
//! evaluates all declared NASA FLOPS transport groups or returns explicit
//! blockers. [`calculate_component_masses_checked`] and the grouped analysis
//! seam dispatch from the authoritative [`MassModelConfig`] architecture;
//! the legacy [`calculate_component_masses`] and [`run_mass_analysis`] names
//! remain frozen compatibility utilities for explicit parity fixtures.
//! [`define_mass_coordinates`] places each centroid;
//! [`calculate_physical_cg`] combines them into a mass-weighted CG; and the
//! checked grouped analysis orchestrates all three with an optional payload
//! layout override.
//!
//! [`WING`] through [`FUEL`] are the ten names upstream's `Dict[str, float]`
//! masses and `Dict[str, List[float]]` coordinates use as keys. Both become a
//! struct with one named field per component: compile-time key safety over a
//! hashmap; while the constants and the `as_pairs` methods give back the
//! name-keyed iteration the dict-shaped callers ([`calculate_physical_cg`],
//! [`OEW_KEYS`]'s summation) need. [`OEW_KEYS`] is the canonical OEW component
//! list upstream's module doc says every other consumer imports rather than
//! redefines, so it is `pub` here too.
//!
//! [`PayloadLayoutSummary`] is the seam to the not-yet-ported
//! `alas/physics/payload.py`; it reproduces the three fields read upstream.

include!("breakdown_parts/part_01.rs");
#[path = "breakdown_parts/part_02.rs"]
mod part_02;
pub use part_02::{
    calculate_component_masses, calculate_component_masses_checked,
    calculate_component_masses_checked_product_with_gear,
    calculate_component_masses_checked_with_gear, calculate_flops_mass_buildup, ProductMassBuildup,
};
