// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stable filenames for figures exported from the registry.

/// Return the stable exported filename stem while preserving dispatch IDs.
pub fn export_file_stem(id: &str) -> &str {
    if id == "asb_threeview" {
        "threeview"
    } else {
        id
    }
}
