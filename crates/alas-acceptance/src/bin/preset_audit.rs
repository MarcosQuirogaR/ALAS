// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Runs the public pipeline for every aircraft preset and prints or retains its audit artifacts.

// This command-line report is the intended user-facing output of the binary.
#![allow(clippy::print_stdout)]

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use alas_acceptance::{format_matrix_json, format_matrix_report, run_acceptance_matrix};

fn main() -> io::Result<()> {
    let output_dir = parse_output_dir(env::args().skip(1))?;
    let report = run_acceptance_matrix();
    let text = format_matrix_report(&report);
    if let Some(output_dir) = output_dir {
        fs::create_dir_all(&output_dir)?;
        fs::write(output_dir.join("preset_acceptance_matrix.txt"), &text)?;
        let json = format_matrix_json(&report).map_err(io::Error::other)?;
        fs::write(output_dir.join("preset_acceptance_matrix.json"), json)?;
    }
    println!("{text}");
    Ok(())
}

fn parse_output_dir(arguments: impl Iterator<Item = String>) -> io::Result<Option<PathBuf>> {
    let values = arguments.collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(None),
        [flag, path] if flag == "--output-dir" => Ok(Some(PathBuf::from(path))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: preset_audit [--output-dir <directory>]",
        )),
    }
}
