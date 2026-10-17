// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe of the uncertainty-aware cruise-thrust and top-of-climb checks over
//! the registered presets (owner decision 2026-10-13): for each jet, required
//! and available thrust-to-weight of level cruise at the mid-cruise weight,
//! the scatter band and the verdict. Run with
//! `cargo test -p alas-opt --release --test thrust_uncertainty_probe -- --ignored --nocapture`.
//! The ordinary test pins the scatter constant and the verdict arithmetic on
//! a real preset.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use alas_opt::mdo::{thrust_scatter, ThrustAssessment, ThrustVerdict};

fn verdict_name(a: &ThrustAssessment) -> &'static str {
    match a.verdict() {
        ThrustVerdict::Pass => "pass",
        ThrustVerdict::Warning => "warning",
        ThrustVerdict::Hard => "hard",
    }
}

#[test]
#[ignore = "probe: prints the per-jet verdicts of all registered presets"]
fn thrust_uncertainty_verdicts_of_every_registered_preset() {
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = assess_product_candidate(&config, &design).unwrap();
        let scatter = thrust_scatter(&config);
        let find = |id: &str| assessment.residuals.iter().find(|row| row.id == id);
        let level = find("cruise_thrust").map(|row| {
            let a = ThrustAssessment::new(row.limit, row.actual, scatter);
            format!(
                "level req {:.4} avail {:.4} short {:+.1} % band {:.1} % -> {}",
                row.limit,
                row.actual,
                100.0 * a.shortfall(),
                100.0 * scatter,
                verdict_name(&a)
            )
        });
        let toc = find("toc_rate_of_climb").map(|row| {
            format!(
                "toc rate {:.3} m/s (criterion {:.3}) violation {:.4}",
                row.actual, row.limit, row.normalized_violation
            )
        });
        let toc_warning = find("toc_thrust_within_scatter").and_then(|row| row.detail.clone());
        let level_warning = find("cruise_thrust_within_scatter").and_then(|row| row.detail.clone());
        println!(
            "PROBE {name}: {} | {} | hard {:?} | level warning {:?} | toc warning {:?}",
            level.unwrap_or_else(|| "no level row".into()),
            toc.unwrap_or_else(|| "no toc row".into()),
            assessment.violated_hard_ids(),
            level_warning,
            toc_warning
        );
    }
}
