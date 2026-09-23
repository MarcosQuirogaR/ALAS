// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning a resolved [`FuelTankLayout`] into a fuel load, and a fuel load
//! into ledger items.
//!
//! Loading uses tank capacities and arrangement. The A380 arrangement has an
//! approximate feed-containing-cell and outer-cell rule; other layouts fill tanks burned
//! last first, with trim last. Burning follows burn priorities independently.
//!
//! # The fill order is an assumption, not a source
//!
//! Only the *burn* order is sourced for every registered aircraft. Exact
//! refuelling sequences were not retrieved in the internal fuel-tank study
//! (2026-09-05, sections 9.4 and 10). The general reverse-burn fill and
//! trim-last exception are therefore modelling assumptions. The A380
//! exception uses inner and mid model cells that each contain feed and
//! nonfeed tank volume (EASA.A.110 Issue 17, section 3.3) and the outer-cell half-capacity preference (Airbus Training
//! Center A380 ATA 28, p.0012). Airbus Flight Deck and Systems Briefing
//! Issue 2, section 10.10 describes automatic ground refuelling chosen from
//! zero-fuel weight and CG for a target takeoff CG near 39.5% MAC. This API
//! receives neither input and cannot target that CG or reproduce FQMS. The
//! approximation supplies the feed-containing model groups at positive
//! partial loads. It cannot establish fuel in each real feed tank, a
//! certified dispatch minimum, or dispatchability.
//!
//! [`crate::wing_reconciliation::declared_wing_fuel_case`] faces the same
//! missing document on the structural side and takes the other branch: it keeps
//! the declared spanwise shape, invents no sequence, and records a bracket. The
//! two are not coupled - the wingbox sizing never reads `burn_priority` - so
//! this rule reaches only the balance:
//! [`crate::product_stations::analyzed_fuel_centroid`], the feasibility
//! mass-balance states, and [`FuelTankLayout::fuel_cg_curve`].
//!
//! Why a burn-priority-only fill is not used: on the A380-800, whose burn
//! order puts the tailplane trim tank third of four, it fills the trim tank
//! and the outer wing to capacity at a partial load and leaves the inner feed
//! tanks empty. The fuel centroid then sits 12.75 m further aft than an
//! inner-first fill of the same mass, aft of the main-gear station, which is
//! the whole of that aircraft's `static_margin_floor` and
//! `min_nose_gear_load` exceedance. That inner cell lumps in Feed 2 and Feed 3
//! (EASA.A.110 Issue 17, section 3.3), two of the four tanks the engines are
//! fed from, so that state cannot be dispatched. Deferring trim alone removes
//! the aft-CG failure but still leaves the inner feed-containing cells empty;
//! the A380 approximation gives them positive fuel at partial loads.

use alas_geom::aircraft::spacing::linspace;

use crate::inertia::rectangular_prism;
use crate::ledger::{MassGroup, MassItem, MassMethod, MassProperties, MassRole};

use super::types::{FuelCgPoint, FuelTank, FuelTankLayout, TankKind, TankLayoutError, TankSide};

/// Match the registered A380 tank arrangement. Resolved layouts carry no
/// preset name, and candidate tank capacities may scale with wing geometry.
/// A custom layout with the same seven-cell topology and burn priorities will
/// also receive this rule; callers must not interpret it as aircraft identity.
fn is_a380_arrangement(tanks: &[FuelTank]) -> bool {
    if tanks.len() != 7 {
        return false;
    }
    let pair = |kind, priority| {
        tanks.iter().filter(|tank| tank.kind == kind).count() == 2
            && [TankSide::Left, TankSide::Right].into_iter().all(|side| {
                tanks.iter().any(|tank| {
                    tank.kind == kind && tank.side == side && tank.burn_priority == priority
                })
            })
    };
    pair(TankKind::WingInner, 1)
        && pair(TankKind::WingMid, 2)
        && pair(TankKind::WingOuter, 4)
        && tanks.iter().any(|tank| {
            tank.kind == TankKind::Trim
                && tank.side == TankSide::Centerline
                && tank.burn_priority == 3
        })
}

/// Add the same fraction of each selected cell's remaining capacity.
fn fill_to_fraction(
    tanks: &[FuelTank],
    fills_kg: &mut [f64],
    remaining_kg: &mut f64,
    selected: impl Fn(TankKind) -> bool,
    limit: f64,
) {
    let indices: Vec<usize> = tanks
        .iter()
        .enumerate()
        .filter_map(|(index, tank)| selected(tank.kind).then_some(index))
        .collect();
    let space_kg: f64 = indices
        .iter()
        .map(|&index| (limit * tanks[index].usable_capacity_kg - fills_kg[index]).max(0.0))
        .sum();
    if space_kg <= 0.0 || *remaining_kg <= 0.0 {
        return;
    }
    let added_kg = remaining_kg.min(space_kg);
    for index in indices {
        let space = (limit * tanks[index].usable_capacity_kg - fills_kg[index]).max(0.0);
        fills_kg[index] += added_kg * space / space_kg;
    }
    *remaining_kg -= added_kg;
}

/// Approximate A380 ground load; no ZFW or zero-fuel CG reaches this API.
fn distribute_a380(tanks: &[FuelTank], usable_fuel_kg: f64) -> FuelState {
    let mut fills_kg = vec![0.0; tanks.len()];
    let mut remaining_kg = usable_fuel_kg;
    // The two inner and two mid cells include feed and nonfeed tank volume.
    // Positive model-cell fuel cannot prove positive fuel in each actual
    // feed tank or aircraft dispatchability. Fill all
    // wing cells together first; then hold the outers at half capacity while
    // the inner/mid and trim cells have space (Airbus Training Center A380
    // ATA 28 p.0012: outer tanks <=50% whenever possible). The 39.5% MAC
    // refuelling target requires ZFW/CG and is not solved here. These stages
    // are an engineering approximation, not FQMS or a dispatch minimum.
    fill_to_fraction(
        tanks,
        &mut fills_kg,
        &mut remaining_kg,
        |kind| {
            matches!(
                kind,
                TankKind::WingInner | TankKind::WingMid | TankKind::WingOuter
            )
        },
        0.5,
    );
    fill_to_fraction(
        tanks,
        &mut fills_kg,
        &mut remaining_kg,
        |kind| matches!(kind, TankKind::WingInner | TankKind::WingMid),
        1.0,
    );
    fill_to_fraction(
        tanks,
        &mut fills_kg,
        &mut remaining_kg,
        |kind| kind == TankKind::Trim,
        1.0,
    );
    fill_to_fraction(
        tanks,
        &mut fills_kg,
        &mut remaining_kg,
        |kind| kind == TankKind::WingOuter,
        1.0,
    );
    FuelState { fills_kg }
}

/// A prism-shaped [`MassItem`] for `mass_kg` of fuel resting in `tank`.
///
/// The prism's height is scaled by how full the tank is (`mass_kg` against
/// its usable capacity): a half-full tank is modelled as a shorter prism at
/// the same footprint, not a full-height prism at half density, which is
/// what keeps the vertical inertia term honest as a tank empties.
fn fuel_prism_item(tank: &FuelTank, mass_kg: f64, role: MassRole, id: String) -> MassItem {
    let fill_fraction = if tank.usable_capacity_kg > 0.0 {
        (mass_kg / tank.usable_capacity_kg).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let [length_x_m, width_y_m, height_z_m] = tank.extent_m;
    MassItem {
        id,
        group: MassGroup::Fuel,
        role,
        mass_kg,
        position_m: tank.centroid_m,
        local_inertia: rectangular_prism(
            mass_kg,
            length_x_m,
            width_y_m,
            height_z_m * fill_fraction,
        ),
        method: MassMethod::TankFill,
    }
}

impl FuelTankLayout {
    /// Allocate `usable_fuel_kg` to tanks. The A380 arrangement follows the
    /// approximate rule above; other layouts fill tanks burned last first,
    /// except trim, which fills last.
    ///
    /// # Errors
    ///
    /// [`TankLayoutError::InvalidFuelMass`] if `usable_fuel_kg` is negative
    /// or not finite. [`TankLayoutError::Overflow`] if it exceeds
    /// [`Self::usable_capacity_kg`].
    pub fn distribute(&self, usable_fuel_kg: f64) -> Result<FuelState, TankLayoutError> {
        if !usable_fuel_kg.is_finite() || usable_fuel_kg < 0.0 {
            return Err(TankLayoutError::InvalidFuelMass {
                fuel_kg: usable_fuel_kg,
            });
        }
        let capacity_kg = self.usable_capacity_kg();
        if usable_fuel_kg > capacity_kg {
            return Err(TankLayoutError::Overflow {
                excess_kg: usable_fuel_kg - capacity_kg,
            });
        }
        if is_a380_arrangement(&self.tanks) {
            return Ok(distribute_a380(&self.tanks, usable_fuel_kg));
        }
        let mut fill_order: Vec<usize> = (0..self.tanks.len()).collect();
        // General-layout assumption: defer trim until the other cells are
        // full. This is not a sourced ground/taxi CG limit or a claim about
        // any registered aircraft's actual refuelling sequence.
        fill_order.sort_by(|&a, &b| {
            let a_trim = self.tanks[a].kind == TankKind::Trim;
            let b_trim = self.tanks[b].kind == TankKind::Trim;
            a_trim.cmp(&b_trim).then_with(|| {
                self.tanks[b]
                    .burn_priority
                    .cmp(&self.tanks[a].burn_priority)
            })
        });
        let mut fills_kg = vec![0.0; self.tanks.len()];
        let mut remaining_kg = usable_fuel_kg;
        // Tanks that share a burn priority (mirrored left/right pairs, most
        // often) split their group's fill by capacity share rather than
        // sequentially by index: a stable sort on equal keys would
        // otherwise always saturate the first-listed tank of the tie before
        // touching the other, biasing every partial load toward one side of
        // a symmetric aircraft with no physical cause.
        for group in
            fill_order.chunk_by(|&a, &b| self.tanks[a].burn_priority == self.tanks[b].burn_priority)
        {
            let group_capacity_kg: f64 = group
                .iter()
                .map(|&index| self.tanks[index].usable_capacity_kg)
                .sum();
            let group_fill_kg = remaining_kg.min(group_capacity_kg);
            if group_capacity_kg > 0.0 {
                for &index in group {
                    let share = self.tanks[index].usable_capacity_kg / group_capacity_kg;
                    fills_kg[index] = share * group_fill_kg;
                }
            }
            remaining_kg -= group_fill_kg;
        }
        Ok(FuelState { fills_kg })
    }

    /// Every tank's unusable fuel as a fixed [`MassItem`] at its centroid.
    pub fn unusable_items(&self) -> Vec<MassItem> {
        self.tanks
            .iter()
            .filter(|tank| tank.unusable_kg > 0.0)
            .map(|tank| {
                fuel_prism_item(
                    tank,
                    tank.unusable_kg,
                    MassRole::UnusableFuel,
                    format!("{}_unusable", tank.id),
                )
            })
            .collect()
    }

    /// The fuel-loading centre-of-gravity curve from empty to full capacity,
    /// following the loading order (the reverse of the burn order) at
    /// `steps` evenly spaced fuel levels.
    ///
    /// Returns an empty vector for `steps == 0`. The last point is always at
    /// [`Self::usable_capacity_kg`], so it equals the full-layout centroid
    /// exactly rather than falling short of it by one step.
    pub fn fuel_cg_curve(&self, steps: usize) -> Vec<FuelCgPoint> {
        if steps == 0 {
            return Vec::new();
        }
        let capacity_kg = self.usable_capacity_kg();
        let levels = if steps == 1 {
            vec![capacity_kg]
        } else {
            linspace(0.0, capacity_kg, steps)
        };
        levels
            .into_iter()
            .filter_map(|fuel_kg| {
                let state = self.distribute(fuel_kg).ok()?;
                Some(FuelCgPoint {
                    fuel_kg,
                    cg_m: state.properties(self).cg_m,
                })
            })
            .collect()
    }
}

/// One fuel load: how much sits in each tank of a [`FuelTankLayout`].
#[derive(Debug, Clone, PartialEq)]
pub struct FuelState {
    /// Fill of each tank, in the same order as [`FuelTankLayout::tanks`].
    fills_kg: Vec<f64>,
}

impl FuelState {
    /// Total usable fuel on board.
    pub fn total_kg(&self) -> f64 {
        self.fills_kg.iter().sum()
    }

    /// One [`MassItem`] per tank actually carrying fuel.
    pub fn mass_items(&self, layout: &FuelTankLayout) -> Vec<MassItem> {
        layout
            .tanks
            .iter()
            .zip(&self.fills_kg)
            .filter(|(_, &fill_kg)| fill_kg > 0.0)
            .map(|(tank, &fill_kg)| {
                fuel_prism_item(tank, fill_kg, MassRole::UsableFuel, tank.id.clone())
            })
            .collect()
    }

    /// This state's combined mass, centre of gravity and inertia tensor.
    pub fn properties(&self, layout: &FuelTankLayout) -> MassProperties {
        let items = self.mass_items(layout);
        let parts: Vec<MassProperties> = items.iter().map(MassItem::properties).collect();
        MassProperties::combine(parts.iter())
    }

    /// Remove `burned_kg`, lowest burn-priority tank first.
    ///
    /// # Errors
    ///
    /// [`TankLayoutError::InvalidFuelMass`] if `burned_kg` is negative or
    /// not finite. [`TankLayoutError::InsufficientFuel`] if it exceeds
    /// [`Self::total_kg`].
    pub fn burned(&self, layout: &FuelTankLayout, burned_kg: f64) -> Result<Self, TankLayoutError> {
        if !burned_kg.is_finite() || burned_kg < 0.0 {
            return Err(TankLayoutError::InvalidFuelMass { fuel_kg: burned_kg });
        }
        let available_kg = self.total_kg();
        if burned_kg > available_kg {
            return Err(TankLayoutError::InsufficientFuel {
                shortfall_kg: burned_kg - available_kg,
            });
        }
        let mut burn_order: Vec<usize> = (0..layout.tanks.len()).collect();
        burn_order.sort_by_key(|&index| layout.tanks[index].burn_priority);
        let mut fills_kg = self.fills_kg.clone();
        let mut remaining_kg = burned_kg;
        // Same-priority tanks (mirrored pairs) burn down together, split by
        // each tank's current fuel share, for the same reason `distribute`
        // fills them together: a sequential drain of the first-listed tank
        // of a tie would otherwise walk the aircraft's fuel CG off-axis on a
        // symmetric default with no asymmetric load to justify it.
        for group in burn_order
            .chunk_by(|&a, &b| layout.tanks[a].burn_priority == layout.tanks[b].burn_priority)
        {
            if remaining_kg <= 0.0 {
                break;
            }
            let group_available_kg: f64 = group.iter().map(|&index| fills_kg[index]).sum();
            let group_take_kg = remaining_kg.min(group_available_kg);
            if group_available_kg > 0.0 {
                for &index in group {
                    let share = fills_kg[index] / group_available_kg;
                    fills_kg[index] -= share * group_take_kg;
                }
            }
            remaining_kg -= group_take_kg;
        }
        Ok(Self { fills_kg })
    }
}
