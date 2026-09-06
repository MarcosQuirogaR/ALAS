// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel-tank arrangement resolved on the built geometry.
//!
//! [`crate::fuel_plan`] and [`crate::dispatch`] answer how much fuel a
//! mission needs; this module answers where that fuel physically sits.
//! [`FuelTankLayout::resolve`] turns a configured tank arrangement
//! (`alas_config::FuelTankLayoutConfig`) into integral wing tanks bounded by
//! the spar box and two semispan stations, a wing carry-through centre tank,
//! a horizontal-stabiliser trim tank and a declared-volume fuselage
//! auxiliary tank -- each with its own capacity, centroid, extent and burn
//! order on the aircraft actually built, not on a number typed in.
//! [`FuelTankLayout::distribute`] turns one fuel load into a per-tank fill
//! following that burn order, and [`FuelState`] is that fill: the ledger
//! items every mass property downstream reads, and the burn that consumes
//! it in flight.
//!
//! Every geometric estimate is kept beside a manufacturer's published
//! capacity rather than in place of it. [`FuelTankLayout::resolve`] applies
//! the calibration that reconciles a redesigned wing's geometric estimate
//! with a registered aircraft's published total, so nothing downstream has
//! to reapply it; [`FuelTankLayout::resolve_scaled`] carries a registered
//! arrangement's published cells onto a redesigned wing as per-cell
//! factors, so a candidate's capacity follows its own spar box.

mod distribute;
mod geometry;
mod resolve;
mod scaled;
mod types;

#[cfg(test)]
mod tests;

pub use distribute::FuelState;
pub use types::{
    CapacitySource, FuelCgPoint, FuelTank, FuelTankLayout, TankKind, TankLayoutError, TankSide,
};
