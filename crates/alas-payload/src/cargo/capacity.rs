// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load-independent capacity, with container volume kept distinct from hold space.

use super::{CargoLoadManager, BULK};

/// Capacity of the generated positions, independent of the dispatched load.
///
/// Nominal ULD internal volume is not the aircraft's published hold volume:
/// it excludes space outside containers and may describe a different loading
/// configuration. These quantities must not share a correlation metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CargoCapacity {
    /// Rigid ULD positions, including empty positions and excluding loose bulk.
    pub uld_positions: usize,
    /// Loose-bulk positions, which carry no container tare.
    pub bulk_positions: usize,
    /// Sum of nominal internal ULD volumes in cubic metres.
    pub container_internal_volume_m3: f64,
    /// Sum of nominal volumes assigned to loose-bulk positions in cubic metres.
    pub bulk_nominal_volume_m3: f64,
    /// Net mass limit of all generated positions in kilograms.
    pub net_capacity_kg: f64,
}

impl CargoLoadManager<'_> {
    /// Describe generated position capacity without counting only loaded ULDs.
    pub fn capacity_summary(&self) -> CargoCapacity {
        let mut result = CargoCapacity {
            uld_positions: 0,
            bulk_positions: 0,
            container_internal_volume_m3: 0.0,
            bulk_nominal_volume_m3: 0.0,
            net_capacity_kg: self.total_capacity(),
        };
        for slot in &self.slots {
            if slot.uld.code == BULK.code {
                result.bulk_positions += 1;
                result.bulk_nominal_volume_m3 += slot.uld.volume_m3;
            } else {
                result.uld_positions += 1;
                result.container_internal_volume_m3 += slot.uld.volume_m3;
            }
        }
        result
    }
}
