// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py (`SeatClassConfig`)
// Reference: alas @ rust-port-baseline.

//! One block of the passenger cabin.
//!
//! Product configuration exposes the desired passenger share for each class.
//! Seat geometry and counts remain serialized for compatibility and for the
//! sizing engine, but are governed by the selected airline preset rather than
//! presenting several competing sources of truth in the form.
//!
//! The hidden geometry fields use bounds broad enough for all product classes:
//! the floor is economy's 28-inch pitch and
//! 16-inch width, and the ceiling has to clear a business flat-bed suite. The
//! range is wide, and its job is only to stop a value that could never be
//! certified -- a 0.2 metre pitch -- from being enterable at all.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// One passenger cabin class block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct SeatClassConfig {
    /// Share of usable cabin floor length given to this class, in percent.
    #[config(
        min = 0.0,
        max = 100.0,
        label = "Target passenger share [%]",
        readonly_unless(field = "requirements.cabin_preset", value = "Custom"),
        help = "Target percentage of passengers in this class. The selected preset supplies the seat geometry; the layout converts the normalized mix into rows that fit the usable, regulation-compliant cabin. Only Custom exposes this value for editing."
    )]
    pub share_pct: f64,

    /// Exact seat count, when the mix is given as counts.
    #[config(skip)]
    pub count: i64,

    /// Seats per row, or zero to derive it from the cabin width.
    #[config(skip)]
    pub abreast: i64,

    /// Longitudinal seat spacing.
    #[config(skip)]
    pub pitch_m: f64,

    /// Lateral footprint per seat.
    #[config(skip)]
    pub width_m: f64,

    /// Seated mass per occupant.
    #[config(skip)]
    pub mass_per_pax_kg: f64,
}

impl SeatClassConfig {
    /// A class with the given share, geometry and occupant mass, and no
    /// explicit seat count or seats abreast.
    ///
    /// The classes differ only in these four numbers, so this is what
    /// the cabin's defaults are written with.
    pub fn new(share_pct: f64, pitch_m: f64, width_m: f64, mass_per_pax_kg: f64) -> Self {
        Self {
            share_pct,
            count: 0,
            abreast: 0,
            pitch_m,
            width_m,
            mass_per_pax_kg,
        }
    }

    /// Whether this class is present in the cabin at all.
    ///
    /// Absence is a count of zero rather than a separate flag, which is what
    /// lets four fixed slots stand in for a variable-length list.
    pub fn is_present(&self) -> bool {
        self.count > 0
    }
}

impl Default for SeatClassConfig {
    fn default() -> Self {
        Self::new(0.0, 0.79, 0.45, 100.0)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entry;

    #[test]
    fn a_class_with_no_seats_is_absent() {
        assert!(!SeatClassConfig::default().is_present());
        let occupied = SeatClassConfig {
            count: 12,
            ..Default::default()
        };
        assert!(occupied.is_present());
    }

    #[test]
    fn only_the_target_share_reaches_the_product_settings_schema() {
        let schema = SeatClassConfig::default().schema();
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].name, "share_pct");
    }

    #[test]
    fn the_target_share_is_editable_only_for_custom_presets() {
        let schema = SeatClassConfig::default().schema();
        match &schema.field("share_pct").unwrap().entry {
            Entry::Leaf(leaf) => {
                let condition = leaf.readonly_unless.unwrap();
                assert_eq!(condition.field, "requirements.cabin_preset");
                assert_eq!(condition.value, "Custom");
            }
            Entry::Node(_) => panic!("a target share is not a group"),
        }
    }
}
