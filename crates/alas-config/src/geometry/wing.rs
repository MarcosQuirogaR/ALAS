// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`WingConfig`)

//! The parts of the main wing the optimizer is not allowed to move.
//!
//! The design vector owns projected span, derived projected area, sweep, the
//! chords and the section morphing factors. What is left here is everything that decides which
//! *family* of wing those numbers describe: where along the fuselage the root
//! sits, how the defining sections are stacked vertically, how they are
//! twisted, and where the planform cranks. Two runs with different values here
//! are not searching the same design space, so these are fixed for the length
//! of a run and configurable between runs, which is the whole reason they
//! are named fields rather than the constants the original scripts buried
//! inside their geometry builders.

mod airfoil_class;
mod config;
mod planform;
mod shape;
#[cfg(test)]
mod tests;

pub use airfoil_class::AirfoilClass;
pub use config::WingConfig;
pub use planform::{
    InboardAerodynamicStation, MainWingPanel, MainWingStation, MainWingStationKind,
    TransportPlanform, TransportPlanformError, WingSection, WingSectionError,
};
pub use shape::{WingHeights, WingShape, MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION};
