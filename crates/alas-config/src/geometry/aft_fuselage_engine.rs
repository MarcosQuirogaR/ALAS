// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The installation of a rear-fuselage engine pair, as on a business jet.
//!
//! A podded wing engine is placed from the wing: its inlet from the local
//! leading edge and its height from the local wing height, so it follows the
//! wing when the wing moves or bends. A rear-fuselage engine hangs from the
//! fuselage through a stub pylon, so neither may follow the wing. This group
//! states the inlet station and the axis height in the fuselage's own frame;
//! the spanwise station stays in `EngineConfig::spanwise_positions_m`.
//!
//! Frame: x metres aft of the nose tip, z metres up in the fuselage-geometry
//! axes (the frame `fuselage.tail_z_m` and `vstab_z_m` are measured in). The
//! inlet station is held from the tail end, so a fuselage stretch carries the
//! engines with the empennage.

use serde::{Deserialize, Serialize};

/// A symmetric rear-fuselage engine installation: inlet station and axis height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AftFuselageEngineInstallation {
    /// Distance of the inlet face ahead of the fuselage tail end, m.
    pub inlet_ahead_of_tail_m: f64,
    /// Height of the nacelle axis, m, in the fuselage-geometry axes.
    pub axis_z_m: f64,
}

impl AftFuselageEngineInstallation {
    /// Whether both values are finite and the inlet is ahead of the tail end.
    pub fn is_valid(&self) -> bool {
        self.inlet_ahead_of_tail_m.is_finite()
            && self.inlet_ahead_of_tail_m > 0.0
            && self.axis_z_m.is_finite()
    }

    /// Whether the mount is valid and its inlet also stands aft of the nose of a
    /// fuselage `fuselage_length_m` long. The station is held from the tail end,
    /// so a fuselage shortened below it would put the inlet ahead of the nose.
    pub fn fits_fuselage(&self, fuselage_length_m: f64) -> bool {
        self.is_valid() && self.inlet_ahead_of_tail_m < fuselage_length_m
    }
}
