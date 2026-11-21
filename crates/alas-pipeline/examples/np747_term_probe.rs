// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The neutral-point chain and its inputs per preset.
//!
//! `cargo run --release -p alas-pipeline --example np747_term_probe [preset ...]`
//! evaluates the clean, rigid neutral point at Mach 0.2 (or `ALAS_PROBE_MACH`), sea level (the NASA
//! CR-2144 747 comparison condition) on the built airplane and prints, in
//! percent MAC from LEMAC: the wing-alone lattice aerodynamic centre, the
//! surfaces-only neutral point, the fuselage terms (Multhopp strip and
//! Torenbeek D2, summed; Torenbeek D1 and Gilruth, cross-checks only), the
//! nacelle term and the neutral point; then the inputs of the Torenbeek
//! nacelle term (Eq. E-41, printed p. 481) per nacelle: inlet station, maximum
//! width `b_n`, length, arm `l_n` from the inlet to 1/4 MAC, each in MAC.
#![allow(clippy::print_stdout, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput};
use serde_json::json;

#[path = "support/np_terms.rs"]
mod np_terms;

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
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    // `ALAS_PROBE_MACH` overrides the Mach 0.2 comparison condition.
    let mach = std::env::var("ALAS_PROBE_MACH")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.2);
    // `ALAS_PROBE_NO_INLET` drops the jet-inlet normal force.
    let jet_inlets = if std::env::var_os("ALAS_PROBE_NO_INLET").is_some() {
        None
    } else {
        alas_opt::envelope::jet_inlet_spec(
            &config,
            alas_opt::envelope::InletDragPolar {
                cd0: report.polar_fit.cd0,
                k: report.polar_fit.k,
            },
            plane.s_ref,
        )
    };
    let input = NpConditionsInput {
        low_speed_mach: mach,
        low_speed_altitude_m: 0.0,
        cruise_mach: config.requirements.cruise_mach,
        cruise_altitude_m: config.requirements.cruise_altitude_m,
        high_lift: alas_stab::neutral_point::HighLiftDevices::declared(
            config.performance.high_lift_chord_extension,
            &config.control_surfaces,
        ),
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        propeller: None,
        jet_inlets,
        max_operating: config.max_operating_condition(),
    };
    let np = neutral_point_conditions(plane, &fine, &input).map_err(|e| format!("{e:?}"))?;
    let d = &np.diagnostics;
    let frame = plane.mac_frame().ok_or("no MAC frame")?;
    let c = frame.chord_m;
    let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / c;
    let dpct = |dx: f64| 100.0 * dx / c;
    let f = &d.fuselage;
    println!(
        "{name}: MAC {c:.3} m, S {:.1} m2, CLa {:.3}/rad, wing share {:.3}, deps/da {:.3} | wing AC {:.1} surfaces NP {:.1} | Multhopp {:+.1} D2 {:+.1} (x share) | D1 {:+.1} Gilruth {:+.1} (not summed) | tunnel {:+.1} (x share, flagged) | nacelles {:+.1} | NP {:.1} (eta {:.3}) | cruise M{:.2} NP {:.1}",
        plane.s_ref,
        d.cl_alpha_total,
        d.wing_lift_fraction,
        d.deps_dalpha_vlm,
        pct(d.x_wing_alone_ac),
        pct(d.x_np_surfaces),
        dpct(f.multhopp_shift_m),
        dpct(d.wing_lift_fraction * f.torenbeek_d2_shift_m),
        dpct(f.torenbeek_d1_shift_m),
        dpct(f.gilruth_shift_m),
        dpct(d.wing_lift_fraction * f.tunnel_correction_shift_m),
        dpct(d.nacelle_shift_m),
        pct(np.clean_low_speed),
        fine.tail_efficiency_or(plane.horizontal_tail_dynamic_pressure_ratio()),
        config.requirements.cruise_mach,
        pct(np.cruise),
    );
    let flow = |c: Option<alas_stab::neutral_point::InletCondition>| {
        c.map_or_else(
            || "-".to_owned(),
            |c| format!("{:.0} kg/s M{:.2}", c.mass_flow_kg_s, c.mach),
        )
    };
    println!(
        "  Inlet: climb {} | approach {} | cruise {} | max-op {} || NP climb {:.1} approach {:.1} cruise {:.1} max-op {} critical {:.1}",
        flow(jet_inlets.map(|s| s.climb)),
        flow(jet_inlets.map(|s| s.approach)),
        flow(jet_inlets.map(|s| s.cruise)),
        flow(jet_inlets.and_then(|s| s.max_operating)),
        pct(np.clean_low_speed),
        pct(np.high_lift),
        pct(np.cruise),
        np.max_operating
            .map_or_else(|| "-".to_owned(), |(_, rigid)| format!("{:.1}", pct(rigid))),
        pct(np.critical),
    );
    if let (Some(w), Some(t)) = (
        plane.wings.iter().find(|w| w.name == "Main Wing"),
        plane
            .wings
            .iter()
            .find(|w| w.name == "Horizontal Stabilizer"),
    ) {
        println!(
            "  Chords: reference {:.3} m, lofted MAC {:.3} m, trapezoid (outer panel to root) {:.3} m",
            plane.c_ref,
            w.mean_aerodynamic_chord(),
            w.theoretical_reference_mac(),
        );
        let (st, ct) = (t.projected_area(), t.mean_aerodynamic_chord());
        let (xw, xt) = (w.aerodynamic_center(0.25)[0], t.aerodynamic_center(0.25)[0]);
        println!(
            "  Tail: S_h {:.1} m2 ({:.0} ft2), S_h/S {:.3}, MAC_h {:.2} m, wing c/4 x {:.2} m, tail c/4 x {:.2} m, arm {:.2} m ({:.2} c), V_h {:.3}",
            st,
            st * 10.7639,
            st / plane.s_ref,
            ct,
            xw,
            xt,
            xt - xw,
            (xt - xw) / c,
            st * (xt - xw) / (plane.s_ref * c),
        );
        // DATCOM downwash gradient (Roskam VI Eqs. 8.45-8.48, p. 272), for
        // comparison with the lattice: K_A, K_lambda, K_h from the planform
        // and the tail position over the wing root chord plane.
        let aspect = plane.b_ref * plane.b_ref / plane.s_ref;
        let root = &w.xsecs[0];
        let tip = &w.xsecs[w.xsecs.len() - 1];
        let taper = tip.chord / root.chord;
        let sweep = alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(plane, 25.0)
            .to_radians();
        let h_h = t.aerodynamic_center(0.25)[2] - (root.xyz_le[2]);
        let k_a = 1.0 / aspect - 1.0 / (1.0 + aspect.powf(1.7));
        let k_l = (10.0 - 3.0 * taper) / 7.0;
        let k_h = (1.0 - (h_h / plane.b_ref).abs()) / (2.0 * (xt - xw) / plane.b_ref).cbrt();
        let datcom = 4.44 * (k_a * k_l * k_h * sweep.cos().sqrt()).powf(1.19);
        println!(
            "  Downwash: lattice {:.3} DATCOM {:.3} (A {:.2}, taper {:.3}, sweep {:.1} deg, h_h {:+.2} m, 2l_h/b {:.3})",
            d.deps_dalpha_vlm,
            datcom,
            aspect,
            taper,
            sweep.to_degrees(),
            h_h,
            2.0 * (xt - xw) / plane.b_ref,
        );
    }
    // The term-by-term decomposition, same terms as the RM A58H12 probe.
    let terms = np_terms::decompose(plane, &fine, mach, frame.x_lemac_m, c);
    println!(
        "{}",
        np_terms::line(&format!("  {name} decomposition"), &terms)
    );
    let x_c4 = frame.x_lemac_m + 0.25 * c;
    for (nacelle, term) in plane.fuselages[1..].iter().zip(&d.nacelle_terms) {
        let first = nacelle.xsecs.first().ok_or("empty nacelle")?;
        let last = nacelle.xsecs.last().ok_or("empty nacelle")?;
        let b_n = nacelle
            .xsecs
            .iter()
            .map(|x| x.width)
            .fold(0.0_f64, f64::max);
        println!(
            "  {}: inlet x {:.2} m y {:+.2} m, b_n {:.2} m ({:.3} c), length {:.2} m ({:.3} c), l_n {:+.2} m ({:+.3} c), k_n {:.1}, shift {:+.2} %MAC",
            nacelle.name,
            first.xyz_c[0],
            first.xyz_c[1],
            b_n,
            b_n / c,
            last.xyz_c[0] - first.xyz_c[0],
            (last.xyz_c[0] - first.xyz_c[0]) / c,
            x_c4 - first.xyz_c[0],
            (x_c4 - first.xyz_c[0]) / c,
            term.k_n,
            dpct(term.shift_m),
        );
    }
    Ok(())
}
