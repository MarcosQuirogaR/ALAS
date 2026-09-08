// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures.py
// Reference: alas @ rust-port-baseline.

//! Figure registry and metadata catalogue for interactive UI and batch export.
#![allow(missing_docs)] // The registry table is self-describing data; field docs add noise to every entry.

include!("registry_parts/part_01.rs");
include!("registry_parts/part_02.rs");
