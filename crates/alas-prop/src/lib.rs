// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! On-design turbofan propulsion cycle analysis.
//!
//! [`cycle`] is a fast, closed-form, on-design conceptual estimate of a
//! separate-flow (unmixed), two-spool turbofan: it walks the engine station by
//! station: ram, inlet, fan, boosters, high-pressure compressor, combustor,
//! both turbines, both nozzles, with polytropic component efficiencies and no
//! curve-fit constants, so altitude and Mach sensitivity come from re-evaluating
//! the same equations at a new ambient state rather than from a lookup table.
//! Its component assumptions are the same ones the mission engine model is
//! built with, so the two fidelity levels start from the same physics.
//!
//! [`mission_turbofan`] is that mission engine model itself: an independent
//! station-based turbofan network, sized at one flight condition by
//! `turbofan_sizing`. It is a *second*, structurally different turbofan cycle
//! reached only through the mission runner, and is kept on its own terms rather
//! than unified with [`cycle`]. Its module doc records why, and the
//! single-flight-condition scope it is held to.

pub mod cycle;
pub mod empirical_turbofan;
pub mod mission_turbofan;
pub mod product_turbofan;
pub mod system;
pub mod turbofan_physics;
pub mod turboprop;
pub mod turboprop_cycle;
