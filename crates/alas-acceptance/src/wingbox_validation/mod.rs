// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validation-only beam export and retained external numerical evidence.

mod deck;
mod f06;
mod model;
mod quadrature;
mod report;
mod shell;

use alas_exec::ToolLocator;
use alas_struct::nastran::run_nastran_with_solver;
use std::{fs, io, path::PathBuf};

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> io::Result<()> {
    let args: Vec<_> = arguments.collect();
    let scratch_dir = std::env::temp_dir().join("alas-nastran");
    let mut directory = scratch_dir.clone();
    let mut executable = None;
    let mut solver = None;
    let mut write_only = false;
    let mut shell_enabled = false;
    let mut shell_divisions = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--write-only" => write_only = true,
            "--shell" => shell_enabled = true,
            "--output-dir" | "--nastran" | "--solver" | "--shell-divisions" => {
                let option = &args[index];
                index += 1;
                let value = args.get(index).ok_or_else(|| io::Error::other("missing option value"))?;
                match option.as_str() {
                    "--output-dir" => directory = PathBuf::from(value),
                    "--nastran" => executable = Some(PathBuf::from(value)),
                    "--shell-divisions" => shell_divisions = Some(value.parse::<usize>().map_err(io::Error::other)?),
                    _ => solver = Some(PathBuf::from(value)),
                }
            }
            _ => return Err(io::Error::other("usage: wingbox_nastran_validation [--output-dir <temp>/alas-nastran/<run>] [--nastran <launcher>] [--solver <solver>] [--write-only] [--shell] [--shell-divisions <count>]")),
        }
        index += 1;
    }
    fs::create_dir_all(&scratch_dir)?;
    let scratch_root = fs::canonicalize(&scratch_dir)?;
    let workspace = std::env::current_dir()?;
    let candidate = workspace.join(&directory);
    if directory
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
        || !candidate.starts_with(&scratch_dir)
    {
        return Err(io::Error::other(
            "validation output must be under the alas-nastran scratch directory",
        ));
    }
    fs::create_dir_all(&directory)?;
    let directory = fs::canonicalize(directory)?;
    if !directory.starts_with(&scratch_root) {
        return Err(io::Error::other(
            "validation output must be under the alas-nastran scratch directory",
        ));
    }
    let locator = ToolLocator::for_current_process();
    let environment = locator.resolve_environment(
        std::path::Path::new(""),
        executable.as_deref().unwrap_or(std::path::Path::new("")),
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
    );
    let executable = executable.or(environment.nastran_exe);
    let solver = solver.or(environment.nastran_solver);
    let mut comparisons = Vec::new();
    for name in ["A320-200", "B787-9", "AVE"] {
        let model = model::build(name)?;
        let case_dir = directory.join(name);
        fs::create_dir_all(&case_dir)?;
        let path = case_dir.join("beam.bdf");
        if path.exists() {
            return Err(io::Error::other(format!(
                "refusing to overwrite {}",
                path.display()
            )));
        }
        fs::write(&path, deck::beam(&model))?;
        fs::write(
            case_dir.join("native.json"),
            serde_json::to_vec_pretty(&model.evidence()).map_err(io::Error::other)?,
        )?;
        if write_only {
            if shell_enabled {
                let text = shell_deck(&model, shell_divisions)?;
                fs::write(case_dir.join("shell.bdf"), text)?;
            }
            continue;
        }
        let exe = executable
            .as_deref()
            .ok_or_else(|| io::Error::other("MSC Nastran not discovered; use --nastran"))?;
        let outcome = run_nastran_with_solver(&path, exe, solver.as_deref(), 120.0);
        fs::write(
            case_dir.join("run.txt"),
            format!(
                "launcher={}\nsolver_override={solver:?}\n{outcome:?}\n",
                exe.display()
            ),
        )?;
        if !outcome.ok {
            return Err(io::Error::other(format!("{name}: {}", outcome.detail)));
        }
        let result = f06::read(&fs::read_to_string(path.with_extension("f06"))?)?;
        let mut comparison = report::compare(&model, &result)?;
        if shell_enabled {
            let shell_path = case_dir.join("shell.bdf");
            match shell_deck(&model, shell_divisions) {
                Ok(deck) => {
                    fs::write(&shell_path, deck)?;
                    let outcome =
                        run_nastran_with_solver(&shell_path, exe, solver.as_deref(), 120.0);
                    fs::write(case_dir.join("shell_run.txt"), format!("{outcome:?}\n"))?;
                    if outcome.ok {
                        let result =
                            f06::read(&fs::read_to_string(shell_path.with_extension("f06"))?)?;
                        comparison["shell"] = report::shell_compare(&model, &result)?;
                    } else {
                        comparison["shell"] = serde_json::json!({"error": outcome.detail});
                    }
                }
                Err(error) => comparison["shell"] = serde_json::json!({"error": error.to_string()}),
            }
        }
        comparisons.push(comparison);
    }
    if !write_only {
        report::write(&directory, &comparisons)?;
    }
    // The console is owned by this validation executable.
    println!("Wing-box validation retained in {}", directory.display());
    Ok(())
}

fn shell_deck(model: &model::Model, divisions: Option<usize>) -> io::Result<String> {
    divisions.map_or_else(
        || shell::deck(model),
        |count| shell::deck_with_refinement(model, count),
    )
}
