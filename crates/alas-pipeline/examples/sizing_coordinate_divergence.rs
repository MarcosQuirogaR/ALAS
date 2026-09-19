// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Does the search and the finalist report agree about where the aeroplane
//! balances, and if not, is it the loading or a coordinate?
//!
//! `alas_opt::assess_product_candidate` closes the mission and reports the
//! mass/CG state its own feasibility gate was evaluated on.
//! `FullAnalysis::run_at_sized_takeoff_mass` then rebuilds the finalist report
//! bound to the takeoff mass that gate produced. Both are handed the *same*
//! preset and the *same* design vector, so any disagreement is internal.
//!
//! The mass lane measured a lumped-payload-against-resolved-layout spread of
//! `+14.91` points of %MAC on the A320-200 and `+1.15` on the A380-800
//! (`opus-payload-cg-consistency-handoff.md`). That measurement is about two
//! *payload* models. This probe asks a narrower question at the
//! mission/optimizer boundary: with the loading held identical, do the two
//! sides still place the aeroplane differently?
//!
//! It decides between exactly two explanations and refuses to guess:
//!
//! * **loading** — the two sides carry different payload or fuel, so the CG
//!   difference follows from a mass difference that is visible here;
//! * **coordinate** — the two sides carry the *same* masses and still place
//!   the centre of gravity differently, which means a station did not survive
//!   the convergence.
//!
//! If the masses differ *and* the residual CG gap is larger than the loading
//! difference can explain, it says so and stays undecided rather than
//! attributing the shift.
//!
//! SI: kg, m. `%MAC = (x_cg - mac_le_x) / mac * 100`, both columns in the same
//! frame, so the frame cancels from the difference.
//!
//! Run:
//! `cargo run --release -p alas-pipeline --example sizing_coordinate_divergence`

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_mass::breakdown::OEW_KEYS;
use alas_pipeline::FullAnalysis;

/// The two the mass lane measured, plus AVE.
///
/// AVE is the control: a bare preset document resolves to reference
/// adaptation, where the fuselage is not re-derived, while `AlasConfig::default()`
/// is a clean sheet that sizes the body from the cabin. If the divergence
/// `finalist_selection_matches_report` measures is that seam, it should appear
/// on the clean sheet and not on the adapted presets.
const PROBE: &[&str] = &["A320-200", "A380-800", "AVE"];

fn oew(masses: &std::collections::HashMap<String, f64>) -> f64 {
    OEW_KEYS.iter().filter_map(|key| masses.get(*key)).sum()
}

fn main() {
    println!(
        "Sized search state against the finalist report at identical geometry.\n\
         SI: kg, m. Both sides use the same preset and the same design vector.\n"
    );
    for name in PROBE {
        println!("================ {name} ================");
        let Ok(preset) = presets::get(name) else {
            println!("preset lookup failed");
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            println!("configuration did not load");
            continue;
        };
        let design: DesignVector = preset.design_vector;

        let assessment = match alas_opt::assess_product_candidate(&config, &design) {
            Ok(assessment) => assessment,
            Err(reason) => {
                println!("search side not sizeable: {reason}");
                continue;
            }
        };
        let sized = &assessment.sized;
        let resolved = &assessment.resolved;

        let report = match FullAnalysis::new(config.clone()).run_at_sized_takeoff_mass(
            &design,
            true,
            sized.takeoff_mass_kg,
        ) {
            Ok(report) => report,
            Err(reason) => {
                println!("report side refused: {reason}");
                continue;
            }
        };

        let search_oew = sized.operating_empty_mass_kg;
        let report_oew = oew(&report.component_masses);
        let search_payload = sized.payload_kg;
        let search_fuel = sized.takeoff_fuel_kg;
        let report_payload = report
            .component_masses
            .get("Payload")
            .copied()
            .unwrap_or(f64::NAN);
        let report_fuel = report
            .component_masses
            .get("Fuel")
            .copied()
            .unwrap_or(f64::NAN);

        let mac = resolved.mac_m;
        let search_cg = resolved.cg_x_m;
        let report_cg = report.physical_cg[0];
        let cg_gap = report_cg - search_cg;
        let pct_mac_gap = if mac > 0.0 {
            cg_gap / mac * 100.0
        } else {
            f64::NAN
        };

        println!(
            "{:<22} {:>16} {:>16} {:>14}",
            "quantity", "search (sized)", "report (finalist)", "difference"
        );
        let row = |label: &str, a: f64, b: f64| {
            println!("{label:<22} {a:>16.3} {b:>16.3} {:>14.3}", b - a);
        };
        row(
            "takeoff mass kg",
            sized.takeoff_mass_kg,
            report.component_masses.values().sum::<f64>(),
        );
        row("operating empty kg", search_oew, report_oew);
        row("payload kg", search_payload, report_payload);
        row("fuel kg", search_fuel, report_fuel);
        row("mac m", mac, report.airplane.c_ref);
        row("cg x m", search_cg, report_cg);
        println!(
            "{:<22} {:>16} {:>16} {:>14.3}",
            "cg gap % MAC", "", "", pct_mac_gap
        );
        println!(
            "worst loading state (search): zfw {:.1} kg | cg {:.3} m | neutral point {:.3} m",
            sized.zero_fuel_mass_kg, search_cg, resolved.x_neutral_point_m
        );

        // The decision. A mass difference moves the centre of gravity; the
        // question is whether it moves it by this much.
        // Every comparison is relative to the quantity being compared. A
        // dispatch closure converges to its own tolerance, so a fuel figure
        // that agrees to about 1e-5 of itself is the same figure; comparing
        // that absolute gap against a payload-scaled tolerance, as the first
        // version of this probe did, reports a disagreement that is not one.
        let relative = |a: f64, b: f64| (b - a).abs() / a.abs().max(1.0);
        let payload_gap = relative(search_payload, report_payload);
        let fuel_gap = relative(search_fuel, report_fuel);
        let cg_relative_gap = cg_gap.abs() / mac.max(1.0);
        let identical_loading = payload_gap <= 1.0e-6 && fuel_gap <= 1.0e-4;
        let cg_moved = cg_relative_gap > 1.0e-6;

        print!("VERDICT: ");
        if !cg_moved {
            println!(
                "AGREE. The centre of gravity is the same on both sides to {cg_relative_gap:.1e} \
                 of MAC ({pct_mac_gap:.3} points), so there is no shift to attribute. \
                 Payload agrees to {payload_gap:.1e} and fuel to {fuel_gap:.1e} of itself."
            );
        } else if identical_loading {
            println!(
                "COORDINATE. The two sides carry the same loading and still place the \
                 centre of gravity {cg_gap:.3} m apart ({pct_mac_gap:.2} points of MAC), \
                 so a station did not survive convergence."
            );
        } else {
            println!(
                "UNDECIDED. The centre of gravity differs by {pct_mac_gap:.2} points of MAC \
                 while the loading also differs (payload {payload_gap:.1e}, fuel \
                 {fuel_gap:.1e} of itself), so this comparison cannot separate a loading \
                 difference from a coordinate one. The missing comparison is the same two \
                 evaluations with the loading pinned identical on both sides."
            );
        }
        println!();
    }
}
