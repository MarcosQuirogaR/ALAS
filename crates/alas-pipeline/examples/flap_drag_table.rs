// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Take-off flap profile drag increment of every registered preset (diagnostic).
//!
//! `cargo run --release -p alas-pipeline --example flap_drag_table`
#![allow(clippy::print_stdout, missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::presets;

fn main() {
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let perf = preset.performance.clone().unwrap_or_default();
        println!(
            "{name}: dCD0_flap {:.4} | {}",
            perf.oei_climb_delta_cd,
            perf.cl_max_to_source
                .split("; take-off flap profile drag")
                .nth(1)
                .unwrap_or("(class default 0.015)")
        );
    }
}
