// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tank operations in flight: transfers, fuel received, and the trim tank.
//!
//! [`FuelState::burned`] consumes fuel in the one burn order of
//! [`super::order`]. A mission that moves fuel on purpose (a transfer to the
//! trim tank to hold the centre of gravity aft, a refuelling in the air)
//! needs the other operations too. Each takes a state and returns the state
//! after it, never exceeding a tank's usable capacity or going below empty,
//! and says how much of the request it could carry out: a transfer into a
//! full tank moves less, and the caller reports it.

use super::distribute::FuelState;
use super::order::{burn_groups, fill_groups};
use super::types::{FuelTankLayout, TankKind, TankLayoutError};

impl FuelState {
    /// The fuel in every tank, `(id, kg)`, in layout order.
    #[must_use]
    pub fn tank_fills(&self, layout: &FuelTankLayout) -> Vec<(String, f64)> {
        layout
            .tanks()
            .iter()
            .zip(&self.fills_kg)
            .map(|(tank, fill)| (tank.id.clone(), *fill))
            .collect()
    }

    fn index_of(layout: &FuelTankLayout, id: &str) -> Result<usize, TankLayoutError> {
        layout
            .tanks()
            .iter()
            .position(|tank| tank.id == id)
            .ok_or_else(|| TankLayoutError::UnknownTank { id: id.to_owned() })
    }

    /// Move up to `kg` from tank `from` to tank `to`. Returns the state and the
    /// mass actually moved, limited by what `from` holds and the room in `to`.
    ///
    /// # Errors
    ///
    /// [`TankLayoutError::UnknownTank`] for an id the layout does not have,
    /// [`TankLayoutError::InvalidFuelMass`] for a negative or non-finite mass.
    pub fn transferred(
        &self,
        layout: &FuelTankLayout,
        from: &str,
        to: &str,
        kg: f64,
    ) -> Result<(Self, f64), TankLayoutError> {
        if !kg.is_finite() || kg < 0.0 {
            return Err(TankLayoutError::InvalidFuelMass { fuel_kg: kg });
        }
        let (source, target) = (Self::index_of(layout, from)?, Self::index_of(layout, to)?);
        let room = layout.tanks()[target].usable_capacity_kg - self.fills_kg[target];
        let moved = kg.min(self.fills_kg[source]).min(room.max(0.0));
        let mut fills_kg = self.fills_kg.clone();
        fills_kg[source] -= moved;
        fills_kg[target] += moved;
        Ok((Self { fills_kg }, moved))
    }

    /// Take on up to `kg` of fuel in flight, into the free room of the tanks in
    /// loading order (feed tanks first, the trim tank last). Returns the state
    /// and the mass taken, which is less than `kg` when the tanks are full.
    ///
    /// # Errors
    ///
    /// [`TankLayoutError::InvalidFuelMass`] for a negative or non-finite mass.
    pub fn received(
        &self,
        layout: &FuelTankLayout,
        kg: f64,
    ) -> Result<(Self, f64), TankLayoutError> {
        if !kg.is_finite() || kg < 0.0 {
            return Err(TankLayoutError::InvalidFuelMass { fuel_kg: kg });
        }
        let mut fills_kg = self.fills_kg.clone();
        let added = put_into(layout, &mut fills_kg, &fill_groups(layout.tanks()), kg);
        Ok((Self { fills_kg }, added))
    }

    /// Burn `kg` in the burn order but leave every trim tank alone: while a
    /// trim control holds the centre of gravity, the trim fuel is not burned.
    ///
    /// # Errors
    ///
    /// [`TankLayoutError::InsufficientFuel`] when the other tanks hold less
    /// than `kg`, [`TankLayoutError::InvalidFuelMass`] for a bad mass.
    pub fn burned_holding_trim(
        &self,
        layout: &FuelTankLayout,
        kg: f64,
    ) -> Result<Self, TankLayoutError> {
        if !kg.is_finite() || kg < 0.0 {
            return Err(TankLayoutError::InvalidFuelMass { fuel_kg: kg });
        }
        let mut fills_kg = self.fills_kg.clone();
        let groups = without_trim(layout, burn_groups(layout.tanks()));
        let taken = take_from(&mut fills_kg, &groups, kg);
        if taken + 1.0e-9 < kg {
            return Err(TankLayoutError::InsufficientFuel {
                shortfall_kg: kg - taken,
            });
        }
        Ok(Self { fills_kg })
    }

    /// Fuel in the trim tanks, kg.
    #[must_use]
    pub fn trim_fuel_kg(&self, layout: &FuelTankLayout) -> f64 {
        layout
            .tanks()
            .iter()
            .zip(&self.fills_kg)
            .filter(|(tank, _)| tank.kind == TankKind::Trim)
            .map(|(_, fill)| *fill)
            .sum()
    }

    /// Usable capacity of the trim tanks, kg.
    #[must_use]
    pub fn trim_capacity_kg(layout: &FuelTankLayout) -> f64 {
        layout
            .tanks()
            .iter()
            .filter(|tank| tank.kind == TankKind::Trim)
            .map(|tank| tank.usable_capacity_kg)
            .sum()
    }

    /// The state with `target_kg` in the trim tanks, the difference moved from
    /// (or to) the other tanks: from the tank to be burned next when going aft,
    /// into the feed tanks first when coming forward. The total is unchanged.
    /// Returns the state and the trim fuel actually reached, which is limited by
    /// the trim capacity and by the fuel available elsewhere.
    #[must_use]
    pub fn with_trim_fuel(&self, layout: &FuelTankLayout, target_kg: f64) -> (Self, f64) {
        let mut fills_kg = self.fills_kg.clone();
        let current = self.trim_fuel_kg(layout);
        let trim_groups: Vec<Vec<usize>> = burn_groups(layout.tanks())
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .filter(|&index| layout.tanks()[index].kind == TankKind::Trim)
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect();
        let others = without_trim(layout, burn_groups(layout.tanks()));
        let others_fill_order = without_trim(layout, fill_groups(layout.tanks()));
        let wanted = target_kg.max(0.0);
        if wanted > current {
            let taken = take_from(&mut fills_kg, &others, wanted - current);
            let stored = put_into(layout, &mut fills_kg, &trim_groups, taken);
            // Fuel that did not fit goes back where it came from.
            put_into(layout, &mut fills_kg, &others_fill_order, taken - stored);
        } else {
            let taken = take_from(&mut fills_kg, &trim_groups, current - wanted);
            let stored = put_into(layout, &mut fills_kg, &others_fill_order, taken);
            put_into(layout, &mut fills_kg, &trim_groups, taken - stored);
        }
        let state = Self { fills_kg };
        let reached = state.trim_fuel_kg(layout);
        (state, reached)
    }
}

fn without_trim(layout: &FuelTankLayout, groups: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    groups
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .filter(|&index| layout.tanks()[index].kind != TankKind::Trim)
                .collect::<Vec<_>>()
        })
        .filter(|group| !group.is_empty())
        .collect()
}

/// Remove up to `kg` from the groups in order, sharing a group's take by each
/// tank's content; returns the mass removed.
fn take_from(fills_kg: &mut [f64], groups: &[Vec<usize>], kg: f64) -> f64 {
    let mut remaining = kg;
    for group in groups {
        if remaining <= 0.0 {
            break;
        }
        let available: f64 = group.iter().map(|&index| fills_kg[index]).sum();
        let take = remaining.min(available);
        if available > 0.0 {
            for &index in group {
                fills_kg[index] -= fills_kg[index] / available * take;
            }
        }
        remaining -= take;
    }
    kg - remaining
}

/// Add up to `kg` to the groups in order, sharing a group's intake by each
/// tank's free room; returns the mass added.
fn put_into(layout: &FuelTankLayout, fills_kg: &mut [f64], groups: &[Vec<usize>], kg: f64) -> f64 {
    let mut remaining = kg;
    for group in groups {
        if remaining <= 0.0 {
            break;
        }
        let room = |fills: &[f64], index: usize| {
            (layout.tanks()[index].usable_capacity_kg - fills[index]).max(0.0)
        };
        let free: f64 = group.iter().map(|&index| room(fills_kg, index)).sum();
        let put = remaining.min(free);
        if free > 0.0 {
            let shares: Vec<(usize, f64)> = group
                .iter()
                .map(|&index| (index, room(fills_kg, index) / free * put))
                .collect();
            for (index, share) in shares {
                fills_kg[index] += share;
            }
        }
        remaining -= put;
    }
    kg - remaining
}
