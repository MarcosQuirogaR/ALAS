// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible wall-time harness for an already generated NASTRAN-95 deck.
//!
//! This intentionally exercises the same supervised subprocess path as ALAS.
//! It writes the complete print file for cross-build numerical comparison and
//! emits one compact machine-readable result on stdout.  An optional final
//! argument selects OCMEM in words, allowing a campaign to reserve the rest
//! of the executable's COMMON /ZZZZZZ/ for NASTRAN-95's in-memory database.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use alas_struct::nastran95::{run_nastran95, Nastran95Solver, RunOutcome};

fn main() {
    if let Err(error) = run() {
        let _ = writeln!(std::io::stderr(), "{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let solver_root = required(&mut args, "solver root")?;
    let runtime = required(&mut args, "Fortran runtime directory")?;
    let deck_path = required(&mut args, "input deck")?;
    let work_dir = required(&mut args, "work directory")?;
    let output_path = required(&mut args, "print output")?;
    let timeout = args
        .next()
        .ok_or_else(|| "missing timeout in seconds".to_owned())?
        .to_string_lossy()
        .parse::<f64>()
        .map_err(|error| format!("invalid timeout: {error}"))?;
    let open_core = args
        .next()
        .map(|value| value.to_string_lossy().into_owned());
    if args.next().is_some() {
        return Err("unexpected extra arguments".to_owned());
    }

    let solver = Nastran95Solver::from_paths(
        &solver_root,
        Some(&runtime),
        Some(Path::new("C:/n95rf")),
        open_core.as_deref(),
    )
    .ok_or_else(|| format!("invalid solver root: {}", solver_root.display()))?;
    let deck = fs::read_to_string(&deck_path)
        .map_err(|error| format!("cannot read {}: {error}", deck_path.display()))?;

    let started = Instant::now();
    let outcome = run_nastran95(&solver, &deck, &work_dir, timeout);
    let elapsed = started.elapsed().as_secs_f64();
    match outcome {
        RunOutcome::Print(print) => {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "cannot create output directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
            fs::write(&output_path, &print)
                .map_err(|error| format!("cannot write {}: {error}", output_path.display()))?;
            writeln!(
                std::io::stdout(),
                "{{\"status\":\"ok\",\"elapsed_s\":{elapsed:.6},\"print_bytes\":{}}}",
                print.len()
            )
            .map_err(|error| format!("cannot write benchmark result: {error}"))?;
            Ok(())
        }
        RunOutcome::Failed(detail) => Err(format!("solve failed after {elapsed:.6}s: {detail}")),
    }
}

fn required(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}"))
}
