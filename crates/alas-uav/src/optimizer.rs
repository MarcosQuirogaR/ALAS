// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mixed discrete-component and continuous-geometry UAV design search.
//!
//! The search only ranks designs that pass [`crate::evaluate`] without a
//! failure or evidence gap. Component data therefore cannot become an
//! optimizer default. Propeller performance is supplied as thrust-at-speed
//! operating points; a retail static-thrust value is never used in flight.
//! Geometry follows the preliminary sizing relations documented in
//! `docs/PHYSICS_SOLVER_FLOW.md`, with every empirical or applicability quantity
//! supplied explicitly in [`PreliminaryModel`].

include!("optimizer_parts/part_01.rs");
include!("optimizer_parts/part_02.rs");
