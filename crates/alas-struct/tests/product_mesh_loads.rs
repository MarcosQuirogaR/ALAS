// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Point-mass moment balance at the same declared engine station as the beam.

// Fallible fixture construction failing here is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use alas_config::{materials, DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::{loads, mesh, sizing};
use support::deck::parse;

#[test]
fn product_engine_centre_preserves_declared_bending_arm_on_perpendicular_ribs() {
    let config = StructuresConfig {
        num_ribs_override: Some(12),
        ..Default::default()
    };
    let requirements = DesignRequirements::default();
    let engine = EngineConfig {
        // A signed symmetric pair retains the starboard installation only.
        spanwise_positions_m: vec![10.234, -10.234],
        ..Default::default()
    };
    let mass_config = MassModelConfig::default();
    let material = materials::get("Al 7075-T6").unwrap();
    let geometry = support::build_geometry(&[0.15, 0.65], &[true, true]);
    let sizing = sizing::size_wingbox_reference_compatibility(
        &geometry,
        &config,
        &requirements,
        material,
        material,
        material,
        material,
    );
    let (deck, _, _) = mesh::build_wing_mesh_bdf_product(
        &geometry,
        &sizing,
        &config,
        &engine,
        &mass_config,
        &requirements,
        material,
        material,
        material,
        material,
    )
    .unwrap();
    let cards = parse(&deck.write_bulk());
    let masses: Vec<_> = cards.iter().filter(|card| card.name == "CONM2").collect();
    let expected = loads::engine_point_loads_n(&engine, &mass_config, &requirements);
    assert_eq!(masses.len(), expected.len());
    assert!(!expected.is_empty());
    let mut actual_first_moment = 0.0;
    let mut expected_first_moment = 0.0;
    for (card, (station, mass)) in masses.iter().zip(expected) {
        let node = deck.grid_xyz(card.integer(1)).unwrap();
        let actual_station = node[1] + card.real(5);
        assert!((actual_station - station.abs()).abs() < 1.0e-10);
        assert!((card.real(3) - mass).abs() < 1.0e-8);
        actual_first_moment += card.real(3) * actual_station;
        expected_first_moment += mass * station.abs();
    }
    // GRAV multiplies this first moment by signed n*g exactly once.
    assert!((actual_first_moment - expected_first_moment).abs() < 1.0e-7);
}
