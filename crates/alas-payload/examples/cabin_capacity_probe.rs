// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: why each preset's cabin seats what it seats.
#![allow(clippy::print_stdout, missing_docs)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::cabin::{cabin_deck_segments, max_certifiable_capacity, select_exit_type};
use alas_payload::{build_payload_layout, CabinGeometry};

fn main() {
    println!(
        "{:<11} {:>7} {:>7} {:>7} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7}",
        "preset",
        "cab_len",
        "usable_w",
        "exit",
        "pairs",
        "exitcap",
        "req",
        "seated",
        "planning",
        "cert"
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
        let seated: i64 = layout
            .items
            .iter()
            .filter_map(|i| match &i.meta {
                alas_payload::ItemMeta::Seat(s) => Some(s.filled),
                _ => None,
            })
            .sum();
        println!(
            "{:<11} {:>7.2} {:>8.2} {:>7} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7}",
            name,
            cab_len,
            usable,
            spec.name,
            segs.len(),
            caps.total,
            config.requirements.num_passengers,
            seated,
            p.reference.planning_seats.unwrap_or(-1),
            p.reference.certified_max_seats.unwrap_or(-1),
        );
    }
}
