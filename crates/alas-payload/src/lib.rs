// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The detailed interior: what the aircraft actually carries, where each piece
//! of it sits, and the centre of gravity that follows.
//!
//! The lumped payload model in `alas-mass::breakdown` spreads payload along
//! the cabin at a constant density, which is enough to size an aircraft and
//! not enough to check one. A real seating distribution and a real load plan
//! put the payload centre of gravity several percent MAC away from where that
//! model puts it, and several percent MAC is the difference between a design
//! inside its envelope and one outside it. So this runs on *every* candidate
//! the optimizer evaluates, not only on the final design, and its answer is
//! the one every caller keeps.
//!
//! # What is here so far
//!
//! [`geometry`] samples the built fuselage into one cabin frame that every
//! other part of the interior reads, so that the seating and the hold cannot
//! disagree about where the floor is or how wide it is. [`layout`] is the item
//! and summary vocabulary both layout engines produce. [`oew`] is the
//! empty-aircraft mass and balance the cargo trim solves against.
//!
//! The two layout engines themselves -- the passenger cabin
//! (`alas/physics/cabin_layout.py`) and the ULD cargo loader
//! (`alas/physics/cargo_loader.py`) -- and the dispatcher and preset sizing
//! that sit on top of them are not translated yet. `docs/PORTING.md` carries
//! the row. `golden/payload/layout.json` already records what the reference
//! produces for all three, including the full item sequence of every case, so
//! the engines land against a fixture rather than against a description.

pub mod geometry;
pub mod layout;
pub mod oew;

// Reproduces CPython's and NumPy's own numerics, which decide whole seats and
// whole containers rather than last digits. Private until a second crate needs
// it, at which point it belongs in `alas-math` rather than copied.
#[allow(dead_code)]
mod numeric;

pub use geometry::{CabinGeometry, CabinGeometryError, DeckSpec};
pub use layout::{DeckItem, ItemKind, ItemMeta, LayoutSummary, Mode, PayloadLayout};
pub use oew::oew_and_cg;
