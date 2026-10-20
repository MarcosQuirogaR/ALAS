// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py (`SeatClassConfig`)

//! One block of the passenger cabin.
//!
//! Product configuration exposes the desired passenger share for each class
//! and, for a Custom cabin only, its seat geometry (pitch, width and seats
//! abreast). A named airline preset supplies that geometry itself, so the
//! form never shows two competing sources of truth. Counts and the per-class
//! occupant mass stay serialized for compatibility and for the sizing engine;
//! the occupant mass is derived from the single passenger-mass authority.
//!
//! The hidden geometry fields use bounds broad enough for all product classes:
//! the floor is economy's 28-inch pitch and
//! 16-inch width, and the ceiling has to clear a business flat-bed suite. The
//! range is wide, and its job is only to stop a value that could never be
//! certified (a 0.2 metre pitch) from being enterable at all.

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
    #[config(
        min = 0,
        max = 10,
        label = "Seats abreast",
        readonly_unless(field = "requirements.cabin_preset", value = "Custom"),
        help = "Seats per row in this class; 0 fits as many as the local cabin width allows beside the regulatory aisle (at most 6 per single aisle, 12 per twin aisle: FAR/CS 25.817). A value that does not fit the cabin width is reduced to the seats that fit. Only Custom exposes this value for editing."
    )]
    pub abreast: i64,

    /// Longitudinal seat spacing.
    #[config(
        min = 0.7112,
        max = 3.0,
        decimals = 3,
        label = "Seat pitch",
        unit = "m",
        readonly_unless(field = "requirements.cabin_preset", value = "Custom"),
        help = "Row-to-row seat spacing in this class. 0.7112 m (28 in) is the tightest economy pitch in service; business flat-bed suites need 1.5 m or more. Only Custom exposes this value for editing."
    )]
    pub pitch_m: f64,

    /// Lateral footprint per seat.
    #[config(
        min = 0.4064,
        max = 1.3,
        decimals = 3,
        label = "Seat width",
        unit = "m",
        readonly_unless(field = "requirements.cabin_preset", value = "Custom"),
        help = "Lateral footprint of one seat including its armrest. 0.4064 m (16 in) is the narrowest economy seat; suites reach about 0.95 m. The seats abreast times this width plus the aisles must fit the cabin width. Only Custom exposes this value for editing."
    )]
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
    fn the_share_and_the_seat_geometry_reach_the_product_settings_schema() {
        let schema = SeatClassConfig::default().schema();
        let names: Vec<_> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(names, ["share_pct", "abreast", "pitch_m", "width_m"]);
    }

    #[test]
    fn the_defaults_sit_inside_the_seat_geometry_bounds() {
        let schema = SeatClassConfig::default().schema();
        for name in ["pitch_m", "width_m"] {
            let Entry::Leaf(leaf) = &schema.field(name).unwrap().entry else {
                panic!("{name} is a leaf");
            };
            let value = leaf.value.as_f64().unwrap();
            let min = match leaf.min.unwrap() {
                crate::Number::Real(v) => v,
                crate::Number::Integer(v) => v as f64,
            };
            assert!(value >= min, "{name} default {value} below {min}");
        }
    }

    #[test]
    fn the_class_inputs_are_editable_only_for_custom_presets() {
        let schema = SeatClassConfig::default().schema();
        for name in ["share_pct", "abreast", "pitch_m", "width_m"] {
            match &schema.field(name).unwrap().entry {
                Entry::Leaf(leaf) => {
                    let condition = leaf.readonly_unless.unwrap();
                    assert_eq!(condition.field, "requirements.cabin_preset");
                    assert_eq!(condition.value, "Custom");
                }
                Entry::Node(_) => panic!("{name} is not a group"),
            }
        }
    }
}
