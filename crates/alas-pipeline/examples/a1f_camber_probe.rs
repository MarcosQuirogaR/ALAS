// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Zero-lift angle of a cambered rectangular wing on the incompressible
//! lattice against thin-airfoil theory (A1F evidence for the ALAS-vs-AVL
//! zero-lift offset).
//!
//! `cargo run --release -p alas-pipeline --example a1f_camber_probe <naca4> [chordwise ...]`
//! prints, per chordwise panel multiplier, CL(0), the lift slope and the
//! zero-lift angle `-CL(0) / CL_alpha`. NACA 2412 has -2.077 deg in thin
//! airfoil theory (Abbott and von Doenhoff, 1959, table of NACA 4-digit
//! sections: -2.0 deg experimentally).
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "naca2412".to_owned());
    let chordwise: Vec<usize> = args.filter_map(|a| a.parse().ok()).collect();
    let chordwise = if chordwise.is_empty() {
        vec![4, 8, 16, 32]
    } else {
        chordwise
    };
    let (semi, chord) = (8.0, 1.0);
    let af = || Airfoil::from_name(&name).unwrap();
    let main = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], chord, 0.0, af()),
            WingXSec::new([0.0, semi, 0.0], chord, 0.0, af()),
        ],
        true,
    );
    let (s_ref, b_ref, c_ref) = (
        main.reference_area(),
        main.reference_span(),
        main.mean_aerodynamic_chord(),
    );
    let plane = Airplane {
        name: "w".into(),
        xyz_ref: [0.25, 0.0, 0.0],
        wings: vec![main],
        fuselages: vec![],
        s_ref,
        c_ref,
        b_ref,
    };
    let atmosphere = Atmosphere::new(0.0);
    let velocity = 50.0;
    for nc in chordwise {
        let system = VlmSystem::assemble(&plane, 40, nc).unwrap();
        let cl = |alpha: f64| {
            let op = OperatingPoint::new(atmosphere, velocity, alpha, 0.0, 0.0, 0.0, 0.0);
            system.solve(&op).unwrap().cl_lift
        };
        let (c0, c4) = (cl(0.0), cl(4.0));
        let slope = (c4 - c0) / 4.0;
        println!(
            "{name} chordwise {nc:3}: CL(0) {c0:+.5}  CLa {slope:.5}/deg  alpha_L0 {:+.3} deg",
            -c0 / slope
        );
    }
}
