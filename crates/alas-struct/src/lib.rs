// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wingbox structural model.
//!
//! [`loads`] (`alas/physics/structural_loads.py`) is the shared spanwise
//! load-integration primitive: one elliptic-lift distribution integrated to
//! shear and bending moment along a cantilever semi-wing (tip to root), the
//! three design load cases that scale it, and the per-engine point loads that
//! relieve it. Strength sizing and the analytical deflection solve land
//! alongside it, in their own modules, as they are ported, all three sharing
//! this one load model so they can never disagree about it.
//!
//! [`mesh`] and [`nastran`] are the finite-element half: the first builds the
//! NASTRAN deck for the sized wingbox, the second writes the solution decks
//! that solve it and drives the solver. They share the same load model, which
//! is what stops the deck's `FORCE` cards and the analytical estimate
//! disagreeing about what the wing is carrying. [`op2`] reads the binary result
//! file a solve produces back in, natively, since there is no Rust pyNastran to
//! read it through. [`nastran95`] is the same finite-element half for the
//! open-source 1995 solver: it reformulates the mesh into that dialect's fixed
//! eight-column deck and drives the built solver, and is validated by solving
//! the same wingbox through both it and a modern solver and holding the two to
//! converge.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod analytical;
pub mod loads;
pub mod mesh;
pub mod nastran;
pub mod nastran95;
pub mod op2;
pub mod sizing;
