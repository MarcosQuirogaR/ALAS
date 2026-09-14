// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared aircraft geometry model.
//!
//! `alas/geometry/` (this crate's other modules) is this program's own code.
//! The model contains the airfoil, wing, fuselage and mesh data consumed by all
//! disciplines. Its compatibility alias is retained only for older solver and
//! unpublished parity callers.

pub mod airfoil;
pub mod airplane;
pub mod fuselage;
pub mod mesh;
pub mod section_outline;
// The spacing helper has its own endpoint-fixup behavior, so it stays in this
// tree rather than moving to
// `alas-math`, which is documented as holding primitives with no upstream
// module of their own. It is public because `alas-aero::kulfan` reconstructs a
// coordinate airfoil on the same cosine spacing `Airfoil::repanel` uses.
pub mod spacing;
pub mod spanwise;
mod vector3;
pub mod wing;
