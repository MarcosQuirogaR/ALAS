// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bit-identity harness for registered presets (kept re-runnable).
//!
//! Prints every output of `assess_product_candidate` of every registered
//! preset in the two reference modes with `{:?}` (shortest round-trip
//! decimals, so equal text is equal bits): the sized candidate, masses,
//! coordinates, the resolved design vector, tail sizing, centre of gravity,
//! neutral point and takeoff mass (the "state" line is these fields, not the
//! whole `ResolvedProductState`), the cost scalars and every residual row.
//!
//! Run it in two trees and `cmp` the `DUMP` lines:
//! `cargo test -p alas-opt --test preset_bitdump -- --ignored --nocapture`.
//! `E1_DUMP_ONLY=A320-200,ATR72-600` restricts it to named presets. The
//! clean-sheet `^DUMPCS ` lines of `tests/clean_sheet_profile.rs` are
//! compared the same way. The dumps must be identical with and without the
//! cabin-length memo (`ALAS_SPEEDUPS_OFF=cabin_memo`); they move only with
//! model changes. Re-pin only from a probe run with the cause.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, AlasConfig};
use alas_opt::{assess_product_candidate, canonicalize_design};
use serde_json::json;

#[test]
#[ignore = "bit-identity harness: prints, asserts nothing"]
fn dump_registered_presets_in_the_reference_modes() {
    let only = std::env::var("E1_DUMP_ONLY").unwrap_or_default();
    for preset in presets::registry() {
        if preset.name == "test" || (!only.is_empty() && !only.contains(preset.name)) {
            continue;
        }
        for mode in [DesignMode::ReferenceAdaptation, DesignMode::BaselineSandbox] {
            let mut config = AlasConfig::from_value(&json!({ "preset": preset.name })).unwrap();
            config.optimizer.design_space.mode = mode;
            let design = canonicalize_design(&config, preset.design_vector).unwrap();
            let label = format!("{}/{}", preset.name, mode.as_str());
            match assess_product_candidate(&config, &design) {
                Ok(a) => {
                    println!("DUMP {label} sized {:?}", a.sized);
                    println!("DUMP {label} masses {:?}", a.resolved.masses);
                    println!("DUMP {label} coords {:?}", a.resolved.coords);
                    println!(
                        "DUMP {label} state {:?} {:?} {:?} {:?} {:?}",
                        a.resolved.design,
                        a.resolved.tail_sizing,
                        a.resolved.cg_x_m,
                        a.resolved.x_neutral_point_m,
                        a.resolved.takeoff_mass_kg
                    );
                    println!(
                        "DUMP {label} scalars {:?} {:?} {:?} {:?} {:?}",
                        a.cost,
                        a.objective_value,
                        a.hard_violation_sum,
                        a.soft_violation_sum,
                        a.hard_feasible
                    );
                    for row in &a.residuals {
                        println!("DUMP {label} row {row:?}");
                    }
                }
                Err(reason) => println!("DUMP {label} failed {reason}"),
            }
        }
    }
}
