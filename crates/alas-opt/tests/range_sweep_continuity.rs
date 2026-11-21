// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product sizing of the A220-300 design vector over a sweep of design
//! ranges across the route-fit boundary of its FL250 route level: block fuel
//! has no step of the RANGE-STEP class.
//!
//! A cruise-speed ceiling that applied only below the configured level made
//! block fuel step +65 kg between 270 and 275 nmi (RANGE-STEP). A step of
//! `J` kg shows as a second difference of the block fuel of order `J`; the
//! slope corners of a continuous block fuel give about 1 kg at this 2 nmi
//! spacing.
//!
//! One smaller discrete step remains and is bounded here, not hidden: card
//! ROUTEFIT-ISLAND. The unpaid climb-to-cruise acceleration peaks near
//! 7,570 m, where the cruise speed turns from the VMO ceiling to the Mach
//! hold while the climb still ends at constant CAS, so the levels that fit
//! the route are not an interval: the plan's highest fitting level jumps from
//! 7,525 m to the configured 7,620 m between 273 and 274 nmi and block fuel
//! drops 4.4 kg (0.25 %) there.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;

/// Largest admitted second difference of block fuel, kg: above the 4.4 kg
/// level-choice step of ROUTEFIT-ISLAND and the 1 kg slope corners, an order
/// below the 65 kg RANGE-STEP jump.
const STEP_BOUND_KG: f64 = 10.0;

#[test]
fn a220_block_fuel_is_continuous_in_design_range() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"})).unwrap();
    let design = alas_config::presets::get("A220-300").unwrap().design_vector;
    let ranges_nmi: Vec<f64> = (0..=15).map(|i| 260.0 + 2.0 * f64::from(i)).collect();
    let block_kg: Vec<f64> = ranges_nmi
        .iter()
        .map(|&range| {
            let mut ranged = config.clone();
            ranged.optimizer.objective.design_range_nmi = range;
            assess_product_candidate(&ranged, &design)
                .unwrap_or_else(|error| panic!("{range} nmi: {error}"))
                .sized
                .block_fuel_kg
        })
        .collect();
    for (fuel, ranges) in block_kg.windows(3).zip(ranges_nmi.windows(3)) {
        let second_kg = fuel[2] - 2.0 * fuel[1] + fuel[0];
        assert!(
            second_kg.abs() <= STEP_BOUND_KG,
            "block fuel second difference {second_kg:.2} kg at {} nmi: {block_kg:?}",
            ranges[1]
        );
    }
}
