// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Generate an inspectable OpenFOAM case without launching a solver.
//!
//! Usage:
//!
//! `cargo run -p alas-cfd --example generate_airfoil_case -- <case-dir> [airfoil]`

use std::env;
use std::path::PathBuf;

use alas_cfd::{generate_case, CfdStudyConfig};

// This binary example's only job is to report the generated case (or the
// failure) on the console for a human running it manually; there is no other
// channel to route this through, so stdout/stderr are the intended sinks,
// not a bypass of library logging.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn main() {
    let mut args = env::args_os().skip(1);
    let case_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/airfoil-cfd/example"));
    let mut config = CfdStudyConfig::default();
    if let Some(airfoil) = args.next().and_then(|value| value.into_string().ok()) {
        config.airfoil_name = airfoil;
    }
    match generate_case(&config, &case_dir) {
        Ok(case) => {
            println!(
                "Generated {} at {} (hash {}, Re={:.6e}, files={})",
                case.airfoil.name,
                case.path.display(),
                case.airfoil.coordinate_hash,
                case.effective_reynolds,
                case.files.len()
            );
        }
        Err(error) => {
            eprintln!("Could not generate case: {error}");
            std::process::exit(2);
        }
    }
}
