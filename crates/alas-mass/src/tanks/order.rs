// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The order tanks are filled on the ground and emptied in flight.
//!
//! # Trim tanks: filled last, emptied first
//!
//! A horizontal-stabiliser trim tank is a centre-of-gravity control tank, not
//! a range reservoir. On the aircraft that carry one (A330, A340, A380) the
//! fuel control and monitoring computer moves fuel aft into it to hold a
//! cruise CG target and transfers it forward into the wing before landing,
//! so the trim tank is empty on approach whatever its position in the main
//! transfer sequence. Every fill and burn in this crate therefore treats a
//! [`TankKind::Trim`] tank as filled after every other tank and emptied
//! before every other tank, independently of its declared burn priority:
//!
//! - filled last: with no zero-fuel mass or zero-fuel CG input, the fill
//!   cannot target a takeoff CG (Airbus A380 Flight Deck and Systems Briefing
//!   for Pilots, Issue 2, section 10.10, targets about 39.5 % MAC), and fuel
//!   in a tank some 25 m aft of the wing box can only move the CG aft, so the
//!   fill assumes the smallest trim load the tank arrangement allows;
//! - emptied first: burning the trim fuel first is mass-equivalent to
//!   transferring it forward into the feed tanks and burning the same mass
//!   there, and it leaves the trim tank empty at every landing state the
//!   ledger evaluates.
//!
//! Source quality: secondary. The practice is recorded in this repository's
//! A340-300 tank arrangement (`alas_config::preset_fuel_tanks`) and in the
//! A380 briefing above; the Airbus FCOM ATA 28 fuel chapters that define the
//! transfer triggers were not retrieved, so the in-flight path between
//! takeoff and landing (aft transfer in the climb, progressive forward
//! transfer in cruise) is not modelled and intermediate burn states place
//! the trim fuel further forward than the aircraft does. The declared burn
//! priorities still order every other tank.

use super::types::{FuelTank, TankKind};

/// Sort key of the in-flight burn: trim tanks first, then burn priority.
fn burn_key(tank: &FuelTank) -> (bool, i64) {
    (tank.kind != TankKind::Trim, tank.burn_priority)
}

/// Tank indices in ground-fill order, the exact reverse of
/// [`burn_groups`]: non-trim tanks in descending burn priority, trim tanks
/// last. The tanks of one group (mirrored pairs sharing a burn priority)
/// fill together.
pub(super) fn fill_groups(tanks: &[FuelTank]) -> Vec<Vec<usize>> {
    let mut groups = burn_groups(tanks);
    groups.reverse();
    groups
}

/// Tank indices in burn order, grouped the same way: trim tanks first, then
/// non-trim tanks in ascending burn priority.
pub(super) fn burn_groups(tanks: &[FuelTank]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..tanks.len()).collect();
    order.sort_by_key(|&index| burn_key(&tanks[index]));
    order
        .chunk_by(|&a, &b| burn_key(&tanks[a]) == burn_key(&tanks[b]))
        .map(<[usize]>::to_vec)
        .collect()
}
