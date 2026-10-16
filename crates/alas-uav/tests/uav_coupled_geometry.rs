// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Spec wing builder and the induced-drag convergence of the tip-clustered
//! stations.
//!
//! Convergence (UAV-local fix, `alas-aero` untouched so the transport path is
//! bit-identical): for a rectangular `A = 8` wing the Oswald factor read from
//! the Trefftz-plane drag of the production lattice, with the stations
//! clustered towards the tip by `build_wing`, against the lattice's own
//! per-panel cosine subdivision and uniform stations:
//!
//! | strips/half | uniform | pair cosine | tip cosine (this) |
//! |---|---|---|---|
//! | 8  | 0.861 | 0.891 | 0.912 |
//! | 24 | 0.923 | 0.944 | 0.952 |
//! | 96 | 0.956 | 0.964 | 0.965 |
//!
//! The elliptic case stays exact: `elliptic_wing_matches_prandtl_lifting_line`
//! in `uav_aero_viscous` (unchanged lattice path) gives `e = 1` within 1 %.

#[path = "support/aero_wings.rs"]
mod wings;

use alas_uav::aero::viscous_vlm::{ViscousConfig, ViscousVlm};
use alas_uav::aero::AirState;
use alas_uav::coupled::{build_wing, CoupledError};
use alas_uav::spec::master::{EmpennageDriving, MasterParams, WingPanel};

fn master(panels: Vec<WingPanel>, root: f64) -> MasterParams {
    MasterParams {
        root_chord_m: root,
        panels,
        winglet: None,
        wing_xyz_m: [0.0; 3],
        fuselage_xyz_m: [0.0; 3],
        tail_xyz_m: [1.0, 0.0, 0.0],
        tail_boom: None,
        controls: Vec::new(),
        empennage: EmpennageDriving::Driven {
            horizontal_volume: 0.5,
            vertical_volume: 0.04,
            arm_m: 1.0,
        },
        layout: Default::default(),
    }
}

fn panel(span: f64, taper: f64, root: &str, tip: &str) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: 0.0,
        dihedral_deg: 0.0,
        twist_tip_deg: 0.0,
        airfoil_root: root.into(),
        airfoil_tip: tip.into(),
    }
}

/// Oswald factor of the built wing with ideal linear sections.
fn efficiency(strips: usize) -> f64 {
    let m = master(vec![panel(1.0, 1.0, "naca0012", "naca0012")], 0.25);
    let b = build_wing(&m, 9.0, strips).unwrap();
    assert!((b.span_m - 2.0).abs() < 1e-9 && (b.s_ref_m2 - 0.5).abs() < 1e-9);
    let n = b.airplane.wings[0].xsecs.len();
    let sections = (0..n).map(|_| wings::linear_table("lin", 0.01)).collect();
    let cfg = ViscousConfig {
        spanwise_strips: n - 1,
        ..ViscousConfig::default()
    };
    let air = AirState::from_isa(0.0, 15.0).unwrap();
    ViscousVlm::new(&b.airplane, sections, cfg)
        .unwrap()
        .solve(&air, 0.1, None)
        .unwrap()
        .span_efficiency
        .unwrap()
}

#[test]
fn rectangular_induced_drag_converges_faster_than_the_lattice_subdivision() {
    let e = [
        (8, efficiency(8)),
        (16, efficiency(16)),
        (24, efficiency(24)),
        (96, efficiency(96)),
    ];
    let e96 = e[3].1;
    // Monotone and within 6 % / 2.5 % / 1.5 % of the 96-strip value.
    assert!(e.windows(2).all(|w| w[1].1 > w[0].1));
    assert!((1.0 - e[0].1 / e96) < 0.06, "{e:?}");
    assert!((1.0 - e[1].1 / e96) < 0.025, "{e:?}");
    assert!((1.0 - e[2].1 / e96) < 0.015, "{e:?}");
    // Against the per-panel cosine subdivision of the unchanged path at 8 strips.
    let plane = wings::rectangular(8.0, 0.25, "naca0012");
    let cfg = ViscousConfig {
        spanwise_strips: 8,
        ..ViscousConfig::default()
    };
    let air = AirState::from_isa(0.0, 15.0).unwrap();
    let old = ViscousVlm::new(
        &plane,
        vec![
            wings::linear_table("l", 0.01),
            wings::linear_table("l", 0.01),
        ],
        cfg,
    )
    .unwrap()
    .solve(&air, 0.1, None)
    .unwrap()
    .span_efficiency
    .unwrap();
    assert!(e[0].1 > old + 0.01, "new {} old {old}", e[0].1);
}

#[test]
fn multi_panel_wing_geometry_and_junction_checks() {
    let mut a = panel(0.4, 0.8, "naca2412", "naca2412");
    a.sweep_le_deg = 10.0;
    a.dihedral_deg = 3.0;
    a.twist_tip_deg = -1.0;
    let b = panel(0.6, 0.5, "naca2412", "naca0012");
    let m = master(vec![a, b], 0.3);
    let w = build_wing(&m, 9.0, 12).unwrap();
    let xs = &w.airplane.wings[0].xsecs;
    assert_eq!(xs.len(), w.sections.len());
    assert!((xs[0].chord - 0.3).abs() < 1e-12 && xs[0].xyz_le[1] == 0.0);
    let tip = xs.last().unwrap();
    assert!((tip.xyz_le[1] - 1.0).abs() < 1e-9);
    assert!((tip.chord - 0.3 * 0.8 * 0.5).abs() < 1e-9);
    assert!(
        (tip.twist + 1.0).abs() < 1e-9,
        "twist accumulates: {}",
        tip.twist
    );
    // Panel 1 tip: LE swept aft by 0.4 tan 10 deg and raised by 0.4 tan 3 deg.
    let junction = xs
        .iter()
        .find(|x| (x.xyz_le[1] - 0.4).abs() < 1e-9)
        .unwrap();
    assert!((junction.xyz_le[0] - 0.4 * 10.0_f64.to_radians().tan()).abs() < 1e-9);
    assert!((junction.xyz_le[2] - 0.4 * 3.0_f64.to_radians().tan()).abs() < 1e-9);
    // Stations cluster towards the tip on the outer panel: spacing shrinks.
    let y = &w.stations_y_m;
    let last = y[y.len() - 1] - y[y.len() - 2];
    let first_outer = y.iter().position(|&v| v > 0.4 + 1e-9).unwrap();
    let first = y[first_outer] - y[first_outer - 1];
    assert!(last < 0.5 * first, "tip spacing {last} vs {first}");
    // A mismatching junction airfoil is an error, not a silent pick.
    let bad = master(
        vec![
            panel(0.4, 1.0, "naca2412", "naca2412"),
            panel(0.4, 1.0, "naca0012", "naca0012"),
        ],
        0.3,
    );
    assert!(matches!(
        build_wing(&bad, 9.0, 8),
        Err(CoupledError::Spec(_))
    ));
    // Unknown airfoil names are reported.
    let unknown = master(vec![panel(0.4, 1.0, "no-such-foil", "no-such-foil")], 0.3);
    assert!(matches!(
        build_wing(&unknown, 9.0, 8),
        Err(CoupledError::Spec(_))
    ));
}
