// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trimmed tail load of every preset.
//!
//! `cargo run --release -p alas-pipeline --example sm1_trim_load_probe [preset ...]`
//! trims the built airplane at its report cruise lift on the Goethert-stretched
//! lattice (the compressible trim of the product) and prints: the CG and
//! neutral point (percent MAC), the static margin, the wing-body zero-lift
//! moment `Cm0` and the moment at the trim lift (tail off, body moment included,
//! about the CG), the tail's lift coefficient on its own area and its share of
//! the total lift, the induced drag trimmed and of the tail-off lattice at the
//! same lift, and the difference (the trim drag, counts).
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{beta_for_mach, goethert_stretch};
use alas_stab::trim::stability_and_trim_with_cache;
use serde_json::json;

const HSTAB: &str = "Horizontal Stabilizer";

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        if let Err(error) = lines(name) {
            println!("{name}: ERROR {error}");
        }
    }
}

fn lines(name: &str) -> Result<(), String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let plane = &report.airplane;
    let req = &config.requirements;
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let cl_target = report.design_point.cl;
    let trim = stability_and_trim_with_cache(
        plane,
        &fine,
        cl_target,
        req.cruise_mach,
        req.cruise_altitude_m,
        &Default::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let flight = trim.attitude();
    if !flight.converged || !flight.compressible {
        return Err("compressible trim did not converge".to_owned());
    }
    let beta = beta_for_mach(req.cruise_mach);
    let stretched = goethert_stretch(plane, beta);
    let frame = plane.mac_frame().ok_or("no MAC frame")?;
    let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / frame.chord_m;
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let v = req.cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * v * v;
    let (mesh_y, mesh_x) = (
        fine.fine_spanwise_resolution as usize,
        fine.fine_chordwise_resolution as usize,
    );
    let at = |alpha: f64| OperatingPoint::new(atmosphere, v, alpha, 0.0, 0.0, 0.0, 0.0);
    let set_incidence = |p: &mut Airplane, ih: f64| {
        for wing in p.wings.iter_mut().filter(|w| w.name == HSTAB) {
            for xsec in &mut wing.xsecs {
                xsec.twist = ih;
            }
        }
    };
    let tail_index = plane.wings.iter().position(|w| w.name == HSTAB);
    // Trimmed state: per-wing lift and the total.
    let mut trimmed = stretched.clone();
    set_incidence(&mut trimmed, flight.incidence_deg);
    let system = VlmSystem::assemble(&trimmed, mesh_y, mesh_x).map_err(|e| format!("{e:?}"))?;
    let solved = system
        .solve(&at(flight.alpha_deg))
        .map_err(|e| format!("{e:?}"))?;
    let mut tail_fz = 0.0;
    for (panel, force) in solved.panels.iter().zip(&solved.panel_forces_geometry) {
        if Some(panel.wing_index) == tail_index {
            tail_fz += force[2];
        }
    }
    let s_tail = tail_index.map_or(0.0, |i| plane.wings[i].projected_area());
    let cl_tail_ref = tail_fz / (q * stretched.s_ref);
    let cl_tail_own = if s_tail > 0.0 {
        tail_fz / (q * s_tail)
    } else {
        0.0
    };
    let cm_body = flight.cm_body_per_deg * flight.alpha_deg;
    let cm_trim = beta * solved.cm_pitch + cm_body;
    let cd_i_trim = system
        .trefftz_induced_drag_coefficient(&solved, &at(flight.alpha_deg))
        .ok_or("no Trefftz drag")?;
    // Wing-body (tail off): zero lift moment about the CG and the moment at the trim lift.
    let mut tail_off = stretched.clone();
    tail_off.wings.retain(|w| w.name != HSTAB);
    let off_system =
        VlmSystem::assemble(&tail_off, mesh_y, mesh_x).map_err(|e| format!("{e:?}"))?;
    let off_lo = off_system.solve(&at(0.0)).map_err(|e| format!("{e:?}"))?;
    let off_hi = off_system.solve(&at(4.0)).map_err(|e| format!("{e:?}"))?;
    let cl_slope = (off_hi.cl_lift - off_lo.cl_lift) / 4.0;
    let alpha_lift = (cl_target - off_lo.cl_lift) / cl_slope;
    let off_trim = off_system
        .solve(&at(alpha_lift))
        .map_err(|e| format!("{e:?}"))?;
    let cd_i_off = off_system
        .trefftz_induced_drag_coefficient(&off_trim, &at(alpha_lift))
        .ok_or("no Trefftz drag")?;
    // Zero-lift moment: moment at the angle where the tail-off lift is zero, body term at that angle.
    let alpha_zero = -off_lo.cl_lift / cl_slope;
    let off_zero = off_system
        .solve(&at(alpha_zero))
        .map_err(|e| format!("{e:?}"))?;
    let cm0 = beta * off_zero.cm_pitch + flight.cm_body_per_deg * alpha_zero;
    let cm_wb_trim = beta * off_trim.cm_pitch + flight.cm_body_per_deg * alpha_lift;
    let performance = AeroAnalysis::new(
        plane,
        AeroAnalysis::quarter_chord_sweep_deg(plane, preset.design_vector.sweep_deg),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(fine.clone()),
    );
    let _ = (
        &performance,
        TrimPoint {
            trim_alpha_deg: flight.alpha_deg,
            trim_ih_deg: flight.incidence_deg,
            cl_alpha: flight.cl_alpha,
        },
    );
    println!(
        "{name} (M {:.3}, CL {cl_target:.3}): x_CG {:.1} % NP {:.1} % SM {:.1} % | wing-body Cm0 {cm0:+.4} Cm at trim lift {cm_wb_trim:+.4} (tail off) -> trimmed Cm {cm_trim:+.5} | tail CL {cl_tail_own:+.3} on its area, {cl_tail_ref:+.4} of {:.3} total ({:+.1} % of the lift), ih {:+.2} deg | CDi trimmed {cd_i_trim:.5} tail off {cd_i_off:.5}: trim drag {:+.1} counts",
        req.cruise_mach,
        pct(plane.xyz_ref[0]),
        pct(flight.x_np),
        100.0 * flight.static_margin,
        solved.cl_lift,
        100.0 * cl_tail_ref / solved.cl_lift,
        flight.incidence_deg,
        1.0e4 * (cd_i_trim - cd_i_off),
    );
    Ok(())
}
