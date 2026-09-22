// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why a preset's candidates are rejected by the dispatch closure, in the
//! closure's own words.
//!
//! The search reports a rejected candidate by residual identifier, and
//! `dispatch_model_failed` is a boolean: `mdo::residuals` matches
//! `DispatchStatus::ModelFailed(_)` and drops the string. That is fine for
//! ranking and useless for diagnosis: the all-preset matrix recorded the
//! A320-200 as `dispatch_model_failed` on 253 of 253 candidates without ever
//! saying what failed. The reason is not lost, only unreported:
//! `CandidateAssessment::sized.dispatch.status` still carries it. This prints
//! it, for each named preset at its own registered design vector.
//!
//! SI throughout: kg, m. Run:
//! `cargo run --release -p alas-opt --example dispatch_rejection_reason`

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_mass::dispatch::DispatchStatus;
use alas_opt::assess_product_candidate;

const PROBE: &[&str] = &[
    "AVE",
    "A320-200",
    "A220-300",
    "ATR72-600",
    "A380-800",
    "B787-9",
    "A340-300",
    "DC-10",
];

fn describe(status: &DispatchStatus) -> String {
    match status {
        DispatchStatus::Converged => "converged".to_owned(),
        DispatchStatus::MtowLimited { shortfall_kg } => {
            format!("mtow limited, {shortfall_kg:.1} kg short")
        }
        DispatchStatus::TankLimited { shortfall_kg } => {
            format!("tank limited by {shortfall_kg:.1} kg")
        }
        DispatchStatus::NotConverged { last_change_kg } => {
            format!("not converged, last change {last_change_kg:.1} kg")
        }
        DispatchStatus::ModelFailed(reason) => format!("model failed: {reason}"),
    }
}

fn main() {
    for name in PROBE {
        let Ok(preset) = presets::get(name) else {
            println!("{name}: not registered");
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            println!("{name}: configuration did not load");
            continue;
        };
        let design: DesignVector = preset.design_vector;
        match assess_product_candidate(&config, &design) {
            Ok(assessment) => {
                let sized = &assessment.sized;
                println!(
                    "{name}: dispatch {} | takeoff {:.1} kg | ramp fuel {:.1} kg | capacity {:.1} kg | range {:.0} m | violated {:?}",
                    describe(&sized.dispatch.status),
                    sized.takeoff_mass_kg,
                    sized.ramp_fuel_kg,
                    sized.usable_capacity_kg,
                    sized.design_range_m,
                    assessment.violated_hard_ids()
                );
            }
            Err(reason) => println!("{name}: assessment refused: {reason}"),
        }
    }
}
