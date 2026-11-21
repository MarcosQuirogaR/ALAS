// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing and tail lift-curve slopes and aerodynamic centres at a stated Mach.
//!
//! `cargo run --release -p alas-pipeline --example sm1_tail_probe <mach> [preset ...]`
//! solves the built lifting surfaces on the Goethert-stretched lattice at
//! two angles and prints, per preset: the tail area ratio, the isolated and
//! in-presence tail slopes per reference area, the downwash gradient, the
//! wing-alone slope and aerodynamic centre, the tail aerodynamic centre, and
//! the neutral point of the surface-only combination for a few tail weights.
//! Stations are percent MAC from the leading edge of the MAC.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{
    beta_for_mach, combine, goethert_stretch, per_surface_contributions,
};
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let mach: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(0.2);
    let wanted: Vec<String> = args.collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        if let Err(error) = lines(name, mach) {
            println!("{name}: ERROR {error}");
        }
    }
}

fn lines(name: &str, mach: f64) -> Result<(), String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let plane = goethert_stretch(&report.airplane, beta_for_mach(mach));
    let beta = beta_for_mach(mach);
    let frame = report.airplane.mac_frame().ok_or("no MAC frame")?;
    let (x_le, c) = (frame.x_lemac_m, frame.chord_m);
    let pct = |x: f64| 100.0 * (x * beta - x_le) / c;
    let main = plane
        .wings
        .iter()
        .position(|w| w.name == "Main Wing")
        .ok_or("no main wing")?;
    let tail = plane
        .wings
        .iter()
        .position(|w| w.name == "Horizontal Stabilizer")
        .ok_or("no tail")?;
    let (mesh_y, mesh_x) = (
        config.analysis.fine_spanwise_resolution as usize,
        config.analysis.fine_chordwise_resolution as usize,
    );
    let atmosphere = Atmosphere::new(0.0);
    let v = mach * atmosphere.speed_of_sound();
    let op = |a: f64| OperatingPoint::new(atmosphere, v, a, 0.0, 0.0, 0.0, 0.0);
    let da = 2.0_f64.to_radians();
    let probe = |p: &alas_geom::aircraft::airplane::Airplane| {
        let system = VlmSystem::assemble(p, mesh_y, mesh_x).unwrap();
        let lo = system.solve(&op(-1.0)).unwrap();
        let hi = system.solve(&op(1.0)).unwrap();
        (lo, hi)
    };
    let (lo, hi) = probe(&plane);
    let contributions = per_surface_contributions(&plane, &lo, &hi);
    let q = 0.5 * atmosphere.density() * v * v;
    let s_ref = plane.s_ref;
    let slope = |dl: f64| dl / (da * q * s_ref);
    let a_main = slope(contributions[main].d_lift);
    let a_tail = slope(contributions[tail].d_lift);
    let mut tail_alone = plane.clone();
    tail_alone.wings = vec![plane.wings[tail].clone()];
    tail_alone.fuselages.clear();
    let (tlo, thi) = probe(&tail_alone);
    let iso = per_surface_contributions(&tail_alone, &tlo, &thi);
    let a_tail_iso = slope(iso[0].d_lift);
    let mut wing_alone = plane.clone();
    wing_alone.wings = vec![plane.wings[main].clone()];
    wing_alone.fuselages.clear();
    let (wlo, whi) = probe(&wing_alone);
    let wiso = per_surface_contributions(&wing_alone, &wlo, &whi);
    let a_wing_iso = slope(wiso[0].d_lift);
    let s_tail = report.airplane.wings[tail].projected_area();
    let eps = 1.0 - a_tail / a_tail_iso;
    println!(
        "{name} M{mach}: St/S {:.3} | slope/rad on S_ref: wing in presence {:.3} alone {:.3}, tail in presence {:.3} isolated {:.3} (per tail area {:.2}), eps {:.3} | \
         x_AC wing-in-presence {:.1} wing-alone {:.1} tail {:.1} % | l_t {:.2} MAC | NP surfaces eta 1.0 {:.1} 0.9 {:.1} 0.83 {:.1} 0.745/(a_t) {:.1}",
        s_tail / report.airplane.s_ref,
        a_main,
        a_wing_iso,
        a_tail,
        a_tail_iso,
        a_tail_iso * s_ref / s_tail,
        eps,
        pct(contributions[main].x_ac),
        pct(wiso[0].x_ac),
        pct(contributions[tail].x_ac),
        (contributions[tail].x_ac - contributions[main].x_ac) * beta / c,
        pct(combine(&contributions, Some(tail), 1.0, 0.0).unwrap()),
        pct(combine(&contributions, Some(tail), 0.9, 0.0).unwrap()),
        pct(combine(&contributions, Some(tail), 0.83, 0.0).unwrap()),
        pct(combine(&contributions, Some(tail), 0.745 / a_tail.max(1e-9), 0.0).unwrap()),
    );
    Ok(())
}
