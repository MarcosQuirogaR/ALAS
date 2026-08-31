// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/objective.py
// Reference: alas @ rust-port-baseline.

//! Preliminary model-derived center-of-gravity and landing-gear constraints.
//!
//! Product assessment keeps the hard longitudinal-stability floor separate
//! from the optimizer's preferred static margin. It also exposes each gear
//! reaction constraint independently, because a single envelope Boolean
//! cannot identify whether stability, tire capacity, or steering authority
//! governs a loading state. The ground reactions follow two-point static
//! equilibrium as presented by Currey, *Aircraft Landing Gear Design:
//! Principles and Practices*, AIAA, 1988.

include!("envelope_parts/part_01.rs");
include!("envelope_parts/part_02.rs");
