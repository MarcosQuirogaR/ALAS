// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset-report values that depend on the polar basis (A1 evidence).
//!
//! `cargo run --release -p alas-pipeline --example a1_effect_probe [preset ...]`
//! prints, per preset, the design point, the polar fit, the trimmed design
//! point, the ISA sea-level field reference and the physical-feasibility
//! findings with their numbers, one line per group, so two builds can be
//! diffed.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::feasibility::assess_physical_feasibility;
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::full_analysis::FullAnalysis;
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
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let d = &report.design_point;
    let f = &report.polar_fit;
    println!(
        "{name} design: alpha {:.3} CL {:.4} CD {:.5} L/D {:.2} | fit cd0 {:.5} c1 {:+.5} k {:.5} e {:.4} ({:?}) | SM {:.4}",
        d.alpha_deg, d.cl, d.cd, d.l_over_d, f.cd0, f.c1, f.k, f.oswald_e, f.status, report.static_margin
    );
    // The nearest sweep sample to the design lift (the pre-A1F design point)
    // beside the interpolated one the report now carries.
    if let Some(k) = (0..report.polar.cl.len()).min_by(|&i, &j| {
        (report.polar.cl[i] - d.cl)
            .abs()
            .total_cmp(&(report.polar.cl[j] - d.cl).abs())
    }) {
        let p = &report.polar;
        println!(
            "{name} nearest: alpha {:.3} CL {:.4} CD {:.5} L/D {:.2} | interpolated minus nearest: dalpha {:+.3} dCL {:+.4} dCD {:+.5} dL/D {:+.2}",
            p.alpha_deg[k], p.cl[k], p.cd[k], p.l_over_d[k],
            d.alpha_deg - p.alpha_deg[k], d.cl - p.cl[k], d.cd - p.cd[k], d.l_over_d - p.l_over_d[k]
        );
    }
    // The polar at fixed lift coefficients, so the basis change is not mixed
    // with which sweep sample happens to lie nearest the design lift.
    let at = |column: &[f64], cl: f64| alas_math::interp(cl, &report.polar.cl, column);
    for cl in [0.4, 0.5, 0.6] {
        let cd = at(&report.polar.cd, cl);
        println!(
            "{name} fixed: CL {cl:.2} CD {cd:.5} CDi {:.5} L/D {:.2} alpha {:.3}",
            at(&report.polar.cd_induced, cl),
            cl / cd,
            at(&report.polar.alpha_deg, cl)
        );
    }
    if let Some(t) = report.trimmed_design_point {
        println!(
            "{name} trim: alpha {:.3} CL {:.4} CD {:.5} L/D {:.2}",
            t.alpha_deg, t.cl, t.cd, t.l_over_d
        );
    }
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|e| e.to_string())?;
    let mtow = config.requirements.mtow_kg;
    let mlw = config.landing_mass_limit_kg(mtow);
    match report_isa_sea_level_field_reference(&config, &report, airplane.s_ref, mtow, mlw) {
        Ok(r) => println!(
            "{name} field: TOFL {:.0} BFL {:.0} LDR {:.0} LFL {:.0} Vref {:.1}",
            r.takeoff_field_length_m,
            r.model_balanced_field_length_m,
            r.landing_distance_m,
            r.landing_field_length_m,
            r.vref_m_s
        ),
        Err(error) => println!("{name} field: ERROR {error}"),
    }
    let feasibility = assess_physical_feasibility(&config, &preset.design_vector, &report, None);
    for finding in &feasibility.findings {
        println!(
            "{name} finding: {:?} {:?} actual {:?} limit {:?} {}",
            finding.code, finding.severity, finding.actual, finding.limit, finding.unit
        );
    }
    Ok(())
}
