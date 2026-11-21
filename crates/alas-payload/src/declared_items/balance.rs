// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Payload auto-balance: sliding the movable declared items to a centre of
//! gravity.
//!
//! A declared zone or point mass marked `movable` carries a station range it
//! may occupy. The balance moves every movable item by the same shift, held to
//! each item's own range, so that the zero-fuel centre of gravity (operating
//! empty plus payload, the state the cargo loader already targets) reaches the
//! target. It is the payload counterpart of the trim-tank transfer: a bounded
//! redistribution of mass that costs nothing but position.
//!
//! The target is the plan's `balance_target_pct_mac`, else the cargo deck's
//! `target_cg_pct_mac`, else the cargo loader's own default. A plan that sets
//! `lock_layout` is never balanced. When the ranges cannot reach the target the
//! items sit at the limit of their ranges and the outcome says so.
//!
//! Only payload mass is balanced: a movable point mass must be payload, and a
//! movable zone must carry no furnishing mass (its occupants are the payload),
//! because furnishing is part of the operating empty mass at its own station.

use alas_config::{AlasConfig, MassCategory, PayloadItemsConfig};

use crate::geometry::CabinGeometry;
use crate::layout::{mass_properties, PayloadLayout};

use super::{point_mass_item, zone_items};

/// What the balance did.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct BalanceOutcome {
    /// Shift applied to each movable item, m (positive aft), by id or zone name.
    pub shifts_m: Vec<(String, f64)>,
    /// The zero-fuel centre of gravity the balance aimed at, m.
    pub target_x_m: f64,
    /// The zero-fuel centre of gravity reached, m.
    pub reached_x_m: f64,
    /// Whether the ranges stopped the items short of the target.
    pub saturated: bool,
    /// Whether the passes settled on the target (within [`SETTLED_M`]) when the
    /// ranges did not stop them. A balance that is neither saturated nor
    /// settled ran out of passes.
    pub settled: bool,
}

/// Distance of the reached zero-fuel centre of gravity from the target, m,
/// below which the balance has settled.
pub const SETTLED_M: f64 = 1.0e-3;

/// One movable item: its mass, how far it may move each way, and how it moves.
struct Mover {
    id: String,
    mass_kg: f64,
    range_m: [f64; 2],
    zone: Option<usize>,
    point: Option<usize>,
}

/// The target station, m, of the balance for `config`.
fn target_x_m(g: &CabinGeometry, config: &AlasConfig) -> f64 {
    let pct = config
        .mission
        .plan
        .balance_target_pct_mac
        .or_else(|| {
            (config.cabin.cargo.target_cg_pct_mac > 0.0)
                .then_some(config.cabin.cargo.target_cg_pct_mac)
        })
        .unwrap_or(crate::cargo::DEFAULT_TARGET_PCT_MAC);
    g.pct_mac_to_x(pct)
}

fn movers(declared: &PayloadItemsConfig, g: &CabinGeometry, occupant_kg: f64) -> Vec<Mover> {
    let mut movers = Vec::new();
    for (index, zone) in declared.zones.iter().enumerate() {
        let Some([low, high]) = zone.x_range_m.filter(|_| zone.movable) else {
            continue;
        };
        if zone.furnishing_kg_per_m2 > 0.0 || zone.total_mass_kg.is_some_and(|m| m > 0.0) {
            continue;
        }
        let mass: f64 = zone_items(g, zone, occupant_kg)
            .iter()
            .map(|item| item.mass)
            .sum();
        if mass > 0.0 {
            movers.push(Mover {
                id: zone.name.clone(),
                mass_kg: mass,
                range_m: [low - zone.x_start_m, high - zone.x_end_m],
                zone: Some(index),
                point: None,
            });
        }
    }
    for (index, point) in declared.point_masses.iter().enumerate() {
        let Some([low, high]) = point.x_range_m.filter(|_| point.movable) else {
            continue;
        };
        if point.category == MassCategory::Payload {
            movers.push(Mover {
                id: point.id.clone(),
                mass_kg: point.mass_kg,
                range_m: [low - point.x_m, high - point.x_m],
                zone: None,
                point: Some(index),
            });
        }
    }
    movers
}

fn shifted(declared: &PayloadItemsConfig, movers: &[Mover], shifts: &[f64]) -> PayloadItemsConfig {
    let mut items = declared.clone();
    for (mover, shift) in movers.iter().zip(shifts) {
        if let Some(index) = mover.zone {
            items.zones[index].x_start_m += shift;
            items.zones[index].x_end_m += shift;
        }
        if let Some(index) = mover.point {
            items.point_masses[index].x_m += shift;
        }
    }
    items
}

/// The zero-fuel centre of gravity, m, with `declared` placed on `base`.
fn zero_fuel_cg_x_m(
    base: &PayloadLayout,
    declared: &PayloadItemsConfig,
    g: &CabinGeometry,
    occupant_kg: f64,
    oew_kg: f64,
    x_oew_m: f64,
) -> f64 {
    let mut items = base.items.clone();
    for zone in &declared.zones {
        items.extend(zone_items(g, zone, occupant_kg));
    }
    for point in &declared.point_masses {
        items.push(point_mass_item(g, point));
    }
    let (mass, cg_x, _) = mass_properties(&items);
    (oew_kg * x_oew_m + mass * cg_x) / (oew_kg + mass)
}

/// The shifts, m, one per mover, that move the payload moment by `moment`,
/// kg m: a common shift held to each range, found by bisection (the moment is
/// non-decreasing in the shift). Returns the shifts and whether a range
/// stopped them.
fn solve_shifts(movers: &[Mover], moment: f64) -> (Vec<f64>, bool) {
    let moment_at = |shift: f64| -> f64 {
        movers
            .iter()
            .map(|mover| mover.mass_kg * shift.clamp(mover.range_m[0], mover.range_m[1]))
            .sum()
    };
    let low = movers
        .iter()
        .map(|m| m.range_m[0])
        .fold(f64::INFINITY, f64::min);
    let high = movers
        .iter()
        .map(|m| m.range_m[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let (shift, saturated) = if moment <= moment_at(low) {
        (low, moment < moment_at(low) - 1.0e-6)
    } else if moment >= moment_at(high) {
        (high, moment > moment_at(high) + 1.0e-6)
    } else {
        let (mut a, mut b) = (low, high);
        for _ in 0..80 {
            let middle = 0.5 * (a + b);
            if moment_at(middle) < moment {
                a = middle;
            } else {
                b = middle;
            }
        }
        (0.5 * (a + b), false)
    };
    (
        movers
            .iter()
            .map(|mover| shift.clamp(mover.range_m[0], mover.range_m[1]))
            .collect(),
        saturated,
    )
}

/// The declared items of `config` with the movable ones balanced on top of
/// `base` (the layout before the declared items), or `None` when there is
/// nothing to balance: no movable item, a locked layout, or no payload.
pub(super) fn balanced(
    base: &PayloadLayout,
    g: &CabinGeometry,
    config: &AlasConfig,
    oew_kg: f64,
    x_oew_m: f64,
    occupant_kg: f64,
) -> Option<(PayloadItemsConfig, BalanceOutcome)> {
    if config.mission.plan.lock_layout || oew_kg.is_nan() || oew_kg <= 0.0 {
        return None;
    }
    let declared = &config.cabin.payload_items;
    let movers = movers(declared, g, occupant_kg);
    if movers.is_empty() {
        return None;
    }
    let target = target_x_m(g, config);
    let mut shifts = vec![0.0; movers.len()];
    let mut saturated = false;
    // A shifted zone changes the width it integrates over, so the centroid
    // does not move exactly by the shift: a few passes settle it.
    for _ in 0..4 {
        let trial = shifted(declared, &movers, &shifts);
        let reached = zero_fuel_cg_x_m(base, &trial, g, occupant_kg, oew_kg, x_oew_m);
        let mass = {
            let mut items = base.items.clone();
            for zone in &trial.zones {
                items.extend(zone_items(g, zone, occupant_kg));
            }
            for point in &trial.point_masses {
                items.push(point_mass_item(g, point));
            }
            mass_properties(&items).0
        };
        let error_moment = (target - reached) * (oew_kg + mass);
        if error_moment.abs() < 1.0e-3 {
            break;
        }
        let current: f64 = movers.iter().zip(&shifts).map(|(m, s)| m.mass_kg * s).sum();
        let (next, stopped) = solve_shifts(&movers, current + error_moment);
        saturated = stopped;
        if next
            .iter()
            .zip(&shifts)
            .all(|(a, b)| (a - b).abs() < 1.0e-9)
        {
            break;
        }
        shifts = next;
    }
    let items = shifted(declared, &movers, &shifts);
    let reached = zero_fuel_cg_x_m(base, &items, g, occupant_kg, oew_kg, x_oew_m);
    let outcome = BalanceOutcome {
        shifts_m: movers.iter().map(|m| m.id.clone()).zip(shifts).collect(),
        target_x_m: target,
        reached_x_m: reached,
        saturated,
        settled: (target - reached).abs() < SETTLED_M,
    };
    Some((items, outcome))
}
