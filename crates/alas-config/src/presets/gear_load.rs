// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The static nose-gear share an airport-planning document publishes at the
//! most-aft centre of gravity, as a function of aircraft weight: plausibility
//! reference data, not an input of the model's ground aft limit.
//!
//! Airport-planning documents print the ground aft limit in gear-load terms:
//! Airbus section 7-3-0 tabulates, per weight variant, the main-gear load at
//! the most-aft CG at that variant's maximum ramp weight (the nose share
//! follows by subtraction), and Boeing and Airbus A220 section 7.4 charts
//! plot the whole ground CG envelope as percent of weight on the main gear
//! against aircraft weight. Each point is a published (weight, nose share)
//! pair, with the most-aft CG in % MAC where the source prints it.
//!
//! The tables echo what each manufacturer certifies for its own airframe, so
//! the model does not read them: its minimum nose-gear load is the class value
//! `mass_model.pct_load_nlg_min`, the same for every aircraft, set from the
//! lowest share at a certified most-aft CG in a sample of airliner families
//! that is independent of these tables. The plausibility check reads the
//! printed aft CG of the points to compare the model's aft limit with the
//! published one.

/// One published static gear split at the most-aft centre of gravity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AftCgNoseLoadPoint {
    /// Aircraft mass the split is published at, kg.
    pub mass_kg: f64,
    /// Static nose-gear load over aircraft weight at that mass and the
    /// most-aft centre of gravity, dimensionless.
    pub nose_gear_fraction: f64,
    /// The most-aft centre of gravity at that mass, % MAC of the source's
    /// own frame, when the source prints it.
    pub aft_cg_pct_mac: Option<f64>,
}

/// The published nose-gear share at the most-aft centre of gravity against
/// aircraft weight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedAftCgNoseLoad {
    /// Published points in ascending mass order (at least one).
    pub points: &'static [AftCgNoseLoadPoint],
    /// Revision-locked document, figures and rows.
    pub source: &'static str,
}

mod published;

pub(crate) use published::{
    A220_300, A320_200, A340_300, A380_800, B747_400, B787_9, DC_10_30, E195_E2, LJ60,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_published_table_is_ascending_and_physical() {
        for table in [
            A220_300, A320_200, A340_300, A380_800, B747_400, B787_9, DC_10_30, E195_E2, LJ60,
        ] {
            assert!(!table.points.is_empty(), "{}", table.source);
            for pair in table.points.windows(2) {
                assert!(pair[0].mass_kg < pair[1].mass_kg, "{}", table.source);
            }
            for point in table.points {
                assert!(
                    point.nose_gear_fraction > 0.0 && point.nose_gear_fraction < 0.15,
                    "{}: {point:?}",
                    table.source
                );
            }
        }
    }
}
