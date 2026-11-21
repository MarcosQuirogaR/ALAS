// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rigid neutral-point chain per preset.
//!
//! `cargo run --release -p alas-pipeline --example sm1_neutral_point_probe [preset ...]`
//! prints, at the preset's cruise condition: the physical CG, the mean-chord
//! frame, the wing-alone aerodynamic centre, the surface-only neutral point,
//! the fuselage and nacelle shifts, the downwash gradient, the wing lift share,
//! the final neutral point and the static margins, all in fractions of the mean
//! aerodynamic chord measured from the leading edge of the MAC.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput};
use serde_json::json;

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
    let mut value = json!({"preset": name});
    // `SM1_ETA=0.745` overrides the tail dynamic-pressure efficiency.
    if let Some(eta) = std::env::var("SM1_ETA")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        value["analysis"] = json!({"tail_efficiency": eta});
    }
    let config = AlasConfig::from_value(&value).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let plane = &report.airplane;
    let frame = plane.mac_frame().ok_or("no MAC frame")?;
    let (x_le, c) = (frame.x_lemac_m, frame.chord_m);
    let pct = |x: f64| 100.0 * (x - x_le) / c;
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let input = NpConditionsInput {
        cruise_mach: config.requirements.cruise_mach,
        cruise_altitude_m: config.requirements.cruise_altitude_m,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        ..NpConditionsInput::default()
    };
    let np = neutral_point_conditions(plane, &fine, &input).map_err(|e| format!("{e:?}"))?;
    let cruise = {
        let mut only = input.clone();
        only.low_speed_mach = input.cruise_mach;
        only.low_speed_altitude_m = input.cruise_altitude_m;
        neutral_point_conditions(plane, &fine, &only).map_err(|e| format!("{e:?}"))?
    };
    let d = &cruise.diagnostics;
    let x_cg = plane.xyz_ref[0];
    println!(
        "{name}: cruise M {:.3} MAC {:.3} m, x_CG {:.1} %, wing-alone AC {:.1} %, surfaces NP {:.1} %, \
         fuselage Multhopp {:+.1} D2 {:+.1} (D1 {:+.1}, Gilruth {:+.1}) %, nacelles {:+.1} %, \
         NP {:.1} % | SM {:.1} % | eps {:.3} wing share {:.3} CLa {:.2}/rad | lowspeed NP {:.1} %",
        input.cruise_mach,
        c,
        pct(x_cg),
        pct(d.x_wing_alone_ac),
        pct(d.x_np_surfaces),
        100.0 * d.fuselage.multhopp_shift_m / c,
        100.0 * d.fuselage.torenbeek_d2_shift_m / c,
        100.0 * d.fuselage.torenbeek_d1_shift_m / c,
        100.0 * d.fuselage.gilruth_shift_m / c,
        100.0 * d.nacelle_shift_m / c,
        pct(cruise.cruise),
        100.0 * (cruise.cruise - x_cg) / c,
        d.deps_dalpha_vlm,
        d.wing_lift_fraction,
        d.cl_alpha_total,
        pct(np.clean_low_speed),
    );
    // The conditions the CG envelope reads: clean low speed, flexible cruise, high lift.
    let conditions = NpConditionsInput {
        cruise_mach: config.requirements.cruise_mach,
        cruise_altitude_m: config.requirements.cruise_altitude_m,
        high_lift: alas_stab::neutral_point::HighLiftDevices::declared(
            config.performance.high_lift_chord_extension,
            &config.control_surfaces,
        ),
        ..NpConditionsInput::default()
    };
    let all = neutral_point_conditions(plane, &fine, &conditions).map_err(|e| format!("{e:?}"))?;
    println!(
        "{name}: conditions: clean low {:.1} cruise {:.1} flexible cruise {:.1} high lift {:.1} critical {:.1} % MAC (CG {:.1} %)",
        pct(all.clean_low_speed),
        pct(all.cruise),
        pct(all.elastic_band.0),
        pct(all.high_lift),
        pct(all.critical),
        pct(plane.xyz_ref[0])
    );
    println!(
        "{name}: report SM {:.1} % (x_NP {:.1} %)",
        100.0 * report.static_margin,
        pct(report.x_neutral_point)
    );
    Ok(())
}
