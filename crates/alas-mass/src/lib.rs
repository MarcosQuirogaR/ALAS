// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component mass estimation.
//!
//! [`torenbeek`] is the empirical wing and fuselage weight methods
//! [`breakdown`] (`alas/physics/mass.py`) calls into: the component mass
//! buildup, the mass-coordinate scaffold, and the mass-weighted centre of
//! gravity every other physics module reads.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod breakdown;
pub mod torenbeek;
