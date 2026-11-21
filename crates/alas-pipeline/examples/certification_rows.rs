// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Second-segment, approach-climb and obstacle-clearance rows of registered presets at
//! their certified MTOW (diagnostic).
//!
//! `cargo run --release -p alas-pipeline --example certification_rows [preset ...]`;
//! `CERT_OLD_FLAP=1` restores the class-default flap increments (0.015 take-off) for a
//! before/after comparison.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use serde_json::json;

fn main() {
    let old = std::env::var("CERT_OLD_FLAP").is_ok();
    for name in std::env::args().skip(1) {
        let mut value = json!({"preset": name});
        if let Ok(mode) = std::env::var("CERT_MODE") {
            value["optimizer"] = json!({"design_space": {"mode": mode}});
        }
        if old {
            value["performance"] = json!({"oei_climb_delta_cd": 0.015});
        }
        let config = AlasConfig::from_value(&value).unwrap();
        let design = presets::get(&name).unwrap().design_vector;
        let assessment = alas_opt::mdo::assess_product_candidate(&config, &design).unwrap();
        for row in assessment.residuals.iter().filter(|row| {
            row.id.starts_with("oei_")
                || row.id.starts_with("takeoff_obstacle")
                || row.id.starts_with("takeoff_net")
                || row.id == "takeoff_weight_limit"
        }) {
            println!(
                "{name} {} actual {:.4} limit {:.4} {} violated {} {:?}",
                row.id,
                row.actual,
                row.limit,
                row.unit,
                row.violated(),
                row.detail.as_deref().map(|d| &d[..d.len().min(160)])
            );
        }
        println!(
            "{name} hard violations {:?}",
            assessment.violated_hard_ids()
        );
    }
}
