// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Translations from AeroSandbox's geometry model.
//!
//! `alas/geometry/` (this crate's other modules) is this program's own code.
//! Everything under `asb` instead ports the pieces of AeroSandbox's own
//! `Airfoil`, `Wing`, `Fuselage` and mesh classes that the reference actually
//! calls -- see `crates/alas-geom/src/lib.rs`'s module doc for why the two
//! provenances share a crate, and each submodule's own doc for what it is
//! scoped to and why.

pub mod airfoil;
pub mod airplane;
pub mod fuselage;
mod spacing;
mod vector3;
pub mod wing;
