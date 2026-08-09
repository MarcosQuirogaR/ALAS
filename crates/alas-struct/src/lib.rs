// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wingbox structural model.
//!
//! [`loads`] (`alas/physics/structural_loads.py`) is the shared spanwise
//! load-integration primitive: one elliptic-lift distribution integrated to
//! shear and bending moment along a cantilever semi-wing (tip to root), the
//! three design load cases that scale it, and the per-engine point loads that
//! relieve it. Strength sizing and the analytical deflection solve land
//! alongside it, in their own modules, as they are ported -- all three sharing
//! this one load model so they can never disagree about it.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod analytical;
pub mod loads;
pub mod sizing;
