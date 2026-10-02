// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The order tanks are filled on the ground and emptied in flight.
//!
//! Every layout follows one sequence built from three tank classes:
//!
//! 1. **Trim tanks: emptied first, filled last.** A horizontal-stabilizer
//!    trim tank is a center-of-gravity control tank, not a range reservoir.
//!    On the aircraft that carry one (A330, A340, A380) the fuel control and
//!    monitoring computer moves fuel aft into it to hold a cruise CG target
//!    and transfers it forward into the wing before landing, so the trim tank
//!    is empty on approach. With no zero-fuel mass or zero-fuel CG input the
//!    fill cannot target a takeoff CG (Airbus A380 Flight Deck and Systems
//!    Briefing for Pilots, Issue 2, section 10.10, targets about 39.5 % MAC),
//!    and fuel some 25 m aft of the wing box can only move the CG aft, so the
//!    fill assumes the smallest trim load the arrangement allows. Burning the
//!    trim fuel first is mass-equivalent to transferring it forward into the
//!    feed tanks and burning the same mass there.
//! 2. **Transfer tanks, by declared burn priority.** Center, auxiliary and
//!    wing transfer cells refill the feed tanks; they are emptied in
//!    ascending burn priority and filled in the reverse order.
//! 3. **Feed tanks: filled first, emptied last.** The engines draw only from
//!    the feed tanks ([`TankKind::WingFeed`]), which the transfer tanks keep
//!    topped up, so the feed tanks hold the fuel that remains at landing. All
//!    feed tanks form one group: they fill in proportion to capacity and
//!    drain in proportion to their contents, which keeps them balanced.
//!
//! Source quality. The class order is the fuel-system architecture the
//! tank names in EASA TCDS EASA.A.110 Issue 17, section 3.3 describe (four
//! feed tanks, inner, mid and outer transfer tanks and a trim tank). The
//! resulting ground fill is checked against the only published A380 fuel
//! distribution in the corpus, the example load on the refuel/defuel panel
//! of the Airbus A380 Aircraft Characteristics, Dec 01/25,
//! FIGURE-5-4-6-991-001-A01 (180,800 kg on board: outer tanks full, feed
//! and mid tanks 84-96 % full, inner tanks 24 % full, trim tank 58 % full).
//! The sequence reproduces that load to within 5 % of capacity in the
//! outer, mid and inner tanks (the model fills the inner tanks to 20 %),
//! while it holds the feed tanks full where the panel shows 84-89 %: the
//! difference is the 11,140 kg of CG-targeting trim fuel, which this crate
//! cannot compute and loads last instead. The Airbus FCOM ATA 28 transfer
//! triggers were not
//! retrieved, so the in-flight path (aft trim transfer in the climb,
//! progressive forward transfer in cruise) is not modelled.

use super::types::{FuelTank, TankKind};

/// Sort key of the in-flight burn: trim tanks first, then transfer tanks by
/// burn priority, then every feed tank together.
fn burn_key(tank: &FuelTank) -> (u8, i64) {
    match tank.kind {
        TankKind::Trim => (0, 0),
        TankKind::WingFeed => (2, 0),
        _ => (1, tank.burn_priority),
    }
}

/// Tank indices in ground-fill order, the exact reverse of
/// [`burn_groups`]: feed tanks first, transfer tanks in descending burn
/// priority, trim tanks last. The tanks of one group fill together.
pub(super) fn fill_groups(tanks: &[FuelTank]) -> Vec<Vec<usize>> {
    let mut groups = burn_groups(tanks);
    groups.reverse();
    groups
}

/// Tank indices in burn order, grouped the same way: trim tanks first, then
/// transfer tanks in ascending burn priority, feed tanks last.
pub(super) fn burn_groups(tanks: &[FuelTank]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..tanks.len()).collect();
    order.sort_by_key(|&index| burn_key(&tanks[index]));
    order
        .chunk_by(|&a, &b| burn_key(&tanks[a]) == burn_key(&tanks[b]))
        .map(<[usize]>::to_vec)
        .collect()
}
