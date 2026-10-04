// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence-aware production verdict for generated fixed-wing UAVs: the
//! coupled calculations behind it.
//!
//! Positive pitching moment is nose-up and positive lift coefficient is
//! upward, so longitudinal static stability requires `dCm/dCL < 0`: an
//! increase in lift must create a restoring nose-down moment. The production
//! geometry has no elevator primitive. Pitch trim is therefore claimed only
//! for an explicitly evidenced trimmable-horizontal-tail incidence range.

use crate::catalog::{Catalog, ComponentKind};
use crate::feasibility::{evaluate, Finding, FindingKind, Severity, UavReport};
use crate::optimizer::OptimizedUav;
use crate::shared_core::{
    assess_generated_geometry_with_shared_core, SharedCoreAssessment, SharedCoreFailure,
    SharedCoreInputs,
};
use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmResult};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;

mod verdict;
pub use verdict::*;
mod trim_support;
use trim_support::*;
