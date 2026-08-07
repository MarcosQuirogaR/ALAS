// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field and point performance.
//!
//! [`landing_gear`] sizes the wheel/tire buildup from the static reaction
//! loads at the aerodynamic centre-of-gravity limits, and derives the
//! per-gear strength fractions the centre-of-gravity envelope check clips
//! against.
//!
//! [`performance`] is the closed-form point-performance surface: the
//! matching-chart constraint curves, the FAR-25 V-speed schedule, the field
//! distances, the Breguet range equation and the V-n envelope.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod landing_gear;
pub mod performance;
