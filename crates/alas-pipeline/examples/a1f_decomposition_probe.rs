// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Decomposition of the compressible induced-drag rise at fixed lift (A1F).
//!
//! `cargo run --release -p alas-pipeline --example a1f_decomposition_probe <preset> [...]`
//! prints, for each preset, the Trefftz induced-drag coefficient at a fixed
//! `CL` on the incompressible lattice and on the Goethert-stretched lattice at
//! the preset's cruise Mach, for a ladder of configurations that adds one
//! effect at a time:
//!
//! 1. the main wing, planar (zero twist, symmetric NACA 0012 sections);
//! 2. + the built twist;
//! 3. + the built camber (the wing as built);
//! 4. + the horizontal stabilizer (and any other lifting surface) at its registered incidence, untrimmed;
//! 5. the same trimmed: the stabilizer incidence and body angle that give `Cm = 0` about the real centre of gravity at the target `CL`.
//!
//! The ratio of the two lattices per rung attributes the rise.
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
use alas_stab::neutral_point::{beta_for_mach, goethert_stretch};
use serde_json::json;

const HSTAB: &str = "Horizontal Stabilizer";
const TARGET_CL: f64 = 0.5;
const INCOMPRESSIBLE_MACH: f64 = 1.0e-4;

struct Solved {
    cl: f64,
    cm: f64,
    cdi: f64,
}

fn solve(plane: &Airplane, mach: f64, altitude_m: f64, alpha: f64, mesh: (usize, usize)) -> Solved {
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let system = VlmSystem::assemble(plane, mesh.0, mesh.1).unwrap();
    let op = OperatingPoint::new(atmosphere, velocity, alpha, 0.0, 0.0, 0.0, 0.0);
    let result = system.solve(&op).unwrap();
    Solved {
        cl: result.cl_lift,
        cm: result.cm_pitch,
        cdi: system
            .trefftz_induced_drag_coefficient(&result, &op)
            .unwrap(),
    }
}

/// `CDi` at `TARGET_CL` of an untrimmed configuration: quadratic Lagrange
/// interpolation over three angles.
fn cdi_untrimmed(plane: &Airplane, mach: f64, altitude_m: f64, mesh: (usize, usize)) -> f64 {
    let points: Vec<Solved> = [-2.0, 2.0, 6.0]
        .iter()
        .map(|&alpha| solve(plane, mach, altitude_m, alpha, mesh))
        .collect();
    (0..3)
        .map(|i| {
            let weight: f64 = (0..3)
                .filter(|&j| j != i)
                .map(|j| (TARGET_CL - points[j].cl) / (points[i].cl - points[j].cl))
                .product();
            weight * points[i].cdi
        })
        .sum()
}

fn with_stabilizer_incidence(plane: &Airplane, incidence_deg: f64) -> Airplane {
    let mut out = plane.clone();
    for wing in out.wings.iter_mut().filter(|w| w.name == HSTAB) {
        for xsec in &mut wing.xsecs {
            xsec.twist = incidence_deg;
        }
    }
    out
}

/// `CDi` at `TARGET_CL` and `Cm = 0` about `plane.xyz_ref`: Newton on the
/// body angle and the stabilizer incidence, three solves for the Jacobian
/// (the lattice is linear in both) and one at the answer.
fn cdi_trimmed(
    plane: &Airplane,
    mach: f64,
    altitude_m: f64,
    mesh: (usize, usize),
) -> (f64, f64, f64) {
    let at = |alpha: f64, ih: f64| {
        solve(
            &with_stabilizer_incidence(plane, ih),
            mach,
            altitude_m,
            alpha,
            mesh,
        )
    };
    let (a0, i0) = (2.0, 0.0);
    let base = at(a0, i0);
    let da = at(a0 + 1.0, i0);
    let di = at(a0, i0 + 1.0);
    let (cl_a, cm_a) = (da.cl - base.cl, da.cm - base.cm);
    let (cl_i, cm_i) = (di.cl - base.cl, di.cm - base.cm);
    let det = cl_a * cm_i - cl_i * cm_a;
    let (rl, rm) = (TARGET_CL - base.cl, -base.cm);
    let alpha = a0 + (rl * cm_i - cl_i * rm) / det;
    let ih = i0 + (cl_a * rm - rl * cm_a) / det;
    let end = at(alpha, ih);
    (end.cdi, alpha, ih)
}

fn main() {
    for name in std::env::args().skip(1) {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let design = presets::get(&name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap();
        let (mach, altitude_m) = (
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        );
        let mesh = (
            config.analysis.fine_spanwise_resolution as usize,
            config.analysis.fine_chordwise_resolution as usize,
        );
        let built = report.airplane.clone();
        let main_index = (0..built.wings.len())
            .filter(|&i| built.wings[i].name != HSTAB)
            .max_by(|&i, &j| {
                built.wings[i]
                    .projected_area()
                    .total_cmp(&built.wings[j].projected_area())
            })
            .unwrap();
        let mut fuselage_free = built.clone();
        fuselage_free.fuselages.clear();
        fuselage_free.xyz_ref = report.physical_cg;

        let mut main_only = fuselage_free.clone();
        main_only.wings = vec![fuselage_free.wings[main_index].clone()];
        let mut planar = main_only.clone();
        for xsec in &mut planar.wings[0].xsecs {
            xsec.twist = 0.0;
            xsec.airfoil = Airfoil::from_name("naca0012").unwrap();
        }
        let mut twisted = main_only.clone();
        for (xsec, source) in twisted.wings[0]
            .xsecs
            .iter_mut()
            .zip(&main_only.wings[0].xsecs)
        {
            xsec.twist = source.twist;
            xsec.airfoil = Airfoil::from_name("naca0012").unwrap();
        }
        let beta = beta_for_mach(mach);
        println!("{name}: cruise Mach {mach} beta {beta:.4}, mesh {mesh:?}, CL {TARGET_CL}");
        let rungs: [(&str, &Airplane, bool); 5] = [
            ("1 planar main wing", &planar, false),
            ("2 + twist", &twisted, false),
            ("3 + camber (wing as built)", &main_only, false),
            ("4 + tail at registered incidence", &fuselage_free, false),
            ("5 trimmed (Cm=0 about the CG)", &fuselage_free, true),
        ];
        for (label, plane, trimmed) in rungs {
            let stretched = goethert_stretch(plane, beta);
            let (inc, comp, note) = if trimmed {
                let (inc, a0, i0) = cdi_trimmed(plane, INCOMPRESSIBLE_MACH, altitude_m, mesh);
                let (comp, a1, i1) = cdi_trimmed(&stretched, mach, altitude_m, mesh);
                (
                    inc,
                    comp,
                    format!("alpha {a0:.2} -> {a1:.2} deg, ih {i0:.2} -> {i1:.2} deg"),
                )
            } else {
                (
                    cdi_untrimmed(plane, INCOMPRESSIBLE_MACH, altitude_m, mesh),
                    cdi_untrimmed(&stretched, mach, altitude_m, mesh),
                    String::new(),
                )
            };
            println!(
                "  {label:34} CDi {inc:.5} -> {comp:.5}  x{:.3}  {note}",
                comp / inc
            );
        }
    }
}
