// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native Athena Vortex Lattice geometry decks and total-force parsing.
//!
//! AVL's coefficients are defined by the references in the geometry header
//! and by the axes selected by the solver. Keeping those values beside every
//! parsed polar prevents an unlabelled coefficient vector from being compared
//! with a differently normalized aircraft model.
//!
//! Reference: M. Drela and H. Youngren, *AVL 3.40 User Primer*, geometry
//! input and OPER total-forces sections, 22 February 2022.

use alas_geom::aircraft::airplane::Airplane;
use std::fmt::Write as FmtWrite;
mod formats;
pub use formats::*;

mod deck;
pub use deck::*;
mod helpers;
use helpers::*;
#[cfg(test)]
mod tests;
