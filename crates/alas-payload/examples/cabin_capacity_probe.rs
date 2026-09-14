// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: why each preset's cabin seats what it seats.
#![allow(clippy::print_stdout, missing_docs)]
// Standalone diagnostic examples report to the console and assert their inputs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::cabin::{cabin_deck_segments, max_certifiable_capacity, select_exit_type};
use alas_payload::{build_payload_layout, CabinGeometry, LayoutSummary};

fn main() {
    println!(
        "{:<11} {:>7} {:>7} {:>11} {:>10} {:>9} {:>6} {:>7} {:>6} {:>7} {:>10}",
        "preset",
        "cab_len",
        "usable_w",
        "generic_exit",
        "generic_cap",
        "model_cap",
        "pairs",
        "seated",
        "req",
        "planning",
        "source"
    );
    for name in presets::available() {
        let p = presets::get(name).unwrap();
        let mut config = AlasConfig {
            preset: p.name.to_owned(),
            geometry: p.geometry.clone(),
            requirements: p.requirements.clone(),
            landing_gear: p.landing_gear.clone(),
            ..AlasConfig::default()
        };
        config.cabin = p.planning_cabin_config();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&p.design_vector), true)
            .unwrap();
        let g = CabinGeometry::new(
            &plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .unwrap();
        let caps = max_certifiable_capacity(&g, &config.cabin.passenger);
        let spec = select_exit_type(g.diameter_m);
        let segs = cabin_deck_segments(&g);
        let cab_len: f64 = segs.iter().map(|s| s.x1 - s.x0).sum();
        let usable = segs
            .first()
            .map(|s| g.usable_width(s.deck, (s.x0 + s.x1) / 2.0))
            .unwrap_or(0.0);
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
        let LayoutSummary::Passenger(summary) = &layout.summary else {
            panic!("registered probe preset must build a passenger layout");
        };
        println!(
            "{:<11} {:>7.2} {:>8.2} {:>11} {:>10} {:>9} {:>6} {:>7} {:>6} {:>7} {:>10}",
            name,
            cab_len,
            usable,
            spec.name,
            caps.total,
            summary.max_certifiable_capacity,
            summary.exit_pairs,
            summary.seated_pax,
            config.requirements.num_passengers,
            p.reference.planning_seats.unwrap_or(-1),
            summary.source_exit_layout.unwrap_or("-"),
        );
    }
}
