// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Do the search-time gate and the finalist report build the *same body*?
//!
//! `finalist_selection_matches_report` measures four disagreements between
//! `alas_opt::assess_product_candidate` and
//! `FullAnalysis::run_at_sized_takeoff_mass` on one identical design vector:
//! the H-stab station, the fuselage mass, the payload station and the neutral
//! point. The mass lane attributed the payload station to a lumped-against-
//! resolved payload model. This probe asks the prior question the attribution
//! assumed away: **is the caller's `fuselage_length_m` the length the search
//! actually evaluates?**
//!
//! `alas_opt::mdo::build::size_fuselage_from_cabin` re-derives that coordinate
//! from the cabin load case whenever the design space says so, and it searches
//! its own specification interval, so the caller's literal is discarded. The
//! report is handed the caller's literal unchanged. If the two lengths differ
//! then the fuselage mass, every body-referenced station and the cabin the
//! payload is laid out in all differ *before* any payload model is consulted.
//!
//! Two measurements, neither of which restates the rule:
//!
//! 1. **Invariance** -- assess the same design at several `fuselage_length_m`
//!    values. If every resolved ledger is bit-identical, the search does not
//!    read the caller's coordinate at all.
//! 2. **Recovery** -- bisect the length the *report* must be given for its
//!    H-stab station to equal the search's. That is the body the search
//!    evaluated, recovered through a public entry point rather than asserted.
//!
//! SI: kg, m; geometry frame (x aft, z up).
//!
//! Run:
//! `cargo run --release -p alas-pipeline --example finalist_geometry_divergence`

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::FullAnalysis;

/// Exact `design_vector` block from the r5 nominal run's own
/// `design_database.json`, the same fixture
/// `crates/alas-pipeline/tests/finalist_selection_matches_report.rs` replays.
fn r5_finalist_design() -> DesignVector {
    DesignVector {
        span_m: 64.25,
        root_chord_m: 12.125,
        break_chord_m: 7.3,
        tip_chord_m: 1.0,
        sweep_deg: 34.0,
        tip_twist_deg: 1.0,
        wing_x_shift_m: -1.6250000000000004,
        tail_scale: 1.0,
        fuselage_length_m: 76.25000000029104,
        tail_x_shift_m: -2.0,
        airfoil_thickness_scale: 0.8,
        airfoil_camber_scale: 1.2625,
        bump_upper_front: -0.002375,
        bump_upper_rear: -0.0006249999999999997,
        bump_lower_mid: 0.002,
        bump_lower_rear: -0.004,
    }
}

fn report_at(config: &AlasConfig, design: &DesignVector, takeoff_mass_kg: f64) -> Option<Row> {
    let report = FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(design, true, takeoff_mass_kg)
        .ok()?;
    Some(Row {
        hstab_x_m: *report.mass_coordinates.get("H-Stab")?.first()?,
        fuselage_x_m: *report.mass_coordinates.get("Fuselage")?.first()?,
        payload_x_m: *report.mass_coordinates.get("Payload")?.first()?,
        fuselage_kg: *report.component_masses.get("Fuselage")?,
        payload_kg: *report.component_masses.get("Payload")?,
    })
}

struct Row {
    hstab_x_m: f64,
    fuselage_x_m: f64,
    payload_x_m: f64,
    fuselage_kg: f64,
    payload_kg: f64,
}

fn main() {
    let config = AlasConfig::default();
    let base = r5_finalist_design();

    println!(
        "Search-time gate against the finalist report, one design vector.\n\
         SI: kg, m. Configuration: AlasConfig::default().\n"
    );

    // ---- 1. Invariance of the search to the caller's fuselage coordinate.
    println!("== 1. Does the search read the caller's fuselage_length_m?");
    let mut reference: Option<(f64, f64, f64, f64)> = None;
    for length_m in [70.0_f64, 76.25000000029104, 82.0] {
        let mut design = base;
        design.fuselage_length_m = length_m;
        match alas_opt::assess_product_candidate(&config, &design) {
            Ok(assessment) => {
                let resolved = &assessment.resolved;
                let probe = (
                    resolved.takeoff_mass_kg,
                    resolved.masses.fuselage,
                    resolved.coords.h_stab[0],
                    resolved.coords.payload[0],
                );
                println!(
                    "  input length {length_m:>10.4} m -> tow {:>12.4} kg | fuselage {:>10.3} kg \
                     | H-Stab x {:>9.4} m | payload x {:>9.4} m",
                    probe.0, probe.1, probe.2, probe.3
                );
                match reference {
                    None => reference = Some(probe),
                    Some(first) => {
                        let identical = (first.0 - probe.0).abs() < 1.0e-9
                            && (first.1 - probe.1).abs() < 1.0e-9
                            && (first.2 - probe.2).abs() < 1.0e-9
                            && (first.3 - probe.3).abs() < 1.0e-9;
                        println!(
                            "    identical to the first row: {}",
                            if identical { "YES" } else { "NO" }
                        );
                    }
                }
            }
            Err(reason) => println!("  input length {length_m:>10.4} m -> refused: {reason}"),
        }
    }

    // ---- 2. Recover the body the search evaluated.
    let assessment = match alas_opt::assess_product_candidate(&config, &base) {
        Ok(assessment) => assessment,
        Err(reason) => {
            println!("\nsearch refused the fixture: {reason}");
            return;
        }
    };
    let tow = assessment.sized.takeoff_mass_kg;
    let search_hstab = assessment.resolved.coords.h_stab[0];
    let search_fuselage_x = assessment.resolved.coords.fuselage[0];
    let search_payload_x = assessment.resolved.coords.payload[0];
    let search_fuselage_kg = assessment.resolved.masses.fuselage;
    let search_payload_kg = assessment.resolved.masses.payload;

    println!("\n== 2. The report at the caller's literal length");
    let literal = report_at(&config, &base, tow).expect("the report builds at the fixture length");
    println!(
        "  {:<16} {:>14} {:>14} {:>12}",
        "quantity", "search", "report", "difference"
    );
    let row = |label: &str, a: f64, b: f64| {
        println!("  {label:<16} {a:>14.4} {b:>14.4} {:>12.4}", b - a);
    };
    row("H-Stab x m", search_hstab, literal.hstab_x_m);
    row("Fuselage x m", search_fuselage_x, literal.fuselage_x_m);
    row("Payload x m", search_payload_x, literal.payload_x_m);
    row("Fuselage kg", search_fuselage_kg, literal.fuselage_kg);
    row("Payload kg", search_payload_kg, literal.payload_kg);

    println!("\n== 3. The report length whose H-stab station equals the search's");
    // The H-stab station is monotone in body length on this geometry, so a
    // plain bisection recovers it. Bounds are the design-space specification
    // interval this probe is allowed to see through the public bound table.
    let (mut lower, mut upper) = (30.0_f64, 90.0_f64);
    let hstab_at = |length_m: f64| -> Option<f64> {
        let mut design = base;
        design.fuselage_length_m = length_m;
        report_at(&config, &design, tow).map(|row| row.hstab_x_m)
    };
    let Some(low_value) = hstab_at(lower) else {
        println!("  the report refused the lower bound; no recovery attempted");
        return;
    };
    let Some(high_value) = hstab_at(upper) else {
        println!("  the report refused the upper bound; no recovery attempted");
        return;
    };
    println!("  H-Stab x at {lower:.2} m body = {low_value:.4} m, at {upper:.2} m body = {high_value:.4} m");
    if (low_value - search_hstab).signum() == (high_value - search_hstab).signum() {
        println!("  the search's H-stab station is outside that bracket; not recovered");
        return;
    }
    for _ in 0..60 {
        let middle = 0.5 * (lower + upper);
        let Some(value) = hstab_at(middle) else { break };
        if (value - search_hstab).signum() == (low_value - search_hstab).signum() {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    let recovered = 0.5 * (lower + upper);
    println!(
        "  recovered body length {recovered:.6} m against the caller's {:.6} m \
         (difference {:.6} m)",
        base.fuselage_length_m,
        base.fuselage_length_m - recovered
    );
    if let Some(row) = report_at(
        &config,
        &{
            let mut design = base;
            design.fuselage_length_m = recovered;
            design
        },
        tow,
    ) {
        println!("\n== 4. The report rebuilt on the recovered body");
        println!(
            "  {:<16} {:>14} {:>14} {:>12}",
            "quantity", "search", "report", "difference"
        );
        let row2 = |label: &str, a: f64, b: f64| {
            println!("  {label:<16} {a:>14.4} {b:>14.4} {:>12.4}", b - a);
        };
        row2("H-Stab x m", search_hstab, row.hstab_x_m);
        row2("Fuselage x m", search_fuselage_x, row.fuselage_x_m);
        row2("Payload x m", search_payload_x, row.payload_x_m);
        row2("Fuselage kg", search_fuselage_kg, row.fuselage_kg);
        row2("Payload kg", search_payload_kg, row.payload_kg);
    }

    println!("\n== 5. Is the derivation a closed fixed point?");
    for start in [70.0_f64, 72.0, 76.25000000029104, 82.0] {
        let mut value = start;
        let mut trail = vec![format!("{value:.4}")];
        for _ in 0..4 {
            let Some(next) = evaluated_body_m(&config, &base, value) else {
                trail.push("refused".to_owned());
                break;
            };
            trail.push(format!("{next:.4}"));
            let converged = (next - value).abs() < 1.0e-6;
            value = next;
            if converged {
                break;
            }
        }
        println!("  start {start:>9.4} m -> {}", trail.join(" -> "));
    }
}

/// Iterate `L -> the body the search evaluates when handed L`.
///
/// `build_geometry_with_fuselage_policy` resolves the cabin load case on the
/// aircraft built at the *caller's* length and only then re-derives the length
/// from that load case, so the evaluated body is a function of the caller's
/// coordinate. A returned design vector rebuilds the same geometry only if
/// that map has the returned value as a fixed point.
fn evaluated_body_m(config: &AlasConfig, base: &DesignVector, input_m: f64) -> Option<f64> {
    let mut design = *base;
    design.fuselage_length_m = input_m;
    let assessment = alas_opt::assess_product_candidate(config, &design).ok()?;
    let target = assessment.resolved.coords.h_stab[0];
    let tow = assessment.sized.takeoff_mass_kg;
    let hstab_at = |length_m: f64| -> Option<f64> {
        let mut probe = *base;
        probe.fuselage_length_m = length_m;
        report_at(config, &probe, tow).map(|row| row.hstab_x_m)
    };
    let (mut lower, mut upper) = (30.0_f64, 90.0_f64);
    let low_value = hstab_at(lower)?;
    let high_value = hstab_at(upper)?;
    if (low_value - target).signum() == (high_value - target).signum() {
        return None;
    }
    for _ in 0..60 {
        let middle = 0.5 * (lower + upper);
        let Some(value) = hstab_at(middle) else { break };
        if (value - target).signum() == (low_value - target).signum() {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    Some(0.5 * (lower + upper))
}
