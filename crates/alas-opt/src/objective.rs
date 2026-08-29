// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/objective.py
// Reference: alas @ rust-port-baseline.

//! Scalar objective function for aircraft design space optimization.

#[path = "objective_model.rs"]
mod objective_model;

pub(crate) use objective_model::apply_candidate_payload_load_case;
pub use objective_model::{
    wing_fuel_volume_m3, wing_fuel_volume_m3_reference_compatibility, DesignObjective,
};

#[cfg(test)]
#[path = "objective_tests.rs"]
mod objective_tests;
