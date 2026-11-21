// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:
//   figure_mass_breakdown, figure_landing_gear_planform, and
//   figure_fuel_volume_check.

//! Mass-balance layout figures split by figure family so each source file
//! stays within the repository's review-size limit.

mod cabin_section;
mod common;
mod fuel_volume;
mod landing_gear;
mod landing_gear_tables;
mod mass_breakdown;
#[cfg(test)]
// These tests intentionally panic if their constructed fixture violates its precondition.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
mod tip_back;

pub use cabin_section::figure_cabin_cross_section;
pub use fuel_volume::{figure_fuel_volume_check, figure_fuel_volume_check_for_loading};
pub use landing_gear::{figure_landing_gear_planform, figure_landing_gear_planform_sized};
pub use mass_breakdown::figure_mass_breakdown;
