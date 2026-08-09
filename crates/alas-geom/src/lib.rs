// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shapes every disciplinary analysis measures: airfoil sections, wings,
//! fuselages, and the builder that turns a design vector and a geometry
//! scaffold into one aircraft.
//!
//! This crate is where a configuration stops being numbers and becomes a
//! geometry. Nothing above it re-derives a planform: the vortex-lattice
//! methods panel the wings this crate returns, the mass estimates measure its
//! fuselage, and the structural mesh is cut from its wingbox. That is the
//! point of collecting it here rather than letting each discipline build the
//! aircraft it happens to need -- two disciplines that each build their own
//! wing agree until the day one of them is edited.
//!
//! # What lives here, and what does not
//!
//! Two provenances share the crate. `alas/geometry/` is this program's own
//! code -- the airfoil library, the parametric shaping, the builder, the
//! wingbox. The `asb` module is translated from AeroSandbox's geometry model,
//! whose `Airfoil`, `Wing` and `Fuselage` the reference uses as its data
//! structures rather than defining its own; a translation that replaced them
//! with something better shaped would have nothing left to compare against.
//!
//! The structural *mesh* is not here. It belongs to `alas-struct`, because it
//! exists to be handed to a finite-element solver and its cards are that
//! solver's vocabulary; `docs/PORTING.md` carries `wing_mesh_bdf.py`
//! accordingly.
//!
//! # The airfoil corpus
//!
//! `data/selig.txt` is 1,665 coordinate sets from the UIUC database, which the
//! reference reaches through a zip archive. Reasoning about why it is text
//! here, and what a reader comparing the two implementations should expect, is
//! in `docs/PORTING.md` under Geometry.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod airfoil_data;
pub mod airfoil_library;
pub mod asb;
pub mod builder;
pub mod selig;
pub mod wing_structure;
