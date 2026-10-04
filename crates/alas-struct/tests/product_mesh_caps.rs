// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product cap centroids, section inventory and both serialized deck dialects.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use alas_config::{materials, DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::{build_wing_mesh_bdf, build_wing_mesh_bdf_product};
use alas_struct::nastran95::{build_modes_deck, rectangle, Dialect};
use alas_struct::sizing::size_wingbox_reference_compatibility;
use support::deck::parse;

#[test]
fn product_caps_have_two_physical_flange_centroids_and_no_extra_beam_web() {
    let cfg = StructuresConfig {
        num_ribs_override: Some(12),
        mesh_chordwise_points: 18,
        ..Default::default()
    };
    let req = DesignRequirements::default();
    let engine = EngineConfig::default();
    let mass = MassModelConfig::default();
    let material = materials::get("Al 7075-T6").expect("registered material");
    let geometry = support::build_geometry(&[0.15, 0.65], &[true, true]);
    let sizing = size_wingbox_reference_compatibility(
        &geometry, &cfg, &req, material, material, material, material,
    );
    let (reference, _, _) = build_wing_mesh_bdf(
        &geometry, &sizing, &cfg, &engine, &mass, &req, material, material, material, material,
    )
    .expect("reference mesh");
    let (product, _, _) = build_wing_mesh_bdf_product(
        &geometry, &sizing, &cfg, &engine, &mass, &req, material, material, material, material,
    )
    .expect("product mesh");
    let old = parse(&reference.write_bulk());
    let cards = parse(&product.write_bulk());
    let bars: Vec<_> = cards.iter().filter(|card| card.name == "CBAR").collect();
    let properties: Vec<_> = cards.iter().filter(|card| card.name == "PBARL").collect();
    assert_eq!(
        bars.len(),
        2 * old.iter().filter(|card| card.name == "CBAR").count()
    );
    assert_eq!(
        product.quads(),
        reference.quads(),
        "web/skin shells are unchanged"
    );
    assert_eq!(bars.len(), 2 * properties.len());
    for pair in bars.chunks_exact(2) {
        let property = properties
            .iter()
            .find(|p| p.integer(0) == pair[0].integer(1))
            .unwrap();
        assert_eq!(property.text(3), "BAR");
        let width = property.real(8);
        let thickness = property.real(9);
        let constants = rectangle(&[width, thickness]).unwrap();
        assert!((constants.area - width * thickness).abs() < 1e-14);
        assert_eq!(pair[0].integer(1), pair[1].integer(1));
        for (node_field, offset_field) in [(2, 10), (3, 13)] {
            let top = product.grid_xyz(pair[0].integer(node_field)).unwrap();
            let bottom = product.grid_xyz(pair[1].integer(node_field)).unwrap();
            let mut spacing2 = 0.0;
            let mut height2 = 0.0;
            let mut offset2 = 0.0;
            for axis in 0..3 {
                let wa = pair[0].real(offset_field + axis);
                let wb = pair[1].real(offset_field + axis);
                assert!(
                    (wa + wb).abs() < 1e-12,
                    "centroid must stay on box midplane"
                );
                spacing2 += (top[axis] + wa - bottom[axis] - wb).powi(2);
                height2 += (top[axis] - bottom[axis]).powi(2);
                offset2 += wa * wa;
            }
            assert!((offset2.sqrt() - thickness / 2.0).abs() < 1e-12);
            assert!((spacing2.sqrt() - (height2.sqrt() - thickness)).abs() < 1e-11);
            let i_pair = 2.0 * (constants.i1 + constants.area * spacing2 / 4.0);
            let i_expected = width * thickness.powi(3) / 6.0
                + width * thickness * (height2.sqrt() - thickness).powi(2) / 2.0;
            assert!((i_pair - i_expected).abs() < 1e-10);
        }
    }
    for dialect in [Dialect::Nastran95, Dialect::Modern] {
        let small = parse(&build_modes_deck(&product, &cfg, dialect));
        let written_bars: Vec<_> = small.iter().filter(|card| card.name == "CBAR").collect();
        assert_eq!(written_bars.len(), bars.len());
        let written_properties: Vec<_> = small.iter().filter(|card| card.name == "PBAR").collect();
        assert_eq!(
            written_properties.len(),
            properties.len(),
            "shared PBAR emitted exactly once"
        );
        for written in written_properties {
            let original = properties
                .iter()
                .find(|p| p.integer(0) == written.integer(0))
                .unwrap();
            let (width, thickness) = (original.real(8), original.real(9));
            for corner in 0..4 {
                assert!((written.real(8 + 2 * corner).abs() - thickness / 2.0).abs() < 1e-5);
                assert!((written.real(9 + 2 * corner).abs() - width / 2.0).abs() < 1e-5);
            }
        }
        for (written, original) in written_bars.iter().zip(&bars) {
            for field in 10..16 {
                assert!((written.real(field) - original.real(field)).abs() < 1e-5);
            }
        }
    }
}

#[test]
fn solid_rectangle_reduction_has_physical_axes_and_saint_venant_torsion() {
    let rectangle = rectangle(&[0.1, 0.02]).unwrap();
    assert!((rectangle.area - 0.002).abs() < 1e-15);
    assert!((rectangle.i1 - 0.1 * 0.02_f64.powi(3) / 12.0).abs() < 1e-16);
    assert!((rectangle.i2 - 0.02 * 0.1_f64.powi(3) / 12.0).abs() < 1e-16);
    assert!(rectangle.j > 0.0 && rectangle.j < 0.1 * 0.02_f64.powi(3) / 3.0);
    let square = alas_struct::nastran95::rectangle(&[1.0, 1.0]).unwrap();
    assert!((square.j - 0.140577014955).abs() < 1e-9);
}
