// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::sections::*;
use super::*;
use alas_config::materials;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::WingXSec;

fn rectangular_wing() -> Wing {
    let airfoil = Airfoil::from_name("naca0012").expect("NACA 0012 is registered");
    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([2.0, 0.0, 0.0], 4.0, 0.0, airfoil.clone()),
            WingXSec::new([2.0, 10.0, 0.0], 4.0, 0.0, airfoil),
        ],
        true,
    )
}

#[test]
fn spar_cap_allocation_recovers_the_applied_bending_moment_once() {
    let layout = SparLayout {
        fractions: vec![0.25, 0.70],
        full_span: vec![true, true],
    };
    let station = Station {
        xyz_le: [0.0, 0.0, 0.0],
        chord_m: 10.0,
        thickness_to_chord: vec![0.10, 0.08],
    };
    let structures = StructuresConfig::default();
    let cap_material = materials::get(&structures.spar_cap_material)
        .expect("the default cap material is registered");
    let skin_material =
        materials::get(&structures.skin_material).expect("the default skin is registered");
    let web_material = materials::get(&structures.spar_web_material)
        .expect("the default web material is registered");
    let supported_force_n = 1.0e6;
    let semi_span_m = 10.0;
    let web_thicknesses = vec![structures.t_web_min_m; 2];
    let distributed = distributed_station_mass(
        &station,
        0.0,
        semi_span_m,
        1.0,
        supported_force_n,
        &layout,
        &web_thicknesses,
        &structures,
        skin_material,
        web_material,
        cap_material,
    );

    let cap_area_each_m2 =
        distributed.caps_kg_m / (2.0 * layout.fractions.len() as f64 * cap_material.rho_kg_m3);
    let effective_height_sum_m = station.thickness_to_chord.iter().sum::<f64>()
        * station.chord_m
        * EFFECTIVE_CAP_DEPTH_FACTOR;
    let recovered_moment_nm = cap_area_each_m2 * cap_material.f_allow_pa * effective_height_sum_m;
    let applied_moment_nm = elliptic_cantilever_moment(0.0, semi_span_m, supported_force_n);

    assert!((recovered_moment_nm - applied_moment_nm).abs() / applied_moment_nm < 1e-12);
}

#[test]
fn a_partial_span_spar_is_inactive_outboard_of_the_break() {
    let layout = SparLayout {
        fractions: vec![0.25, 0.50, 0.70],
        full_span: vec![true, false, true],
    };

    assert!(active_spar(1, 0.40, 0.40, &layout));
    assert!(!active_spar(1, 0.41, 0.40, &layout));
    assert!(active_spar(0, 1.0, 0.40, &layout));
    assert!(active_spar(2, 1.0, 0.40, &layout));
}

#[test]
fn a_symmetric_wing_centroid_is_finite_and_on_the_centerline() {
    let centroid = wing_structural_centroid(
        &rectangular_wing(),
        &DesignRequirements::default(),
        &StructuresConfig::default(),
    )
    .expect("the default two-spar wingbox is valid");

    assert!(centroid.xyz_m.into_iter().all(f64::is_finite));
    assert_eq!(centroid.xyz_m[1], 0.0);
    assert!((3.0..=4.8).contains(&centroid.xyz_m[0]));
    assert!(
        centroid.cap_mass_kg > 0.0 && centroid.skin_mass_kg > 0.0 && centroid.rib_mass_kg > 0.0
    );
}

#[test]
fn an_invalid_spar_layout_is_reported_instead_of_using_a_fallback_point() {
    let structures = StructuresConfig {
        spar_chord_fractions: vec![0.25],
        ..Default::default()
    };
    let error = wing_structural_centroid(
        &rectangular_wing(),
        &DesignRequirements::default(),
        &structures,
    )
    .expect_err("one full-span spar cannot define a wingbox");

    assert_eq!(error, WingCentroidError::InvalidSparLayout);
}
