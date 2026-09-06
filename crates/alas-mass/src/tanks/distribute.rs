// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning a resolved [`FuelTankLayout`] into a fuel load, and a fuel load
//! into ledger items.
//!
//! Every quantity here is downstream of the same two facts the layout
//! already carries: how much a tank holds, and in what order it burns.
//! Loading fills the tanks burned last first, so the aircraft always has the
//! outboard-most fuel it would keep longest already on board; burning walks
//! the same order forward.

use alas_geom::aircraft::spacing::linspace;

use crate::inertia::rectangular_prism;
use crate::ledger::{MassGroup, MassItem, MassMethod, MassProperties, MassRole};

use super::types::{FuelCgPoint, FuelTank, FuelTankLayout, TankLayoutError};

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
    /// Fill every tank with `usable_fuel_kg`, tanks burned last filled
    /// first.
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
        let mut fill_order: Vec<usize> = (0..self.tanks.len()).collect();
        fill_order.sort_by(|&a, &b| {
            self.tanks[b]
                .burn_priority
                .cmp(&self.tanks[a].burn_priority)
        });
        let mut fills_kg = vec![0.0; self.tanks.len()];
        let mut remaining_kg = usable_fuel_kg;
        for index in fill_order {
            let fill = remaining_kg.min(self.tanks[index].usable_capacity_kg);
            fills_kg[index] = fill;
            remaining_kg -= fill;
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
        for index in burn_order {
            if remaining_kg <= 0.0 {
                break;
            }
            let take_kg = remaining_kg.min(fills_kg[index]);
            fills_kg[index] -= take_kg;
            remaining_kg -= take_kg;
        }
        Ok(Self { fills_kg })
    }
}
