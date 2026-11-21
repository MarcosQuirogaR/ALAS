// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cargo_loader.py (`CargoSlot`)

//! One loading position and what it holds.

use super::{UldType, BULK};

/// One loading position: a container of a given type at a station on a deck,
/// and what has been put in it.
#[derive(Debug, Clone, PartialEq)]
pub struct CargoSlot {
    /// The position's identifier, which is what a load plan refers to it by.
    pub sid: String,
    /// Which deck it is on.
    pub deck: &'static str,
    /// Longitudinal centre in aircraft body axes, m (positive aft).
    pub x: f64,
    /// Lateral centre in aircraft body axes, m (positive starboard).
    pub y: f64,
    /// The container standing here.
    pub uld: &'static UldType,
    /// Clear height used by this position, m. Rigid ULDs retain their full height.
    pub realized_height_m: f64,
    /// Width used by this position, m. Rigid ULDs retain their full width; loose
    /// bulk conforms to the hold floor it stands on.
    pub realized_width_m: f64,
    /// Net cargo loaded, kg, excluding the container's own tare.
    pub payload: f64,
    /// Volume of a pallet load built up to the section over it, m3: its load
    /// area times the mean height it is built to. `None` for a position that
    /// keeps the nominal volume of its type.
    pub contour_volume_m3: Option<f64>,
    /// A limit of this position below the container's own, kg net: the
    /// load-carrying ramp of a freight hold, and the volume the position
    /// holds at the stowage density (`CargoLoadManager::new`), or `None`.
    pub net_limit_kg: Option<f64>,
}

/// Below this a position counts as empty, so an all-but-unloaded container is
/// not flown, drawn or weighed.
pub(crate) const MIN_LOADED_KG: f64 = 1.0;

impl CargoSlot {
    /// Share of the nominal bulk block the hold's clear height and floor width
    /// leave.
    fn bulk_fill_ratio(&self) -> f64 {
        (self.realized_height_m / self.uld.height).clamp(0.0, 1.0)
            * (self.realized_width_m / self.uld.width).clamp(0.0, 1.0)
    }

    /// What this position may hold.
    pub fn max_net(&self) -> f64 {
        if self.uld.code == BULK.code {
            // A loose block's nominal mass is a volume-density proxy, not a
            // structural hold limit. Retain that density when headroom shrinks.
            let nominal = self.uld.max_net() * self.bulk_fill_ratio();
            self.net_limit_kg
                .map_or(nominal, |limit| limit.min(nominal))
        } else {
            self.net_limit_kg
                .map_or(self.uld.max_net(), |limit| limit.min(self.uld.max_net()))
        }
    }

    /// Usable position volume, cubic meters. Bulk volume follows the realized
    /// clearance, a built-up pallet the contour its load is built to.
    pub fn usable_volume_m3(&self) -> f64 {
        if let Some(volume) = self.contour_volume_m3 {
            volume
        } else if self.uld.code == BULK.code {
            self.uld.volume_m3 * self.bulk_fill_ratio()
        } else {
            // A position stacked below its type's nominal height holds that
            // share of the nominal volume.
            self.uld.volume_m3 * (self.realized_height_m / self.uld.height).min(1.0)
        }
    }

    /// What it weighs as loaded, container included, and nothing at all
    /// while it is empty, since an empty position is not carried.
    pub fn total_weight(&self) -> f64 {
        if self.payload > MIN_LOADED_KG {
            self.payload + self.uld.tare_weight
        } else {
            0.0
        }
    }
}
