// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure generation families categorized by engineering discipline.
//!
//! Each module constructs backend-neutral [`crate::scene::Scene`] structures
//! representing engineering diagrams, polar charts, flight profiles, or 3D wireframes.

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
