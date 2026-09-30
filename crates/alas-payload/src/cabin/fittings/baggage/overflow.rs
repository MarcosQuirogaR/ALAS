// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Baggage the container positions could not take, spread over the hold
//! compartments instead of one block at the aft end of the cabin.
//!
//! Frames: x is metres aft of the nose tip, masses are kilograms. No mass is
//! ever dropped: whatever the compartment limits cannot absorb is still
//! stowed, in proportion to volume, and reported as overload.

use alas_config::{BaggagePolicy, HoldDeck};

use super::*;
use crate::cargo::{deck_spec, CargoSlot, HoldCompartment, STOWAGE_DENSITY_KG_M3};
use crate::layout::MAIN;

/// Smallest block dimension the shrink-and-retry fit will try, m.
const MIN_BLOCK_DIMENSION_M: f64 = 0.05;
/// Factor applied to a block that fails the envelope check.
const SHRINK_FACTOR: f64 = 0.8;
/// Attempts before the smallest block is stowed unchecked.
const MAX_FIT_ATTEMPTS: usize = 16;
/// Mass below which a share is not worth a block, kg.
const NEGLIGIBLE_KG: f64 = 1.0e-9;

/// What one compartment can still take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Room {
    /// Station of the compartment centre, m.
    pub x: f64,
    /// Volume not occupied by loaded container positions, m3.
    pub residual_volume_m3: f64,
    /// Net mass the compartment can still take, kg.
    pub cap_kg: f64,
}

/// The split of one overflow mass.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Allocation {
    /// Mass per compartment, kg. Sums to the requested mass.
    pub shares_kg: Vec<f64>,
    /// Part of the request above every compartment limit, kg.
    pub overload_kg: f64,
}

/// What was stowed.
pub(super) struct Overflow {
    /// One block per loaded compartment.
    pub items: Vec<DeckItem>,
    /// Overflow mass per compartment, kg, in compartment order.
    pub shares_kg: Vec<f64>,
    /// Mass above every compartment limit, kg.
    pub overload_kg: f64,
}

/// Index of the compartment `x` belongs to: the one containing it, otherwise
/// the one whose centre is nearest.
pub(super) fn compartment_index(compartments: &[HoldCompartment], x: f64) -> Option<usize> {
    compartments
        .iter()
        .position(|compartment| compartment.contains(x))
        .or_else(|| {
            compartments
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    (a.centroid_x_m() - x)
                        .abs()
                        .total_cmp(&(b.centroid_x_m() - x).abs())
                })
                .map(|(index, _)| index)
        })
}

/// Split `total` over positions in proportion to `weights`, none above its
/// `cap`, redistributing what a full position cannot take. Returns the
/// shares; their sum is below `total` only when every cap is reached.
pub(super) fn fill_proportionally(weights: &[f64], caps: &[f64], total: f64) -> Vec<f64> {
    let n = weights.len().min(caps.len());
    let mut shares = vec![0.0; n];
    let mut free: Vec<usize> = (0..n).filter(|&i| caps[i] > NEGLIGIBLE_KG).collect();
    let mut remaining = total.max(0.0);
    while remaining > NEGLIGIBLE_KG && !free.is_empty() {
        let weight_sum: f64 = free.iter().map(|&i| weights[i].max(0.0)).sum();
        let weight_of = |i: usize| {
            if weight_sum > 0.0 {
                weights[i].max(0.0) / weight_sum
            } else {
                1.0 / free.len() as f64
            }
        };
        let full: Vec<usize> = free
            .iter()
            .copied()
            .filter(|&i| remaining * weight_of(i) >= caps[i] - shares[i])
            .collect();
        if full.is_empty() {
            for &i in &free {
                shares[i] += remaining * weight_of(i);
            }
            break;
        }
        for &i in &full {
            let room = caps[i] - shares[i];
            shares[i] += room;
            remaining -= room;
        }
        free.retain(|i| !full.contains(i));
    }
    shares
}

/// Move mass between compartments until its centroid reaches `target_x` or a
/// limit stops it. The pair moved is the most aft loaded compartment and the
/// most forward one with room (or the reverse), which changes the moment
/// fastest per kilogram moved.
fn trim_toward(shares: &mut [f64], rooms: &[Room], target_x: f64) {
    let total: f64 = shares.iter().sum();
    if total <= NEGLIGIBLE_KG || !target_x.is_finite() {
        return;
    }
    for _ in 0..(4 * shares.len() + 4) {
        let moment: f64 = shares.iter().zip(rooms).map(|(m, r)| m * r.x).sum();
        let error = moment - target_x * total;
        if error.abs() <= 1.0e-9 * total.max(1.0) {
            return;
        }
        let toward_forward = error > 0.0;
        let extreme = |candidates: Vec<usize>, aft: bool| {
            candidates.into_iter().max_by(|&a, &b| {
                let order = rooms[a].x.total_cmp(&rooms[b].x);
                if aft {
                    order
                } else {
                    order.reverse()
                }
            })
        };
        let loaded: Vec<usize> = (0..shares.len())
            .filter(|&i| shares[i] > NEGLIGIBLE_KG)
            .collect();
        let with_room: Vec<usize> = (0..shares.len())
            .filter(|&i| rooms[i].cap_kg - shares[i] > NEGLIGIBLE_KG)
            .collect();
        let (Some(from), Some(to)) = (
            extreme(loaded, toward_forward),
            extreme(with_room, !toward_forward),
        ) else {
            return;
        };
        let lever = (rooms[from].x - rooms[to].x) * if toward_forward { 1.0 } else { -1.0 };
        if lever <= 0.0 {
            return;
        }
        let amount = shares[from]
            .min(rooms[to].cap_kg - shares[to])
            .min(error.abs() / lever);
        if amount <= NEGLIGIBLE_KG {
            return;
        }
        shares[from] -= amount;
        shares[to] += amount;
    }
}

/// Split `mass` over the compartments.
///
/// The share within the limits is proportional to residual volume; under
/// [`BaggagePolicy::TargetCg`] with a `target_x` it is then trimmed toward
/// that centroid. Mass above every limit is added in proportion to residual
/// volume and reported as overload, so the shares always sum to `mass`.
pub(super) fn allocate(
    rooms: &[Room],
    mass: f64,
    policy: BaggagePolicy,
    target_x: Option<f64>,
) -> Allocation {
    let mass = mass.max(0.0);
    let weights: Vec<f64> = rooms.iter().map(|r| r.residual_volume_m3).collect();
    let caps: Vec<f64> = rooms.iter().map(|r| r.cap_kg.max(0.0)).collect();
    let capacity: f64 = caps.iter().sum();
    let within = mass.min(capacity);
    let mut shares = fill_proportionally(&weights, &caps, within);
    if policy == BaggagePolicy::TargetCg {
        if let Some(target_x) = target_x {
            let placed: f64 = shares.iter().sum();
            if placed > NEGLIGIBLE_KG {
                trim_toward(&mut shares, rooms, target_x);
            }
        }
    }
    let overload = mass - within;
    if overload > 0.0 && !shares.is_empty() {
        let unlimited = vec![f64::INFINITY; shares.len()];
        for (share, extra) in shares
            .iter_mut()
            .zip(fill_proportionally(&weights, &unlimited, overload))
        {
            *share += extra;
        }
    }
    Allocation {
        shares_kg: shares,
        overload_kg: overload,
    }
}

/// The room each compartment has left after the container positions are
/// loaded.
pub(super) fn rooms(compartments: &[HoldCompartment], slots: &[CargoSlot]) -> Vec<Room> {
    let mut loaded_kg = vec![0.0; compartments.len()];
    let mut occupied_m3 = vec![0.0; compartments.len()];
    for slot in slots
        .iter()
        .filter(|slot| slot.payload > MIN_PLACED_MASS_KG)
    {
        if let Some(index) = compartment_index(compartments, slot.x) {
            loaded_kg[index] += slot.payload;
            let fill = if slot.max_net() > 0.0 {
                (slot.payload / slot.max_net()).clamp(0.0, 1.0)
            } else {
                1.0
            };
            occupied_m3[index] += slot.usable_volume_m3() * fill;
        }
    }
    compartments
        .iter()
        .enumerate()
        .map(|(index, compartment)| {
            let residual_volume = (compartment.volume_m3 - occupied_m3[index]).max(0.0);
            let by_volume = residual_volume * STOWAGE_DENSITY_KG_M3;
            let cap_kg = compartment.max_net_kg.map_or(by_volume, |limit| {
                by_volume.min((limit - loaded_kg[index]).max(0.0))
            });
            Room {
                x: compartment.centroid_x_m(),
                residual_volume_m3: residual_volume,
                cap_kg,
            }
        })
        .collect()
}

/// Load the container positions in proportion to their volume, none above its
/// limit. Used by [`BaggagePolicy::VolumeProportional`].
pub(super) fn fill_slots_by_volume(slots: &mut [CargoSlot], mass: f64) {
    let weights: Vec<f64> = slots.iter().map(CargoSlot::usable_volume_m3).collect();
    let caps: Vec<f64> = slots.iter().map(CargoSlot::max_net).collect();
    for (slot, share) in slots
        .iter_mut()
        .zip(fill_proportionally(&weights, &caps, mass))
    {
        slot.payload = share;
    }
}

/// Stow `leftover` kg over the compartments, one block per loaded compartment
/// at its centre.
///
/// `target_x` is the station the overflow should balance at, when the policy
/// trims toward one.
pub(super) fn place_overflow(
    g: &CabinGeometry,
    compartments: &[HoldCompartment],
    slots: &[CargoSlot],
    leftover: f64,
    policy: BaggagePolicy,
    target_x: Option<f64>,
) -> Overflow {
    let rooms = rooms(compartments, slots);
    let allocation = allocate(&rooms, leftover, policy, target_x);
    let mut shares_kg = allocation.shares_kg;
    merge_small_shares(compartments, &mut shares_kg);
    let items = compartments
        .iter()
        .zip(&shares_kg)
        .filter(|(_, &share)| share > MIN_PLACED_MASS_KG)
        .map(|(compartment, &share)| block(g, compartment, share))
        .collect();
    Overflow {
        items,
        shares_kg,
        overload_kg: allocation.overload_kg,
    }
}

/// Move every share too small for a block into the nearest compartment that
/// keeps a block, so the placed mass equals the requested mass. When no share
/// is large enough, the largest one takes them all.
fn merge_small_shares(compartments: &[HoldCompartment], shares_kg: &mut [f64]) {
    let keeps_block = |share: f64| share > MIN_PLACED_MASS_KG;
    let largest = shares_kg
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index);
    for index in 0..shares_kg.len() {
        let share = shares_kg[index];
        if share <= 0.0 || keeps_block(share) {
            continue;
        }
        let from_x = compartments[index].centroid_x_m();
        let nearest = (0..shares_kg.len())
            .filter(|&other| other != index && keeps_block(shares_kg[other]))
            .min_by(|&a, &b| {
                let da = (compartments[a].centroid_x_m() - from_x).abs();
                let db = (compartments[b].centroid_x_m() - from_x).abs();
                da.total_cmp(&db)
            });
        if let Some(target) = nearest.or(largest.filter(|&big| big != index)) {
            shares_kg[target] += share;
            shares_kg[index] = 0.0;
        }
    }
}

/// The loose block for one compartment's share. A block that does not fit the
/// envelope is shrunk and retried; the mass is stowed regardless.
fn block(g: &CabinGeometry, compartment: &HoldCompartment, mass: f64) -> DeckItem {
    let deck = deck_spec(g, compartment.deck);
    let deck_id = match compartment.deck {
        HoldDeck::Lower => LOWER,
        HoldDeck::Main => MAIN,
    };
    let x = compartment.centroid_x_m();
    let bottom = g.floor_z(deck, x);
    let full_length = compartment.length_m().min(BULK_BLOCK_LEN_M);
    let full_height = g.clamp_height(deck, x, BULK_BLOCK_HEIGHT_M);
    let mut scale = 1.0;
    let mut dimensions = (full_length, 0.0, full_height);
    for attempt in 0..MAX_FIT_ATTEMPTS {
        let length = (full_length * scale).max(MIN_BLOCK_DIMENSION_M);
        let height = (full_height * scale).max(MIN_BLOCK_DIMENSION_M);
        let width = if g.enforces_physical_envelope() {
            [bottom, bottom + height]
                .into_iter()
                .map(|z| g.usable_width_at_z(x, z))
                .fold(f64::INFINITY, f64::min)
                * deck.width_factor
        } else {
            g.usable_width(deck, x)
        };
        let width = (width * scale).max(MIN_BLOCK_DIMENSION_M);
        dimensions = (length, width, height);
        let fits = g
            .check_rectangular_prism(x, length, 0.0, width, bottom, height)
            .is_ok();
        if fits || attempt + 1 == MAX_FIT_ATTEMPTS {
            break;
        }
        scale *= SHRINK_FACTOR;
    }
    let (length, width, height) = dimensions;
    DeckItem {
        kind: ItemKind::Bag,
        deck: deck_id,
        x,
        y: 0.0,
        z: bottom + height * 0.5,
        length,
        width,
        mass,
        height,
        label: format!("{} {} kg", compartment.name, mass.round() as i64),
        meta: ItemMeta::BulkBag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(x: f64, volume: f64, cap: f64) -> Room {
        Room {
            x,
            residual_volume_m3: volume,
            cap_kg: cap,
        }
    }

    fn moment(shares: &[f64], rooms: &[Room]) -> f64 {
        shares.iter().zip(rooms).map(|(m, r)| m * r.x).sum()
    }

    #[test]
    fn an_unconstrained_split_is_proportional_to_residual_volume() {
        let rooms = [room(3.0, 2.0, 1_000.0), room(20.0, 6.0, 1_000.0)];
        let split = allocate(&rooms, 800.0, BaggagePolicy::VolumeProportional, Some(3.0));
        assert!((split.shares_kg[0] - 200.0).abs() < 1e-9);
        assert!((split.shares_kg[1] - 600.0).abs() < 1e-9);
        assert_eq!(split.overload_kg, 0.0);
    }

    #[test]
    fn a_full_compartment_passes_its_excess_to_the_other() {
        let rooms = [room(3.0, 6.0, 100.0), room(20.0, 2.0, 1_000.0)];
        let split = allocate(&rooms, 500.0, BaggagePolicy::VolumeProportional, None);
        assert!((split.shares_kg[0] - 100.0).abs() < 1e-9);
        assert!((split.shares_kg[1] - 400.0).abs() < 1e-9);
    }

    #[test]
    fn the_target_policy_trims_toward_the_target_within_the_limits() {
        let rooms = [room(3.0, 4.0, 300.0), room(23.0, 4.0, 1_000.0)];
        let split = allocate(&rooms, 600.0, BaggagePolicy::TargetCg, Some(18.0));
        let centroid = moment(&split.shares_kg, &rooms) / 600.0;
        assert!((centroid - 18.0).abs() < 1e-9, "{centroid}");
        assert!(split.shares_kg[0] <= 300.0);

        let split = allocate(&rooms, 600.0, BaggagePolicy::TargetCg, Some(3.0));
        assert!((split.shares_kg[0] - 300.0).abs() < 1e-9);
        assert!((split.shares_kg[1] - 300.0).abs() < 1e-9);
    }

    #[test]
    fn mass_above_every_limit_is_kept_and_reported() {
        let rooms = [room(3.0, 1.0, 100.0), room(20.0, 3.0, 300.0)];
        let split = allocate(&rooms, 1_000.0, BaggagePolicy::TargetCg, Some(3.0));
        assert!((split.overload_kg - 600.0).abs() < 1e-9);
        assert!((split.shares_kg.iter().sum::<f64>() - 1_000.0).abs() < 1e-9);
        assert!(split.shares_kg[0] >= 100.0 && split.shares_kg[1] >= 300.0);
    }

    fn compartment(x_start_m: f64, x_end_m: f64) -> HoldCompartment {
        HoldCompartment {
            name: "C".to_owned(),
            x_start_m,
            x_end_m,
            volume_m3: 10.0,
            max_net_kg: None,
            deck: HoldDeck::Lower,
        }
    }

    #[test]
    fn a_share_too_small_for_a_block_joins_the_nearest_placed_block() {
        let holds = [
            compartment(0.0, 2.0),
            compartment(10.0, 12.0),
            compartment(14.0, 16.0),
        ];
        let mut shares = vec![0.4, 50.0, 30.0];
        merge_small_shares(&holds, &mut shares);
        assert_eq!(shares, vec![0.0, 50.4, 30.0]);
        assert!((shares.iter().sum::<f64>() - 80.4).abs() < 1e-12);

        let mut all_small = vec![0.3, 0.5, 0.1];
        merge_small_shares(&holds, &mut all_small);
        assert!((all_small.iter().sum::<f64>() - 0.9).abs() < 1e-12);
        assert_eq!(all_small, vec![0.0, 0.9, 0.0]);
    }

    #[test]
    fn a_request_with_nowhere_to_go_still_sums_to_the_request() {
        let rooms = [room(3.0, 0.0, 0.0), room(20.0, 0.0, 0.0)];
        let split = allocate(&rooms, 250.0, BaggagePolicy::TargetCg, Some(10.0));
        assert!((split.shares_kg.iter().sum::<f64>() - 250.0).abs() < 1e-9);
        assert!((split.overload_kg - 250.0).abs() < 1e-9);
    }
}
