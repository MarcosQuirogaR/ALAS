// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Adapter from preliminary UAV output to the production geometry and VLM core.
//!
//! The mixed optimizer owns preliminary sizing and its parabolic-polar checks.
//! This module does not replace those checks: it reconstructs the generated
//! aircraft with [`alas_geom`] primitives, independently runs
//! [`alas_aero::vlm`], and reports the disagreement. Airfoil identity and
//! operating point are explicit inputs because neither can be inferred from a
//! retail component catalogue.

use crate::optimizer::{GeneratedGeometry, OptimizedUav, TopologyOptimizedUav};
use crate::topology::{TopologyUnavailableReason, UavAnalysisPath, UavTopology};
use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmResult};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, FuselageXSecError, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{Wing, WingXSec};

mod assessment;
pub use assessment::*;
mod airframe;
use airframe::*;
