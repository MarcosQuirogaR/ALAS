// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compressible lift slope of a preset on the Goethert-stretched lattice
//! against USAF DATCOM 4.1.3.2 and the whole-wing Prandtl-Glauert relabel
//! (A1 evidence).
//!
//! `cargo run --release -p alas-pipeline --example compressible_lift_probe [preset ...]`
//! prints, per Mach, the wing-alone lattice slope ratio `CL_a(M)/CL_a(0)`
//! against the DATCOM ratio and the relabel's `1/beta_w`, and the same ratio
//! for the whole airplane (wing, tail, nacelles' lifting surfaces).
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::analysis::swept_pg_beta;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_stab::compressible_sweep::{compressible_lift_sweep, datcom_slope_ratio};
use serde_json::json;

const ALTITUDE_M: f64 = 11_000.0;
const MACHS: [f64; 8] = [1.0e-6, 0.3, 0.45, 0.5, 0.65, 0.75, 0.8, 0.82];

fn slope(plane: &Airplane, analysis: &alas_config::analysis::AnalysisConfig, mach: f64) -> f64 {
    let sweep = compressible_lift_sweep(plane, analysis, mach, ALTITUDE_M, &[0.0, 4.0]).unwrap();
    (sweep.cl[1] - sweep.cl[0]) / 4.0
}

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&str> = if wanted.is_empty() {
        vec!["A320-200", "B747-400", "ATR72-600"]
    } else {
        presets::available()
            .into_iter()
            .filter(|n| wanted.iter().any(|w| w == n))
            .collect()
    };
    for name in names {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let design = presets::get(name).unwrap().design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let mut analysis = config.analysis.clone();
        analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
        analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
        let main = plane
            .wings
            .iter()
            .find(|w| w.name == "Main Wing")
            .unwrap_or(&plane.wings[0]);
        let aspect = main.aspect_ratio();
        let half = main.mean_sweep_angle(0.5);
        let quarter = main.mean_sweep_angle(0.25);
        let mut wing_only = plane.clone();
        wing_only.wings.retain(|w| w.name == main.name);
        wing_only.fuselages.clear();
        println!("{name}: A {aspect:.2}, sweep c/4 {quarter:.1} deg, c/2 {half:.1} deg");
        println!("  M     wing lattice  DATCOM   relabel 1/bw   airplane lattice");
        let (w0, a0) = (
            slope(&wing_only, &analysis, 1.0e-6),
            slope(&plane, &analysis, 1.0e-6),
        );
        for mach in MACHS {
            let wing = slope(&wing_only, &analysis, mach) / w0;
            let air = slope(&plane, &analysis, mach) / a0;
            let dat = datcom_slope_ratio(aspect, mach, half);
            let relabel = 1.0 / swept_pg_beta(mach, quarter);
            println!("  {mach:.2}  {wing:.4}        {dat:.4}   {relabel:.4}         {air:.4}");
        }
    }
}
