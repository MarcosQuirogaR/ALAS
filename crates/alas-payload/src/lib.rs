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
//! # What is here
//!
//! [`geometry`] samples the built fuselage into one cabin frame that every
//! other part of the interior reads, so that the seating and the hold cannot
//! disagree about where the floor is or how wide it is. [`layout`] is the item
//! and summary vocabulary both layout engines produce. [`cabin`] is the
//! passenger engine (seats, monuments, CS-25 exits and checked baggage)
//! and [`cargo`] is the freighter one, containers and the trim solver that
//! distributes a load across them. [`build`] is the dispatcher every consumer
//! actually calls, together with the fast auto-sizer that turns a class mix
//! into seat counts and the named cabin presets built on it. [`oew`] is the
//! empty-aircraft mass and balance the cargo trim solves against.

pub mod build;
pub mod cabin;
pub mod cargo;
pub mod geometry;
pub mod layout;
pub mod oew;

// Reproduces CPython's and NumPy's own numerics, which decide whole seats and
// whole containers rather than last digits. Private until a second crate needs
// it, at which point it belongs in `alas-math` rather than copied.
mod numeric;

pub use build::{
    apply_cabin_preset, build_payload_layout, build_payload_layout_reference_compatibility,
    simulate_passenger_counts, simulate_passenger_counts_for_seat_mix,
};
pub use build::{CabinPresetError, PassengerCounts};
pub use cabin::{build_passenger_layout, build_passenger_layout_reference_compatibility};
pub use cargo::{
    build_cargo_layout, build_cargo_layout_reference_compatibility, CargoLoadManager, CargoSlot,
    UldType,
};
pub use geometry::{CabinGeometry, CabinGeometryError, DeckSpec};
pub use layout::{DeckItem, ItemKind, ItemMeta, LayoutSummary, Mode, PayloadLayout};
pub use oew::oew_and_cg;
