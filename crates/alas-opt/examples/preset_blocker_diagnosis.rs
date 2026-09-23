// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why each registered aircraft's design space is or is not feasible, named
//! quantity by named quantity.
//!
//! Dispatch-local probe for the independent optimizer verification. The
//! all-preset benchmark reports *which* residual rejects every candidate; this
//! probe reports the numbers behind that residual on the preset's own nominal
//! design, so a blocker can be attributed to its owning discipline instead of
//! being restated as a search failure.
//!
//! Three questions it answers directly:
//!
//! 1. `structural_inventory_unverified`: is the wing inventory incomplete
//!    because the strength-sized box outweighs the frozen empirical wing, and
//!    by how many kg? That variant is the only way a reference-adaptation
//!    preset can report an incomplete inventory.
//! 2. `mission_profile_range`: what still-air distance does the configured
//!    climb/descent ladder occupy at the configured cruise altitude, against
//!    the range the configured route actually supplies?
//! 3. everything else: the full typed residual table, in evaluation order,
//!    with `actual`, `limit`, unit, signed raw residual and policy.
//!
//! Masses kg, lengths m, distances m unless the printed unit says otherwise.
//!
//! Usage:
//!   `cargo run --release -p alas-opt --example preset_blocker_diagnosis -- \
//!      [preset|all] [clean_sheet|reference_adaptation|baseline_sandbox]`

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::wing_reconciliation::{reconcile, StructuralInventory};

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());
    // A bare preset document loads `CleanSheet`, which re-derives the
    // fuselage from the cabin. `reference_adaptation` is the mode a
    // registered aircraft is actually optimized in and the mode the
    // all-preset benchmark runs, so a blocker diagnosis meant to explain that
    // benchmark has to be taken there.
    let mode = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "clean_sheet".to_owned());
    let presets: Vec<String> = if target.eq_ignore_ascii_case("all") {
        alas_config::presets::available()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    } else {
        vec![target]
    };
    for preset in presets {
        diagnose(&preset, &mode);
    }
}

fn diagnose(preset: &str, mode: &str) {
    println!("================ {preset} ({mode}) ================");
    let config = match AlasConfig::from_value(&serde_json::json!({ "preset": preset })) {
        Ok(mut config) => {
            config.optimizer.design_space.mode = match mode {
                "reference_adaptation" => alas_config::DesignMode::ReferenceAdaptation,
                "baseline_sandbox" => alas_config::DesignMode::BaselineSandbox,
                _ => alas_config::DesignMode::CleanSheet,
            };
            config
        }
        Err(error) => {
            println!("config load failed: {error}");
            return;
        }
    };
    let design = match alas_config::presets::get(preset) {
        Ok(entry) => entry.design_vector,
        Err(error) => {
            println!("preset lookup failed: {error}");
            return;
        }
    };

    // The mission inputs the `mission_profile_range` residual is built from.
    println!(
        "route: {} -> {} | design_range_nmi (explicit override) {:.1} | cruise_mach {:.3} | cruise_altitude_m {:.1}",
        config.departure_airport,
        config.arrival_airport,
        config.optimizer.objective.design_range_nmi,
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
    );
    println!(
        "mission profile: initial_climb_rate {:.2} m/s | descent_1_rate {:.2} m/s | cruise_1_tas {:.1} m/s",
        config.mission.profile.initial_climb_rate_m_s,
        config.mission.profile.descent_1_rate_m_s,
        config.mission.profile.cruise_1_air_speed_m_s,
    );

    // The wing inventory, which is what `structural_inventory_unverified`
    // reports on. Built from the same geometry the sizing path builds.
    match AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), false) {
        Ok(plane) => match reconcile(&config, &design, &plane, None) {
            Ok(reconciliation) => {
                let label = match &reconciliation.inventory {
                    StructuralInventory::FrozenReference => "FrozenReference (complete)".to_owned(),
                    StructuralInventory::ReferenceExceededBySizedBox {
                        reference_total_kg,
                        sized_box_kg,
                    } => format!(
                        "ReferenceExceededBySizedBox (INCOMPLETE): empirical wing {reference_total_kg:.1} kg, strength-sized complete box {sized_box_kg:.1} kg, excess {:.1} kg (+{:.1} %)",
                        sized_box_kg - reference_total_kg,
                        100.0 * (sized_box_kg / reference_total_kg.max(1e-9) - 1.0)
                    ),
                    StructuralInventory::CleanSheet(_) => format!(
                        "CleanSheet enumerated inventory, complete = {}",
                        reconciliation.inventory_complete()
                    ),
                };
                println!("wing inventory: {label}");
                println!(
                    "reconciled wing: total {:.1} kg (primary box {:.1} + secondary {:.1}, closure residual {:.1} kg)",
                    reconciliation.feedback.total_wing_mass_kg,
                    reconciliation.feedback.primary_mass_kg,
                    reconciliation.feedback.secondary_mass_kg,
                    reconciliation.feedback.closure_residual_kg,
                );
            }
            Err(error) => println!("wing reconciliation failed: {error}"),
        },
        Err(error) => println!("geometry build failed: {error}"),
    }

    match alas_opt::assess_product_candidate(&config, &design) {
        Ok(assessment) => {
            println!(
                "nominal: hard_feasible {} | objective {:.3} | tow_kg {:.1} | oew_kg {:.1} | design_range_m {:.1} | minimum_profile_range_m {:.1}",
                assessment.hard_feasible,
                assessment.objective_value,
                assessment.sized.takeoff_mass_kg,
                assessment.sized.operating_empty_mass_kg,
                assessment.sized.design_range_m,
                assessment
                    .residuals
                    .iter()
                    .find(|residual| residual.id == "mission_profile_range")
                    .map_or(f64::NAN, |residual| residual.limit),
            );
            println!(
                "{:<34} {:<12} {:>14} {:>14} {:>8} {:>14} {:>11} {:<10}",
                "residual",
                "family",
                "actual",
                "limit",
                "unit",
                "raw_residual",
                "normalized",
                "policy"
            );
            let mut rows: Vec<_> = assessment.residuals.iter().collect();
            // Violated first, then by normalized violation, so the blocker is
            // the first line and the near-misses follow it.
            rows.sort_by(|left, right| {
                right
                    .normalized_violation
                    .total_cmp(&left.normalized_violation)
            });
            for residual in rows {
                if residual.normalized_violation <= 0.0
                    && residual.policy != alas_config::ConstraintPolicy::Hard
                {
                    continue;
                }
                println!(
                    "{:<34} {:<12} {:>14.4} {:>14.4} {:>8} {:>14.4} {:>11.3e} {:<10}",
                    residual.id,
                    format!("{:?}", residual.family),
                    residual.actual,
                    residual.limit,
                    residual.unit,
                    residual.raw_residual,
                    residual.normalized_violation,
                    format!("{:?}", residual.policy),
                );
            }
        }
        Err(reason) => println!("nominal candidate not sizeable: {reason}"),
    }
    println!();
}
