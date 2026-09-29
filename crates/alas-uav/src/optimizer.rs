// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mixed discrete-component and continuous-geometry UAV design search.
//!
//! The search only ranks designs that pass [`crate::evaluate`] without a
//! failure or evidence gap. Component data therefore cannot become an
//! optimizer default. Propeller performance is supplied as thrust-at-speed
//! operating points; a retail static-thrust value is never used in flight.
//! Geometry follows preliminary sizing relations, with every empirical or applicability quantity
//! supplied explicitly in [`PreliminaryModel`].

mod generation;
mod topology;
mod validation;
use crate::catalog::{Catalog, Dimensions};
use crate::{Finding, FindingKind, UavDesign, UavReport};
pub use generation::{
    EmpennageGeometry, FuselageGeometry, GeneratedGeometry, LandingGearGeometry, WingGeometry,
};
pub use topology::{
    optimize_for_topology, optimize_for_topology_with_control, TopologyOptimizationError,
    TopologyOptimizedUav,
};
use validation::validate_problem;

mod problem;
pub use problem::*;
mod search;
pub use search::*;
