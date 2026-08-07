// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! On-design turbofan propulsion cycle analysis.
//!
//! [`cycle`] is a fast, closed-form, on-design conceptual estimate of a
//! separate-flow (unmixed), two-spool turbofan: it walks the engine station by
//! station -- ram, inlet, fan, boosters, high-pressure compressor, combustor,
//! both turbines, both nozzles -- with polytropic component efficiencies and no
//! curve-fit constants, so altitude and Mach sensitivity come from re-evaluating
//! the same equations at a new ambient state rather than from a lookup table.
//! Its component assumptions are the same ones the mission's SUAVE engine model
//! is built with, so the two fidelity levels start from the same physics.

pub mod cycle;
