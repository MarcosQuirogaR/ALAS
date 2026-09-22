// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Print the Results Summary propulsion card for the ATR 72-600.
//!
//! This calls the same producer the desktop Results Summary calls
//! (`propulsion_cycle_summary`, reached from
//! `alas-gui/src/views/results_view/summary_parts/part_01.rs`), so what it
//! prints is what a user reads after selecting the ATR preset and running an
//! analysis. It exists to make the turboprop's availability in the user flow
//! checkable without a screenshot: shaft power, propeller power, propeller
//! efficiency and fuel flow have to appear as themselves, and no jet thrust
//! may appear anywhere.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-report --example atr_summary_probe
//! ```

// A probe whose entire purpose is its console output.
#![allow(clippy::print_stdout)]

// A diagnostic probe run by hand: if the registered preset does not load,
// stopping with that message is the useful outcome.
#[allow(clippy::expect_used)]
fn main() {
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
        .expect("the registered ATR 72-600 loads");
    for line in alas_report::families::propulsion::propulsion_cycle_summary(&config) {
        println!("{line}");
    }
}
