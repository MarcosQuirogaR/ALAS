// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lift at zero and two degrees of the built wing, planar / twisted / as built,
//! and with the stabilizer, on the incompressible lattice (A1F evidence for the
//! ALAS-vs-AVL zero-lift offset).
//!
//! `cargo run --release -p alas-pipeline --example a1f_cl0_probe <preset>`
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn cl(plane: &Airplane, alpha: f64, mesh: (usize, usize)) -> f64 {
    let atmosphere = Atmosphere::new(11_000.0);
    let velocity = 1.0e-4 * atmosphere.speed_of_sound();
    let system = VlmSystem::assemble(plane, mesh.0, mesh.1).unwrap();
    let op = OperatingPoint::new(atmosphere, velocity, alpha, 0.0, 0.0, 0.0, 0.0);
    system.solve(&op).unwrap().cl_lift
}

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().expect("preset");
    let chordwise: Vec<usize> = args.filter_map(|a| a.parse().ok()).collect();
    {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let design = presets::get(&name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap();
        let default_mesh = (
            config.analysis.fine_spanwise_resolution as usize,
            config.analysis.fine_chordwise_resolution as usize,
        );
        let mut base = report.airplane.clone();
        base.fuselages.clear();
        let main_index = (0..base.wings.len())
            .filter(|&i| base.wings[i].name != "Horizontal Stabilizer")
            .max_by(|&i, &j| {
                base.wings[i]
                    .projected_area()
                    .total_cmp(&base.wings[j].projected_area())
            })
            .unwrap();
        let mut as_built = base.clone();
        as_built.wings = vec![base.wings[main_index].clone()];
        let mut twisted = as_built.clone();
        let mut planar = as_built.clone();
        for xsec in &mut twisted.wings[0].xsecs {
            xsec.airfoil = Airfoil::from_name("naca0012").unwrap();
        }
        for xsec in &mut planar.wings[0].xsecs {
            xsec.airfoil = Airfoil::from_name("naca0012").unwrap();
            xsec.twist = 0.0;
        }
        let mut two = base.clone();
        two.wings
            .retain(|w| w.name == "Main Wing" || w.name == "Horizontal Stabilizer");
        let chordwise = if chordwise.is_empty() {
            vec![default_mesh.1]
        } else {
            chordwise
        };
        for nc in chordwise {
            let mesh = (default_mesh.0, nc);
            println!("{name}: mesh {mesh:?}");
            for (label, plane) in [
                ("planar main wing", &planar),
                ("+ twist", &twisted),
                ("+ camber (as built)", &as_built),
                ("+ stabilizer", &two),
            ] {
                println!(
                    "  {label:22} CL(0) {:+.4}  CL(2) {:+.4}",
                    cl(plane, 0.0, mesh),
                    cl(plane, 2.0, mesh)
                );
            }
        }
    }
}
