// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Repository tasks: the checks that run before a commit, packaging, benchmarks, and backup.
//!
//! These live in a crate rather than a shell script because they have to run
//! identically from a terminal, from a hook and from an editor, on a machine
//! where the only tool guaranteed to exist is Cargo.

mod bench;
mod checks;
mod dist;
mod dist_archive;
mod dist_avl;
mod evidence;
mod ledger;
mod source_size;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let root = repo_root();

    let outcome = match task.as_deref() {
        Some("gate") => gate(&root),
        Some("checks") => run_checks(&root),
        Some("evidence-audit") => run_evidence_audit(&root),
        Some("install-hooks") => install_hooks(&root),
        Some("bench") => bench::run_benchmarks(&root),
        Some("dist") | Some("package") => dist::create_distribution(&root),
        Some("backup") => backup(&root),
        Some(other) => {
            println!("unknown task: {other}");
            usage();
            return ExitCode::FAILURE;
        }
        None => {
            usage();
            return ExitCode::SUCCESS;
        }
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            println!("\n{message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    println!(
        "cargo xtask <task>\n\
         \n\
         gate           formatting, lints, tests and the repository checks\n\
         checks         the repository checks alone, without invoking Cargo\n\
         evidence-audit audit ledger, fixture and generator evidence bookkeeping\n\
         install-hooks  install the pre-commit hook that runs the gate\n\
         bench          build and execute the release computational benchmark suite\n\
         dist           compile release binary and assemble standalone distribution archive\n\
         package        alias for dist\n\
         backup         write a git bundle to the synced backup directory"
    );
}

fn run_evidence_audit(root: &Path) -> Result<(), String> {
    let findings = evidence::check(root)?;
    println!(
        "evidence-audit: presence and linkage only; it does not establish physical correctness"
    );
    if findings.is_empty() {
        println!("evidence-audit: pass");
        return Ok(());
    }
    for finding in &findings {
        println!("{finding}");
    }
    Err(format!("evidence-audit: {} finding(s)", findings.len()))
}

/// The full gate, in increasing order of cost.
///
/// The repository checks run first because they need no compilation, so a file
/// that is too long or missing a licence header fails in under a second rather
/// than after a full build. Every step below runs even if an earlier one
/// failed, an array literal evaluates all of its elements before
/// `report_gate_results` sees any of them, so one failing step can never hide
/// the others.
fn gate(root: &Path) -> Result<(), String> {
    let results = [
        run_checks(root),
        cargo(root, &["fmt", "--all", "--check"]),
        cargo(
            root,
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        cargo(root, &["test", "--workspace"]),
    ];
    report_gate_results(results)
}

/// Reports every failed gate step instead of stopping at the first one.
fn report_gate_results(results: [Result<(), String>; 4]) -> Result<(), String> {
    let failures: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    if failures.is_empty() {
        println!("\ngate: pass");
        return Ok(());
    }
    for failure in &failures {
        println!("\ngate: {failure}");
    }
    Err(format!("gate: {} of 4 check(s) failed", failures.len()))
}

fn run_checks(root: &Path) -> Result<(), String> {
    let sources = checks::rust_sources(root)?;
    println!("checking {} source files", sources.len());

    let mut findings = Vec::new();
    for path in &sources {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        findings.extend(checks::check_file(root, path, &text));
    }
    findings.extend(ledger::check(root)?);
    findings.extend(source_size::check(root, &sources)?);

    if findings.is_empty() {
        println!("checks: pass");
        return Ok(());
    }

    for finding in &findings {
        println!("{finding}");
    }
    Err(format!("checks: {} finding(s)", findings.len()))
}

fn install_hooks(root: &Path) -> Result<(), String> {
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let body = "#!/bin/sh\nexec cargo xtask gate\n";
    std::fs::write(&hook, body).map_err(|e| format!("cannot write {}: {e}", hook.display()))?;
    println!("installed {}", hook.display());
    Ok(())
}

/// Write a bundle of the whole repository into the synced backup directory.
///
/// This repository has no remote by design, and it lives outside the synced
/// folder because Cargo's build directory defeats a file-level sync client. A
/// bundle is a single file, so it syncs cleanly and still contains every branch
/// and every commit.
fn backup(root: &Path) -> Result<(), String> {
    let dir = PathBuf::from(std::env::var("USERPROFILE").map_err(|_| "USERPROFILE is unset")?)
        .join("OneDrive")
        .join("Proyectos")
        .join("Universidad")
        .join("ALAS-backups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let target = dir.join("alas-rust.bundle");
    let status = Command::new("git")
        .current_dir(root)
        .args(["bundle", "create"])
        .arg(&target)
        .arg("--all")
        .status()
        .map_err(|e| format!("cannot run git: {e}"))?;

    if !status.success() {
        return Err("git bundle failed".to_owned());
    }
    println!("wrote {}", target.display());
    Ok(())
}

fn cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    println!("\ncargo {}", args.join(" "));
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(args)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed", args.join(" ")))
    }
}

/// The workspace root, found from this crate's manifest rather than the working
/// directory, so the task behaves the same wherever it is invoked from.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or(manifest.clone(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::report_gate_results;

    #[test]
    fn report_gate_results_passes_when_all_steps_pass() {
        let results = [Ok(()), Ok(()), Ok(()), Ok(())];
        assert!(report_gate_results(results).is_ok());
    }

    #[test]
    fn report_gate_results_reports_every_failure_not_just_the_first() {
        let results = [
            Err("checks: 1 finding(s)".to_owned()),
            Ok(()),
            Err("cargo clippy --workspace --all-targets -- -D warnings failed".to_owned()),
            Ok(()),
        ];
        let error = report_gate_results(results).expect_err("two steps failed");
        assert_eq!(error, "gate: 2 of 4 check(s) failed");
    }
}
