// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static aeroelastic neutral-point shift per preset.
//!
//! `cargo run --release -p alas-pipeline --example elastic_np_probe [preset ...]`
//! builds each preset, takes the stiffness of its own sized wing box, and
//! prints the flexible-minus-rigid clean neutral point at the preset's cruise
//! condition in percent MAC. For `B747-400` it also scans the dynamic
//! pressures of the NASA CR-2144 flexible data (6-30 kPa) and prints the
//! least-squares slope per kPa beside the CR-2144 value 0.396 %MAC/kPa.
#![allow(clippy::print_stdout, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_stab::neutral_point::{neutral_point_conditions, NpConditionsInput, WingStiffness};
use serde_json::json;

const DEFAULT: [&str; 5] = ["A320-200", "A350-900", "B787-9", "LJ60", "B747-400"];

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in DEFAULT {
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
    let table =
        alas_mass::wing_reconciliation::wing_box_stiffness(&config, &preset.design_vector, plane)
            .map_err(|e| format!("{e:?}"))?;
    let stiffness = WingStiffness {
        y_m: table.y_m,
        ei_nm2: table.ei_nm2,
        gj_nm2: table.gj_nm2,
        box_axis_fraction: table.box_axis_fraction,
        inertia_relief: Some(alas_stab::neutral_point::InertiaRelief {
            carried_kg_m: table.carried_kg_m,
            point_masses_kg: table.point_masses_kg,
            aircraft_mass_kg: table.aircraft_mass_kg,
        }),
    };
    let tail =
        alas_mass::wing_reconciliation::tail_flexibility(&config, &preset.design_vector, plane)
            .ok()
            .map(|t| alas_stab::neutral_point::TailFlexibility {
                fuselage_x_m: t.fuselage_x_m,
                fuselage_ei_nm2: t.fuselage_ei_nm2,
                stabilizer: WingStiffness {
                    y_m: t.stabilizer_y_m,
                    ei_nm2: t.stabilizer_ei_nm2,
                    gj_nm2: t.stabilizer_gj_nm2,
                    box_axis_fraction: t.stabilizer_axis_fraction,
                    inertia_relief: None,
                },
            });
    let mut tail = tail;
    // Diagnostic split of the tail terms: `ELASTIC_TAIL_TERM=fuselage` keeps only
    // the fuselage rotation, `stabilizer` only the tail box.
    if let (Some(t), Ok(term)) = (tail.as_mut(), std::env::var("ELASTIC_TAIL_TERM")) {
        let rigid = 1.0e9;
        if term == "fuselage" {
            t.stabilizer.ei_nm2.iter_mut().for_each(|v| *v *= rigid);
            t.stabilizer.gj_nm2.iter_mut().for_each(|v| *v *= rigid);
        } else if term == "stabilizer" {
            t.fuselage_ei_nm2.iter_mut().for_each(|v| *v *= rigid);
        }
    }
    if let Some(t) = &tail {
        println!(
            "{name}: fuselage EI_f at junction {:.3e} at tail {:.3e} N m2, tail box EI root {:.3e} GJ root {:.3e}",
            t.fuselage_ei_nm2[0],
            t.fuselage_ei_nm2[t.fuselage_ei_nm2.len() - 1],
            t.stabilizer.ei_nm2[0],
            t.stabilizer.gj_nm2[0]
        );
    }
    println!(
        "{name}: box EI root {:.3e} tip {:.3e} N m2, GJ root {:.3e} tip {:.3e} N m2, usable {}",
        stiffness.ei_nm2[0],
        stiffness.ei_nm2[stiffness.ei_nm2.len() - 1],
        stiffness.gj_nm2[0],
        stiffness.gj_nm2[stiffness.gj_nm2.len() - 1],
        stiffness.is_usable()
    );
    let mac = plane.c_ref;
    let shift_at = |mach: f64, altitude_m: f64, with_tail: bool| -> Result<_, String> {
        let input = NpConditionsInput {
            cruise_mach: mach,
            cruise_altitude_m: altitude_m,
            wing_stiffness: Some(stiffness.clone()),
            tail_flexibility: if with_tail { tail.clone() } else { None },
            ..NpConditionsInput::default()
        };
        let np = neutral_point_conditions(plane, &fine, &input).map_err(|e| format!("{e:?}"))?;
        np.elastic_cruise
            .ok_or_else(|| "no elastic report".to_owned())
    };
    let req = &config.requirements;
    let cruise = shift_at(req.cruise_mach, req.cruise_altitude_m, true)?;
    let wing_only = shift_at(req.cruise_mach, req.cruise_altitude_m, false)?;
    println!(
        "{name}: wing-only cruise shift {:+.2} %MAC",
        100.0 * wing_only.shift_m / mac
    );
    println!(
        "{name}: cruise M {:.3} at {:.0} m, q {:.2} kPa: shift {:+.2} %MAC (MAC {mac:.2} m), converged {}, {} it, tip incidence {:+.2} deg",
        req.cruise_mach,
        req.cruise_altitude_m,
        cruise.dynamic_pressure_pa / 1000.0,
        100.0 * cruise.shift_m / mac,
        cruise.converged,
        cruise.iterations,
        cruise.tip_incidence_deg,
    );
    if name == "B747-400" {
        // The CR-2144 flight conditions span Mach 0.45-0.8 and sea level to
        // 40,000 ft, q 6-30 kPa.
        let conditions = [
            (0.45, 6000.0),
            (0.60, 3000.0),
            (0.50, 3000.0),
            (0.80, 12_192.0),
            (0.70, 6000.0),
            (0.80, 6000.0),
            (0.65, 0.0),
        ];
        let mut points = Vec::new();
        for (mach, altitude) in conditions {
            let r = shift_at(mach, altitude, true)?;
            let q = r.dynamic_pressure_pa / 1000.0;
            println!(
                "  M {mach:.2} h {altitude:6.0} m q {q:5.1} kPa: shift {:+.2} %MAC converged {}",
                100.0 * r.shift_m / mac,
                r.converged
            );
            if r.converged {
                points.push((q, r.shift_m / mac));
            }
        }
        let n = points.len() as f64;
        let (sq, ss) = points
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mq, ms) = (sq / n, ss / n);
        let slope = points.iter().map(|p| (p.0 - mq) * (p.1 - ms)).sum::<f64>()
            / points.iter().map(|p| (p.0 - mq).powi(2)).sum::<f64>();
        println!(
            "B747-400 slope {:+.3} %MAC/kPa through q 6-30 kPa (CR-2144 measured: -0.396 %MAC/kPa), wing + fuselage + tail, intercept {:+.2} %MAC",
            100.0 * slope,
            100.0 * (ms - slope * mq)
        );
    }
    Ok(())
}
