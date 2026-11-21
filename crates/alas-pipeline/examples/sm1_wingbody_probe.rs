// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! DATCOM 4.3.2.2 wing-body aerodynamic centre against the through-body
//! lattice wing.
//!
//! `cargo run --release -p alas-pipeline --example sm1_wingbody_probe <mach> [preset ...]`
//! prints, per preset at the stated Mach (Goethert-stretched lattice), the
//! lattice slope and aerodynamic centre of the wing through the body, of the
//! exposed wing alone, the DATCOM interference factors `K_W(B)`, `K_B(W)`
//! (`d/b` fits of Fig. 4.3.1.2-10), the carryover aerodynamic centre of
//! Eq. 4.3.2.2-c and the neutral point of the DATCOM wing-body assembly with
//! the current Multhopp and nacelle moments, in percent MAC.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{
    beta_for_mach, goethert_stretch, neutral_point_conditions, per_surface_contributions,
    NpConditionsInput,
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

/// `wing` with everything inboard of `y_cut` removed: a new root section at
/// `y_cut` interpolated between the bounding sections.
fn clip_root(wing: &Wing, y_cut: f64) -> Wing {
    let mut xsecs: Vec<WingXSec> = Vec::new();
    for pair in wing.xsecs.windows(2) {
        let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
        if xsecs.is_empty() {
            if y1 <= y_cut {
                continue;
            }
            let t = ((y_cut - y0) / (y1 - y0)).clamp(0.0, 1.0);
            let lerp = |a: f64, b: f64| a + t * (b - a);
            let mut root = pair[1].clone();
            root.xyz_le = [
                lerp(pair[0].xyz_le[0], pair[1].xyz_le[0]),
                pair[0].xyz_le[1].signum().max(1.0) * y_cut,
                lerp(pair[0].xyz_le[2], pair[1].xyz_le[2]),
            ];
            root.chord = lerp(pair[0].chord, pair[1].chord);
            root.twist = lerp(pair[0].twist, pair[1].twist);
            xsecs.push(root);
        }
        xsecs.push(pair[1].clone());
    }
    Wing {
        xsecs,
        ..wing.clone()
    }
}

/// DATCOM Fig. 4.3.1.2-10 interference factors against `k = d/b`.
fn k_factors(k: f64) -> (f64, f64) {
    (
        0.1714 * k * k + 0.8326 * k + 0.9974,
        0.7810 * k * k + 1.1976 * k + 0.0088,
    )
}

/// DATCOM Eq. 4.3.2.2-c bracket, `beta A_e >= 4`.
fn carryover_bracket(k: f64) -> f64 {
    let root = (1.0 - 2.0 * k).sqrt();
    let log = ((1.0 - k) / k + root / k).ln();
    let numerator = root * log - (1.0 - k) + std::f64::consts::FRAC_PI_2 * k;
    let denominator = k * (1.0 - k) / root * log + (1.0 - k).powi(2) / k
        - std::f64::consts::FRAC_PI_2 * (1.0 - k);
    -k / (1.0 - k) + numerator / denominator
}

fn lines(name: &str, mach: f64) -> Result<(), String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let beta = beta_for_mach(mach);
    let physical = &report.airplane;
    let plane = goethert_stretch(physical, beta);
    let frame = physical.mac_frame().ok_or("no MAC frame")?;
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
        .position(|w| w.name == "Horizontal Stabilizer");
    let (mesh_y, mesh_x) = (
        config.analysis.fine_spanwise_resolution as usize,
        config.analysis.fine_chordwise_resolution as usize,
    );
    let atmosphere = Atmosphere::new(0.0);
    let v = mach.max(0.05) * atmosphere.speed_of_sound();
    let op = |a: f64| OperatingPoint::new(atmosphere, v, a, 0.0, 0.0, 0.0, 0.0);
    let da = 2.0_f64.to_radians();
    let q = 0.5 * atmosphere.density() * v * v;
    let slope = |dl: f64| dl / (da * q * plane.s_ref);
    let probe = |p: &Airplane| {
        let system = VlmSystem::assemble(p, mesh_y, mesh_x).unwrap();
        let lo = system.solve(&op(-1.0)).unwrap();
        let hi = system.solve(&op(1.0)).unwrap();
        per_surface_contributions(p, &lo, &hi)
    };
    let system_full = VlmSystem::assemble(&plane, mesh_y, mesh_x).unwrap();
    let lo_full = system_full.solve(&op(-1.0)).unwrap();
    let hi_full = system_full.solve(&op(1.0)).unwrap();
    let full = per_surface_contributions(&plane, &lo_full, &hi_full);
    let (a_through, x_through) = (slope(full[main].d_lift), full[main].x_ac);
    let (a_tail, x_tail) = tail.map_or((0.0, 0.0), |t| (slope(full[t].d_lift), full[t].x_ac));

    // Body width at the wing root and the exposed wing.
    let fus = physical.fuselages.first().ok_or("no fuselage")?;
    let wing = &plane.wings[main];
    let x_root_c4 = wing.xsecs[0].xyz_le[0] + 0.25 * wing.xsecs[0].chord;
    let d = fus
        .xsecs
        .iter()
        .min_by(|a, b| {
            (a.xyz_c[0] * 1.0 / beta - x_root_c4 / beta)
                .abs()
                .total_cmp(&(b.xyz_c[0] / beta - x_root_c4 / beta).abs())
        })
        .map(|s| s.width)
        .unwrap();
    let b = 2.0
        * wing
            .xsecs
            .iter()
            .map(|s| s.xyz_le[1].abs())
            .fold(0.0, f64::max);
    let k = d / b;
    let exposed_wing = clip_root(wing, 0.5 * d);
    let mut exposed = plane.clone();
    exposed.wings = vec![exposed_wing.clone()];
    exposed.fuselages.clear();
    let e = probe(&exposed);
    let (a_e, x_e) = (slope(e[0].d_lift), e[0].x_ac);
    let (k_w, k_b) = k_factors(k);
    let root = &exposed_wing.xsecs[0];
    let c_re = root.chord;
    let tan_c4 = exposed_wing.mean_sweep_angle(0.25).to_radians().tan();
    let x_carry =
        root.xyz_le[0] + c_re * (0.25 + (b - d) / (2.0 * c_re) * tan_c4 * carryover_bracket(k));
    let a_exposed_area = exposed_wing.projected_area();
    let beta_ae = beta * b * b / (2.0 * a_exposed_area.max(1.0) + 1.0e-9) * 1.0;

    // Current model's airframe moments.
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let input = NpConditionsInput {
        low_speed_mach: mach,
        low_speed_altitude_m: 0.0,
        cruise_mach: mach,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        ..NpConditionsInput::default()
    };
    let np = neutral_point_conditions(physical, &fine, &input).map_err(|e| format!("{e:?}"))?;
    let d_old = &np.diagnostics;
    let cla_old = d_old.cl_alpha_total;
    let airframe_m = d_old.fuselage.multhopp_shift_m + d_old.nacelle_shift_m;
    let eta = config
        .analysis
        .tail_efficiency_or(physical.horizontal_tail_dynamic_pressure_ratio());
    let a_w = k_w * a_e;
    let a_c = k_b * a_e;
    let den = a_w + a_c + eta * a_tail;
    let num = a_w * x_e + a_c * x_carry + eta * a_tail * x_tail;
    let x_np_surfaces = num / den;
    let x_np = x_np_surfaces + airframe_m / beta * cla_old / den;
    let x_wb = (a_w * x_e + a_c * x_carry) / (a_w + a_c);
    // Hybrid: the lattice wing through the body, plus the DATCOM carryover on
    // the body (K_B(W) x the exposed panels' lift at Eq. 4.3.2.2-c) and the
    // body-induced exposed-wing lift (K_W(B) - 1), the Torenbeek D2 dropped.
    let y_cut = 0.5 * d;
    let (mut dl_e, mut dm_e) = (0.0_f64, 0.0_f64);
    for ((panel, f_lo), f_hi) in lo_full
        .panels
        .iter()
        .zip(lo_full.panel_forces_geometry.iter())
        .zip(hi_full.panel_forces_geometry.iter())
    {
        if panel.wing_index != main || panel.vortex_center[1].abs() < y_cut {
            continue;
        }
        let dx = panel.vortex_center[0] - plane.xyz_ref[0];
        let dz = panel.vortex_center[2] - plane.xyz_ref[2];
        let (dfx, dfz) = (f_hi[0] - f_lo[0], f_hi[2] - f_lo[2]);
        dl_e += dfz;
        dm_e += dz * dfx - dx * dfz;
    }
    let (a_ex, x_ex) = (slope(dl_e), plane.xyz_ref[0] - dm_e / dl_e);
    let a_extra_w = (k_w - 1.0) * a_ex;
    let a_extra_c = k_b * a_ex;
    let den_h = a_through + a_extra_w + a_extra_c + eta * a_tail;
    let num_h =
        a_through * x_through + a_extra_w * x_ex + a_extra_c * x_carry + eta * a_tail * x_tail;
    let x_np_hybrid = num_h / den_h + airframe_m / beta * cla_old / den_h;
    let x_np_hybrid_with_d2 = x_np_hybrid
        + d_old.fuselage.torenbeek_d2_shift_m / beta
            * d_old.wing_lift_fraction.clamp(0.0, 1.0)
            * cla_old
            / den_h;
    println!(
        "{name} M{mach} HYBRID: exposed panels slope {a_ex:.2} AC {:.1} % carryover AC {:.1} % | NP {:.1} % (with Torenbeek D2 {:.1} %) | current model NP {:.1} % | wing-body AC {:.1} %",
        pct(x_ex),
        pct(x_carry),
        pct(x_np_hybrid),
        pct(x_np_hybrid_with_d2),
        100.0 * (np.cruise - frame.x_lemac_m) / c,
        pct((a_through * x_through + a_extra_w * x_ex + a_extra_c * x_carry) / (a_through + a_extra_w + a_extra_c)),
    );
    println!(
        "{name} M{mach}: d/b {k:.3} Ke/S {:.2} | lattice slope through {a_through:.2} AC {:.1} % | exposed {a_e:.2} AC {:.1} % | K_W(B) {k_w:.3} K_B(W) {k_b:.3} carryover AC {:.1} % | wing-body slope {:.2} AC {:.1} % | tail {a_tail:.2}*{eta} AC {:.1} % | NP surfaces {:.1} + airframe moments {:+.1} = {:.1} % | current model NP {:.1} % | beta*A_e~{beta_ae:.1}",
        a_exposed_area / (physical.wings[main].projected_area()),
        pct(x_through),
        pct(x_e),
        pct(x_carry),
        a_w + a_c,
        pct(x_wb),
        pct(x_tail),
        pct(x_np_surfaces),
        100.0 * airframe_m * cla_old / den / c,
        pct(x_np),
        100.0 * (np.cruise - frame.x_lemac_m) / c,
    );
    Ok(())
}
