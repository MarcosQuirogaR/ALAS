// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled DC-motor and propeller torque balance.

use super::{apc_performance_map, ElectricPropulsionError, PropellerPerformanceMap};
use crate::catalog::ComponentKind;
use crate::optimizer::{PropulsionMap, PropulsionOperatingPoint};
use crate::Catalog;

mod model;
pub use model::*;
mod resolve;
use resolve::*;
