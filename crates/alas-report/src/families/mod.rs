// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure generation families categorized by engineering discipline.
//!
//! Each module constructs backend-neutral [`crate::scene::Scene`] structures
//! representing engineering diagrams, polar charts, flight profiles, or 3D wireframes.

/// What a gear figure says instead of drawing a station nothing measured.
///
/// Raised when `alas_pipeline::gear_stations::resolved_gear_stations` refuses
/// the wing-mounted main-gear fallback for a layout outside its domain: the
/// aircraft registers no source gear-station anchor and its wing root sits
/// above the fuselage crown, so there is no wing-root gear bay the rule could
/// place legs in. This is a missing datum, closed by registering the
/// aircraft's published gear stations, and the figure must not stand in a
/// number for it.
pub(crate) const MAIN_GEAR_STATION_NOT_MEASURED: &str =
    "No main-gear station measured for this layout (no published gear-station anchor; wing root above fuselage crown)";

pub mod aerodynamics;
pub mod geometry;
pub mod mass_balance;
pub mod mass_balance_layout;
pub mod mission;
pub mod optimization;
pub mod performance;
pub mod propulsion;
pub mod screening;
pub mod stability;
pub mod structures;
pub mod structures_dynamics;
