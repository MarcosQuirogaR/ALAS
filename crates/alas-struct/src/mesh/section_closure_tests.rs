// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Fallible fixture construction failing here is a failed assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mesh::{
    cards::*,
    elements,
    nodes::{NodeMap, Surface},
};
use alas_config::{materials, AlasConfig, DesignVector, WingConfig};
use alas_geom::aircraft::airfoil::Airfoil;

fn geometry(section: &Airfoil, chord: f64) -> WingStructureGeometry {
    let design = DesignVector {
        span_m: 2.0,
        root_chord_m: chord,
        break_chord_m: chord,
        tip_chord_m: chord,
        sweep_deg: 0.0,
        ..Default::default()
    };
    WingStructureGeometry::new(
        &design,
        &WingConfig {
            root_z_m: 0.0,
            break_z_m: 0.0,
            tip_z_m: 0.0,
            outboard_sweep_decrement_deg: 0.0,
            ..Default::default()
        },
        section,
        section,
        &[0.25, 0.7],
        Some(&[true, true]),
    )
    .unwrap()
}

fn crossed_section() -> Airfoil {
    Airfoil::from_coordinates(
        "linear section",
        vec![(1.0, -0.1), (0.5, 0.2), (0.0, 0.0), (0.5, 0.0), (1.0, 0.0)],
    )
}

#[test]
fn intersections_use_the_first_exact_linear_root() {
    let knots = [0.0, 0.5, 1.0];
    let thickness = |x| if x <= 0.5 { 0.4 * x } else { 0.5 - 0.6 * x };
    let root = first_intersection(&knots, thickness).unwrap().unwrap();
    assert!((root - 5.0 / 6.0).abs() < 1e-15);
    assert!(thickness(root).abs() < 1e-15);
    assert!(first_intersection(&knots, |_| -0.1).is_err());
    let knots = [0.0, 0.2, 0.4, 0.6, 1.0];
    let heights = [0.0, 1.0, -1.0, 1.0, 0.0];
    let first = first_intersection(&knots, |x| {
        heights[knots.iter().position(|&knot| knot == x).unwrap()]
    })
    .unwrap()
    .unwrap();
    assert!(
        (first - 0.3).abs() < 1e-15,
        "a later reopening cannot restore material"
    );
}

#[test]
fn a_genuine_closed_trailing_edge_keeps_every_station_unchanged() {
    let section = Airfoil::from_coordinates(
        "closed section",
        vec![(1.0, 0.0), (0.5, 0.2), (0.0, 0.0), (0.5, 0.0), (1.0, 0.0)],
    );
    let geometry = geometry(&section, 1.0);
    let mut stations = geometry.get_rib_stations(8, 15);
    let original = stations.clone();
    assert_eq!(trim_aft_intersections(&geometry, &mut stations), Ok(0));
    assert_eq!(stations, original);
}

#[test]
fn trimming_preserves_spar_indices_and_independently_known_triangle_mass() {
    for chord in [1.0, 2.0] {
        let geometry = geometry(&crossed_section(), chord);
        let fractions = [0.0, 0.25, 0.5, 0.7, 0.9, 1.0];
        let mut station = geometry.get_rib_stations(2, 10).remove(0);
        station.extrados.clear();
        station.intrados.clear();
        for fraction in fractions {
            let (upper, lower) = geometry.airfoil_zu_zl(0.0, fraction);
            station
                .extrados
                .push([fraction * chord, 0.0, upper * chord]);
            station
                .intrados
                .push([fraction * chord, 0.0, lower * chord]);
        }
        station.j_spars = vec![1, 3];
        let mut stations = [station];
        assert_eq!(trim_aft_intersections(&geometry, &mut stations), Ok(1));
        let station = &stations[0];
        assert_eq!(station.j_spars, vec![1, 3]);
        assert_eq!(station.extrados.last(), station.intrados.last());
        assert!((station.frac_actual - 5.0 / 6.0).abs() < 1e-15);
        assert!(station
            .extrados
            .iter()
            .zip(&station.intrados)
            .all(|(u, l)| u[2] >= l[2]));
        let mut deck = Deck::new();
        let mut nodes = NodeMap::new();
        for (surface, points) in [
            (Surface::Ext, &station.extrados),
            (Surface::Int, &station.intrados),
        ] {
            for (index, &point) in points.iter().enumerate() {
                nodes.place(&mut deck, (0, index, surface), point);
            }
        }
        deck.materials.push(Mat1 {
            mid: 1,
            e: 1.0,
            g: 1.0,
            nu: 0.3,
            rho: 1_000.0,
        });
        deck.shell_properties.push(Pshell {
            pid: 1,
            mid1: 1,
            t: 0.01,
            mid2: 1,
        });
        let mut eid = 1;
        for index in 0..station.extrados.len() - 1 {
            elements::add_quad(
                &mut deck,
                &mut eid,
                [
                    nodes.at(0, index, Surface::Ext),
                    nodes.at(0, index + 1, Surface::Ext),
                    nodes.at(0, index + 1, Surface::Int),
                    nodes.at(0, index, Surface::Int),
                ],
                1,
            );
        }
        // The positive material is a triangle: base 5c/6, height c/5.
        let area = 0.5 * (5.0 * chord / 6.0) * (chord / 5.0);
        let expected_mass = area * 0.01 * 1_000.0;
        assert!((deck.primary_structural_mass_kg().unwrap() - expected_mass).abs() < 1e-12);
    }
}

#[test]
fn a_closure_inside_the_credited_spar_box_fails_closed() {
    let section = Airfoil::from_coordinates(
        "early closure",
        vec![(1.0, -0.2), (0.2, 0.1), (0.0, 0.0), (0.2, 0.0), (1.0, 0.0)],
    );
    let geometry = geometry(&section, 1.0);
    let mut stations = geometry.get_rib_stations(4, 25);
    assert!(matches!(
        trim_aft_intersections(&geometry, &mut stations),
        Err(MeshError::SectionMaterialDomain { .. })
    ));
}

#[test]
fn dc10_product_mesh_has_positive_material_mass_and_rejects_overlapping_caps() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset":"DC-10"})).unwrap();
    let design = alas_config::presets::get("DC-10").unwrap().design_vector;
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let wing = &plane.wings[0];
    let (fractions, full_span) = config.structures.resolved_spars();
    let geometry = WingStructureGeometry::new(
        &design,
        &config.geometry.wing,
        &wing.xsecs[0].airfoil,
        &wing.xsecs.last().unwrap().airfoil,
        &fractions,
        Some(&full_span),
    )
    .unwrap();
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).unwrap();
    let web = materials::get(&cfg.spar_web_material).unwrap();
    let cap = materials::get(&cfg.spar_cap_material).unwrap();
    let rib = materials::get(&cfg.rib_material).unwrap();
    let mut sizing = crate::sizing::size_wingbox_reference_compatibility(
        &geometry,
        cfg,
        &config.requirements,
        skin,
        web,
        cap,
        rib,
    );
    let build = |sizing: &crate::sizing::WingboxSizing| {
        crate::mesh::build_wing_mesh_bdf_product(
            &geometry,
            sizing,
            cfg,
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
            skin,
            web,
            cap,
            rib,
        )
    };
    let (deck, _, index) = build(&sizing).unwrap();
    assert!(deck
        .primary_structural_mass_kg()
        .is_some_and(|mass| mass > 0.0));
    assert!(index
        .spar_upper_nids
        .iter()
        .flatten()
        .all(|&node| deck.grid_xyz(node).is_some()));
    for spar in &mut sizing.spars {
        spar.t_cap.clone_from(&spar.h);
    }
    assert!(matches!(
        build(&sizing),
        Err(MeshError::InvalidCapGeometry { .. })
    ));
}
