// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing-body neutral-point evaluation per preset: tail-off wing-body
//! aerodynamic centre `h_wb` and lift slope, and the stick-fixed neutral
//! point, at Mach 0.2 sea level on the fine mesh. Heights are fractions of the
//! reference MAC from its LEMAC.
//!
//! `cargo run --release -p alas-pipeline --example np_wingbody_eval [preset ...]`
//!
//! Tail-off `h_wb` is the moment balance of the wing alone (lattice a.c.), the
//! extra wing-body lift `(K_I - 1) a_w` at the exposed-root mid-chord and the
//! fuselage + nacelle shifts (which the product normalises by the total
//! slope, hence multiplied back by it): `h_wb = [a_w h_w + (K_I-1) a_w h_x +
//! a_tot (F + N)] / (K_I a_w)`. With the legacy terms (`F = D1 + share D2`)
//! this differs from the raw-D2 tail-off of the R15 probe by the share on D2.
#![allow(clippy::print_stdout, missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput};
use serde_json::json;

#[path = "support/np_terms.rs"]
mod np_terms;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    println!("preset,a_w,K_I,a_wb,h_w_iso,h_wb,a_tot,share,np,d1,d2raw,mult,nac");
    let mut names: Vec<&str> = presets::available().to_vec();
    names.push("B747-100like");
    for name in names {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        match run(name) {
            Ok(l) => println!("{l}"),
            Err(e) => println!("{name},ERROR {e}"),
        }
    }
}

fn run(label: &str) -> Result<String, String> {
    // "B747-100like": the -400 preset with the wing tip cut at semispan
    // 29.82 m (S_ref 511, b_ref 59.64, as the R15 probe), the frame held at the
    // -400's own MAC/LEMAC.
    let name = if label == "B747-100like" {
        "B747-400"
    } else {
        label
    };
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let mut owned = report.airplane.clone();
    let frame0 = owned.mac_frame().ok_or("no frame")?;
    if label == "B747-100like" {
        let wing = owned
            .wings
            .iter_mut()
            .find(|w| w.name == "Main Wing")
            .unwrap();
        let n = wing.xsecs.len();
        let (a, b) = (wing.xsecs[n - 2].clone(), wing.xsecs[n - 1].clone());
        let t = (29.82 - a.xyz_le[1]) / (b.xyz_le[1] - a.xyz_le[1]);
        let mut tip = a.clone();
        for k in 0..3 {
            tip.xyz_le[k] = a.xyz_le[k] + t * (b.xyz_le[k] - a.xyz_le[k]);
        }
        tip.chord = a.chord + t * (b.chord - a.chord);
        wing.xsecs[n - 1] = tip;
        wing.reference = None;
        wing.reference_xsecs = None;
        owned.s_ref = 511.0;
        owned.b_ref = 59.64;
    }
    let plane = &owned;
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let (x0, c) = (frame0.x_lemac_m, frame0.chord_m);
    let t = np_terms::decompose(plane, &fine, 0.2, x0, c);
    let input = NpConditionsInput {
        low_speed_mach: 0.2,
        low_speed_altitude_m: 0.0,
        cruise_mach: 0.2,
        cruise_altitude_m: 0.0,
        high_lift: None,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    let np = neutral_point_conditions(plane, &fine, &input).map_err(|e| format!("{e:?}"))?;
    let d = &np.diagnostics;
    let f = &d.fuselage;
    let share = d.wing_lift_fraction;
    let k_i = d.wing_body_lift_factor; //BASE:let k_i = 1.0;
    let h_x = if k_i != 1.0 {
        (d.x_wing_body_lift_m - x0) / c
    } else {
        0.0
    }; //BASE:let h_x = 0.0;
    let fus = (f.torenbeek_d1_shift_m + share * f.torenbeek_d2_shift_m) / c;
    let nac = d.nacelle_shift_m / c;
    let a_wb = k_i * t.a_w;
    let h_wb =
        (t.a_w * t.h_wing_isolated + (k_i - 1.0) * t.a_w * h_x + d.cl_alpha_total * (fus + nac))
            / a_wb;
    Ok(format!(
        "{label},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
        t.a_w,
        k_i,
        a_wb * plane.s_ref / 511.0,
        t.h_wing_isolated,
        h_wb,
        d.cl_alpha_total,
        share,
        t.h_np,
        f.torenbeek_d1_shift_m / c,
        f.torenbeek_d2_shift_m / c,
        f.multhopp_shift_m / c,
        nac
    ))
}
