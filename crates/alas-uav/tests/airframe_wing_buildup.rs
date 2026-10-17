// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::unwrap_used, clippy::print_stdout)]

//! Wing component build-up: hand calculation and the three constructions.

use alas_uav::airframe::geometry::{Panel, Planform};
use alas_uav::airframe::wing_buildup::{Construction, WingBuildup};

fn rect() -> Planform {
    Planform {
        panels: vec![Panel {
            span_m: 1.0,
            chord_root_m: 0.3,
            chord_tip_m: 0.3,
            root_le_m: [0.0; 3],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: true,
        vertical: false,
    }
}

#[test]
fn foam_core_matches_the_hand_calculation() {
    // b 2 m, c 0.3 m, S 0.6 m2, t/c 0.12, 0.05 kg/m2 glass x1 ply, resin 1:1, XPS 33.
    let b = WingBuildup::new(Construction::FoamCoreComposite, 0.12, 4.0)
        .breakdown(&rect())
        .unwrap();
    let wetted = 2.0 * 0.6 * (1.0 + 0.25 * 0.12);
    assert!((b.skin_kg - wetted * 0.05 * 2.0).abs() < 1e-9);
    let vol = 0.689 * 0.12 * 0.3 * 0.3 * 2.0;
    assert!((b.core_kg - vol * 23.0).abs() < 1e-6 * vol * 23.0);
    // Spar: M = 5.25 * 4 * 9.80665 * 2 / (3 pi) = 43.7 N m > one tube (38 N m).
    let m = 5.25 * 4.0 * 9.806_65 * 2.0 / (3.0 * std::f64::consts::PI);
    let tubes = m / (650.0e6 / 1.5 * 87.8e-9);
    assert!(tubes > 1.0);
    assert!((b.spar_kg - 0.9 * 2.0 * 0.0536 * tubes).abs() < 1e-9);
}

#[test]
fn constructions_differ_and_the_team_factor_scales_everything() {
    let totals: Vec<f64> = [
        Construction::FoamCoreComposite,
        Construction::BalsaFilm,
        Construction::Printed3D,
    ]
    .iter()
    .map(|&c| {
        let b = WingBuildup::new(c, 0.12, 4.0).breakdown(&rect()).unwrap();
        println!("WING|{c:?}|{b:?}|total {:.3} kg", b.total_kg());
        b.total_kg()
    })
    .collect();
    assert!(totals.iter().all(|&t| t > 0.1 && t < 2.0));
    let mut w = WingBuildup::new(Construction::BalsaFilm, 0.12, 4.0);
    let base = w.breakdown(&rect()).unwrap().total_kg();
    w.team_factor = 0.8;
    let k = w.breakdown(&rect()).unwrap().total_kg();
    assert!((k - 0.8 * base).abs() < 1e-12);
}
