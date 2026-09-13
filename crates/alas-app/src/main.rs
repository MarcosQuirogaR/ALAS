// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Binary executable entry point for ALAS.

// The shipped executable is a desktop application when it is opened without
// arguments (the normal Explorer launch).  Selecting the Windows GUI
// subsystem prevents Windows from creating a console window for that launch.
// Explicit command-line invocations still use the same `run_cli` path and
// retain their inherited stdout/stderr handles when started from a terminal.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = alas_app::cli::run_cli(&args);
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
