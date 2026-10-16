// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Finding F44: a registered preset in CleanSheet mode sizes its fuselage by
//! the cabin of its brief. The global [65, 85] m box once pinned every
//! narrow-body fuselage to 65 m.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, AlasConfig};
use alas_opt::{canonicalize_design, DesignOptimizer};

#[test]
fn a_registered_narrowbody_clean_sheet_fuselage_is_sized_by_its_cabin() {
    // (preset, registered fuselage length m). The cabin-required length is
    // within 5 % of the registered body: the registered aircraft seats the
    // same brief, with its own door and galley allowances.
    for (name, registered_m) in [
        ("A320-200", 37.57),
        ("A220-300", 38.7),
        ("E195-E2", 41.6),
        ("ATR72-600", 27.166),
    ] {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        let (lower, _) = config.fuselage_sizing_interval_m();
        assert!(lower < registered_m, "{name}: interval floor {lower} m");
        let preset = presets::registry().iter().find(|p| p.name == name).unwrap();
        let design = canonicalize_design(&config, preset.design_vector).unwrap();
        let length = design.fuselage_length_m;
        assert!(
            (length - registered_m).abs() < 0.05 * registered_m,
            "{name}: cabin-sized fuselage {length} m vs registered {registered_m} m"
        );
        let _ = DesignOptimizer::new(config);
    }
}
