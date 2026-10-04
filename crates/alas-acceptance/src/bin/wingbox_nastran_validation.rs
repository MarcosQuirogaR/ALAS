// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproduce the native preset wing-box beam in MSC Nastran SOL 101.

// This executable owns the console summary of the retained validation files.
#![allow(clippy::print_stdout)]
// Test fixtures are deliberate inputs, so failed parsing is a failed assertion.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[path = "../wingbox_validation/mod.rs"]
mod validation;

fn main() -> std::io::Result<()> {
    validation::run(std::env::args().skip(1))
}
