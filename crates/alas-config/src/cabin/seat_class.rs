// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py (`SeatClassConfig`)
// Reference: alas @ rust-port-baseline.

//! One block of the passenger cabin.
//!
//! A class is described by the share of cabin *length* it gets, not by a seat
//! count, because that is the quantity an airline actually chooses: how much
//! of the cabin is business and how much is economy. The seat count follows
//! from that share together with this class's pitch and seats abreast and the
//! fuselage's real shape, so it is an output of the layout and not something
//! that can be picked independently of it. The count is still settable, for
//! reproducing a specific aircraft, but only in the mode that asks for it.
//!
//! Every bound here is the union across all four classes, since they are four
//! instances of this one struct: the floor is economy's 28-inch pitch and
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
        label = "Share of cabin length [%]",
        help = "Percentage of usable cabin floor length allocated to this class. Seat count is derived from it using this class's pitch/abreast and the real fuselage geometry. Shares are normalised, so they need not add up to exactly 100. Set the class to 0 to remove it. Only used when Class mix mode is 'percent'."
    )]
    pub share_pct: f64,

    /// Exact seat count, when the mix is given as counts.
    #[config(
        min = 0,
        readonly_unless(field = "class_mix_mode", value = "count"),
        help = "Exact number of seats in this class (0 = class absent). Only editable when Class mix mode is 'count'; in 'percent' mode this is computed from the share above."
    )]
    pub count: i64,

    /// Seats per row, or zero to derive it from the cabin width.
    #[config(
        min = 0,
        help = "Seats per row in this class, or 0 to derive it from the local cabin floor width and this class's seat footprint. A premium suite fits far fewer abreast than an economy seat in the same fuselage, which is why this follows from the footprint rather than from the fuselage alone."
    )]
    pub abreast: i64,

    /// Longitudinal seat spacing.
    #[config(
        min = 0.7112,
        max = 2.5,
        help = "Longitudinal seat spacing. Regulatory/industry floor is economy's 28 in (0.7112 m); premium/business/first cabins use larger values (Matrix B, payload_processed.md)."
    )]
    pub pitch_m: f64,

    /// Lateral footprint per seat.
    #[config(
        min = 0.4064,
        max = 1.2,
        help = "Lateral seat footprint (incl. shell/armrests -- wider than the raw cushion for premium classes). Floor is economy's 16 in (0.4064 m) cushion width (Matrix B)."
    )]
    pub width_m: f64,

    /// Seated mass per occupant.
    #[config(
        help = "Mass of one occupant and their carry-on. The checked bag is not included here: it is added separately and routed to the lower-deck holds, so that the two together stay consistent with the lumped per-passenger mass the sizing pass uses."
    )]
    pub mass_per_pax_kg: f64,
}

impl SeatClassConfig {
    /// A class with the given share, geometry and occupant mass, and no
    /// explicit seat count or seats abreast.
    ///
    /// The four classes differ only in these four numbers, so this is what
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
    use crate::{Entry, Number};

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
    fn a_whole_number_bound_stays_a_whole_number() {
        // A seat count's minimum is 0, not 0.0. Serializing the bound as a
        // fraction would make a countable field look continuous to whatever
        // renders it.
        let schema = SeatClassConfig::default().schema();
        match &schema.field("count").unwrap().entry {
            Entry::Leaf(leaf) => assert_eq!(leaf.min, Some(Number::Integer(0))),
            Entry::Node(_) => panic!("a seat count is not a group"),
        }
        match &schema.field("share_pct").unwrap().entry {
            Entry::Leaf(leaf) => assert_eq!(leaf.min, Some(Number::Real(0.0))),
            Entry::Node(_) => panic!("a share is not a group"),
        }
    }

    #[test]
    fn the_seat_geometry_bounds_admit_economy_and_a_flat_bed_suite() {
        // The four classes share this struct, so the range has to cover both
        // ends; a bound that excluded either would make one class unenterable.
        let schema = SeatClassConfig::default().schema();
        match &schema.field("pitch_m").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.min, Some(Number::Real(0.7112)));
                assert_eq!(leaf.max, Some(Number::Real(2.5)));
            }
            Entry::Node(_) => panic!("a pitch is not a group"),
        }
    }

    #[test]
    fn the_seat_count_is_editable_only_where_it_is_an_input() {
        // In percent mode it is a derived output of the layout, and showing a
        // derived value as editable invites two sources of truth.
        let schema = SeatClassConfig::default().schema();
        match &schema.field("count").unwrap().entry {
            Entry::Leaf(leaf) => {
                let condition = leaf.readonly_unless.unwrap();
                assert_eq!(condition.field, "class_mix_mode");
                assert_eq!(condition.value, "count");
            }
            Entry::Node(_) => panic!("a seat count is not a group"),
        }
    }
}
