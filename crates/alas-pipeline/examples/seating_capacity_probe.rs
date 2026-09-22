// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ask what the pinned default shell seats as the passenger brief is raised.
//!
//! The passenger-capacity shortfall finding exists so that a brief the cabin
//! cannot physically seat is reported instead of being quietly satisfied. This
//! probe walks the brief upward on the fixed default design and prints what
//! the payload layout says each time, so "the shell now seats it" and "the
//! finding no longer fires" can be told apart.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-pipeline --example seating_capacity_probe
//! ```

// A probe whose entire purpose is its console output.
#![allow(clippy::print_stdout)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;
use alas_payload::layout::LayoutSummary;

fn main() {
    for brief in [350_i64, 400, 500, 700, 900, 1500] {
        let mut config = AlasConfig::default();
        config.mission.enabled = false;
        config.structures.enabled = false;
        config.requirements.num_passengers = brief;

        let design = DesignVector::default();
        let builder = AircraftBuilder::new(Some(config.geometry.clone()));
        let plane = match builder.build(Some(&design), true) {
            Ok(plane) => plane,
            Err(error) => {
                println!("{brief:>5} pax: geometry build failed: {error}");
                continue;
            }
        };
        // The layout only needs a mass and a station to place items against;
        // the seating question this probe asks does not depend on either.
        match build_payload_layout(&plane, &config, 100_000.0, 25.0) {
            Ok(layout) => match layout.summary {
                LayoutSummary::Passenger(summary) => println!(
                    "{brief:>5} pax: total {:>5}  seated {:>5}  unseated {:>5}",
                    summary.total_pax, summary.seated_pax, summary.unseated_pax
                ),
                LayoutSummary::Cargo(_) => {
                    println!("{brief:>5} pax: layout resolved as a cargo configuration");
                }
            },
            Err(error) => println!("{brief:>5} pax: layout failed: {error}"),
        }
    }
}
