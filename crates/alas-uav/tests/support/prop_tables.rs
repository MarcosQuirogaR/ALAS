// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Points the process at the SYNTHETIC propeller tables in `fixtures/prop_data`.
//!
//! ALAS ships no manufacturer propeller data, so tests that need a table use
//! these made-up files.  Results from them test code paths, not propellers.

use std::path::PathBuf;

use alas_uav::propulsion_electric::prop_data::set_prop_data_dir;

pub fn use_synthetic_tables() {
    set_prop_data_dir(Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prop_data"),
    ));
}
