// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled feasibility checks for a fixed-wing electric UAV.
//!
//! The aerodynamic equations are the steady point-mass relations in John D.
//! Anderson, *Aircraft Performance and Design*, McGraw-Hill, 1999: lift is
//! `q S C_L`, and the preliminary drag polar is `C_D0 + k C_L^2`. Standard
//! gravity is the exact conventional value from the BIPM SI Brochure, 9th ed.
//! Component ratings are not aerodynamic models: published static thrust is
//! never treated as thrust available in flight. Every flight point therefore
//! carries thrust from an independently evaluated propeller operating point.

use crate::catalog::{EscSpec, MotorSpec};
pub use crate::model::*;
use crate::packaging::{check_packaging, collect_mass_terms, total_mass};

mod checks;
pub use checks::*;
mod flight;
pub(crate) use flight::*;
