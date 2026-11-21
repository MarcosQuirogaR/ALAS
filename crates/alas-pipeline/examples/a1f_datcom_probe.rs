// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lattice versus DATCOM 4.1.3.2 Method 1 lift-slope ratio: mesh convergence on
//! a straight-tapered wing, and the effect of the midchord-sweep definition on
//! cranked preset wings (A1F evidence).
//!
//! `cargo run --release -p alas-pipeline --example a1f_datcom_probe <preset> [...]`
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::compressible_sweep::{compressible_lift_sweep, datcom_slope_ratio};
use serde_json::json;

fn trapezoid(semi: f64, root: f64, tip: f64, midchord_sweep_deg: f64) -> Airplane {
    let af = || Airfoil::from_name("naca2412").unwrap();
    let tip_le = semi * midchord_sweep_deg.to_radians().tan() + 0.5 * root - 0.5 * tip;
    let main = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], root, 0.0, af()),
            WingXSec::new([tip_le, semi, 0.0], tip, 0.0, af()),
        ],
        true,
    );
    let (s_ref, b_ref, c_ref) = (
        main.reference_area(),
        main.reference_span(),
        main.mean_aerodynamic_chord(),
    );
    Airplane {
        name: "w".into(),
        xyz_ref: [0.2 * 2.0 * semi, 0.0, 0.0],
        wings: vec![main],
        fuselages: vec![],
        s_ref,
        c_ref,
        b_ref,
    }
}

fn ratio(plane: &Airplane, analysis: &alas_config::AnalysisConfig, mach: f64) -> f64 {
    let slope = |m: f64| {
        let s = compressible_lift_sweep(plane, analysis, m, 11_000.0, &[0.0, 4.0]).unwrap();
        (s.cl[1] - s.cl[0]) / 4.0
    };
    slope(mach) / slope(1.0e-6)
}

/// Area-weighted midchord sweep of the segments, degrees.
fn weighted_midchord_sweep_deg(wing: &Wing) -> f64 {
    let (mut sum, mut area) = (0.0, 0.0);
    for pair in wing.xsecs.windows(2) {
        let dy = ((pair[1].xyz_le[1] - pair[0].xyz_le[1]).powi(2)
            + (pair[1].xyz_le[2] - pair[0].xyz_le[2]).powi(2))
        .sqrt();
        if dy <= 0.0 {
            continue;
        }
        let mid = |x: &WingXSec| x.xyz_le[0] + 0.5 * x.chord;
        let sweep = ((mid(&pair[1]) - mid(&pair[0])) / dy).atan().to_degrees();
        let segment_area = 0.5 * (pair[0].chord + pair[1].chord) * dy;
        sum += sweep * segment_area;
        area += segment_area;
    }
    sum / area
}

fn main() {
    println!("mesh convergence, straight-tapered wing A 9.5, midchord sweep 19.6 deg, M 0.75 (span x chord multipliers)");
    for taper in [0.1, 0.28] {
        let root = 7.16 / (1.0 + taper);
        let plane = trapezoid(17.0, root, taper * root, 19.6);
        let wing = &plane.wings[0];
        let datcom = datcom_slope_ratio(wing.aspect_ratio(), 0.75, wing.mean_sweep_angle(0.5));
        for (span, chord) in [(1, 4), (1, 8), (1, 16), (2, 16), (1, 32), (2, 32)] {
            let analysis = alas_config::AnalysisConfig {
                spanwise_resolution: span,
                chordwise_resolution: chord,
                ..alas_config::AnalysisConfig::default()
            };
            let r = ratio(&plane, &analysis, 0.75);
            println!("  taper {taper:.2} mesh ({span},{chord:2}): lattice {r:.4} DATCOM {datcom:.4} dev {:+.2} %", 100.0 * (r / datcom - 1.0));
        }
    }
    println!("presets: wing alone at cruise Mach, mean_sweep_angle(0.5) vs area-weighted segment midchord sweep");
    for name in std::env::args().skip(1) {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let design = presets::get(&name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap();
        let mut plane = report.airplane.clone();
        plane.fuselages.clear();
        let idx = (0..plane.wings.len())
            .filter(|&i| plane.wings[i].name != "Horizontal Stabilizer")
            .max_by(|&i, &j| {
                plane.wings[i]
                    .projected_area()
                    .total_cmp(&plane.wings[j].projected_area())
            })
            .unwrap();
        plane.wings = vec![plane.wings[idx].clone()];
        let wing = &plane.wings[0];
        let mach = config.requirements.cruise_mach;
        let mut analysis = config.analysis.clone();
        analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
        analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
        let r = ratio(&plane, &analysis, mach);
        let (a, mean) = (wing.aspect_ratio(), wing.mean_sweep_angle(0.5));
        let weighted = weighted_midchord_sweep_deg(wing);
        let d_mean = datcom_slope_ratio(a, mach, mean);
        let d_weighted = datcom_slope_ratio(a, mach, weighted);
        println!(
            "  {name} M {mach}: A {a:.2}, {} segments, sweep mean {mean:.1} / weighted {weighted:.1} deg; lattice {r:.4}; DATCOM mean {d_mean:.4} ({:+.2} %), weighted {d_weighted:.4} ({:+.2} %)",
            wing.xsecs.len() - 1,
            100.0 * (r / d_mean - 1.0),
            100.0 * (r / d_weighted - 1.0)
        );
    }
}
