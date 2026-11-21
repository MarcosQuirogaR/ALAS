// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe: the load factor to buffet onset and its floor for every registered
//! preset, in declared-MTOW and mission-sized modes.
//! `cargo test -p alas-opt --release --test buffet_probe -- --ignored --nocapture`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]
use alas_config::AlasConfig;
mod support;
use support::assess_view;

#[test]
#[ignore = "prints a table; run with --ignored --nocapture"]
fn buffet_margin_table() {
    for mode in [
        alas_config::MtowSizing::FixedRequirement,
        alas_config::MtowSizing::SizedByMission,
    ] {
        for preset in alas_config::presets::available() {
            let mut config =
                AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
            config.optimizer.objective.mtow_sizing = mode;
            let design = alas_config::presets::get(preset).unwrap().design_vector;
            let view = assess_view(&config, &design).unwrap();
            let row = view.residuals.iter().find(|r| r.id == "buffet_margin");
            eprintln!(
                "BUFFET {mode:?} {preset} n={:?} floor={:?}",
                row.map(|r| r.actual),
                row.map(|r| r.limit)
            );
        }
    }
}
