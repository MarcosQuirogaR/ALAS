// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `cargo xtask smoke`: the per-change check for anything that touches the
//! optimizer, the pipeline or the mass and mission models.
//!
//! For every registered preset it runs `smoke_cell` (a small binary in
//! alas-acceptance): the preset's native baseline analysis, then a two-stage
//! search in `fixed_requirement` mode with a **24 + 24 evaluation** budget, a
//! recorded seed and no clock-based stops, so a cell is reproducible. Cells run
//! as separate processes in parallel, bounded so that `cells x workers` stays
//! within the 15-thread budget of the shared host.
//!
//! A cell passes when the baseline analysis completes and the search ends in a
//! reported outcome (a feasible design, or "no feasible design" after its few
//! evaluations); a failed baseline or an unexpected search error fails it. It
//! shows that every preset still closes its baseline and survives a search; it
//! is not a validation of the results.
//!
//! Options: `--presets A,B`, `--evaluations N` (default 24), `--cells N`
//! (default 12: one wave on the 12 registered presets), `--workers N` (default
//! 1; the A380-800, about four times as costly per evaluation as the jets,
//! gets two more while the 15-thread budget allows), `--out DIR`, `--no-build`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::Value;

/// Threads the whole smoke run may use (16 on the shared host, one left free).
const THREAD_CAP: usize = 15;

struct Options {
    presets: Vec<String>,
    evaluations: u64,
    cells: usize,
    workers: usize,
    out: PathBuf,
    build: bool,
}

fn parse(root: &Path, args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        presets: Vec::new(),
        evaluations: 24,
        cells: 12,
        workers: 1,
        out: std::env::var_os("CARGO_TARGET_DIR")
            .map_or_else(|| root.join("target"), PathBuf::from)
            .join("alas-smoke"),
        build: true,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--presets" => {
                options.presets = value("--presets")?.split(',').map(str::to_owned).collect();
            }
            "--evaluations" => {
                options.evaluations = number("--evaluations", &value("--evaluations")?)?
            }
            "--cells" => options.cells = number("--cells", &value("--cells")?)? as usize,
            "--workers" => options.workers = number("--workers", &value("--workers")?)? as usize,
            "--out" => options.out = PathBuf::from(value("--out")?),
            "--no-build" => options.build = false,
            other => return Err(format!("unknown smoke argument: {other}")),
        }
    }
    if options.cells == 0 || options.workers == 0 {
        return Err("--cells and --workers must be at least 1".to_owned());
    }
    if options.cells * options.workers > THREAD_CAP {
        return Err(format!(
            "{} cells x {} workers = {} threads exceeds the {THREAD_CAP}-thread budget",
            options.cells,
            options.workers,
            options.cells * options.workers
        ));
    }
    Ok(options)
}

fn number(flag: &str, text: &str) -> Result<u64, String> {
    text.parse()
        .map_err(|_| format!("{flag} needs a whole number, got `{text}`"))
}

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let mut options = parse(root, args)?;
    let started = Instant::now();
    let cell_exe = if options.build {
        build(root, "smoke_cell")?
    } else {
        existing(root, "smoke_cell")?
    };
    if options.presets.is_empty() {
        options.presets = preset_names(&cell_exe)?;
    }
    println!(
        "smoke: {} presets, {}+{} evaluations, {} cells x {} workers, output {}",
        options.presets.len(),
        options.evaluations,
        options.evaluations,
        options.cells,
        options.workers,
        options.out.display()
    );
    let _ = std::fs::remove_dir_all(&options.out);
    std::fs::create_dir_all(&options.out)
        .map_err(|e| format!("cannot create {}: {e}", options.out.display()))?;

    let queue = Mutex::new(options.presets.iter().cloned().collect::<VecDeque<_>>());
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..options.cells.min(options.presets.len()) {
            scope.spawn(|| {
                while let Some(preset) = queue.lock().ok().and_then(|mut q| q.pop_front()) {
                    let cell = run_cell(&cell_exe, &preset, &options);
                    if let Ok(mut results) = results.lock() {
                        results.push((preset, cell));
                    }
                }
            });
        }
    });
    let mut results = results.into_inner().map_err(|e| e.to_string())?;
    results.sort_by(|a, b| a.0.cmp(&b.0));

    println!("\n{:<14} {:<8} {:>7}  detail", "preset", "status", "wall s");
    let mut failed = 0;
    for (preset, cell) in &results {
        let (status, secs, detail) = match cell {
            Ok(cell) => (cell.status.as_str(), cell.secs, cell.detail.as_str()),
            Err(message) => ("ERROR", 0.0, message.as_str()),
        };
        if status != "ok" {
            failed += 1;
        }
        println!("{preset:<14} {status:<8} {secs:>7.1}  {detail}");
    }
    println!(
        "\nsmoke: {} of {} cells succeeded in {:.0} s (logs and rows under {})",
        results.len() - failed,
        results.len(),
        started.elapsed().as_secs_f64(),
        options.out.display()
    );
    if failed == 0 {
        Ok(())
    } else {
        Err(format!("smoke: {failed} cell(s) failed"))
    }
}

struct Cell {
    status: String,
    secs: f64,
    detail: String,
}

fn run_cell(exe: &Path, preset: &str, options: &Options) -> Result<Cell, String> {
    let dir = options.out.join(preset);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let log = std::fs::File::create(dir.join("stderr.log")).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let output = Command::new(exe)
        .args(["--preset", preset, "--evaluations"])
        .arg(options.evaluations.to_string())
        .arg("--workers")
        .arg(workers_for(preset, options).to_string())
        .stderr(log)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let secs = started.elapsed().as_secs_f64();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row: Value = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| {
            format!(
                "exit {}, no result row; see {}",
                output.status,
                dir.join("stderr.log").display()
            )
        })?;
    let _ = std::fs::write(dir.join("result.json"), stdout.as_bytes());
    let (ok, detail) = judge(&row);
    Ok(Cell {
        status: if ok { "ok" } else { "FAIL" }.to_owned(),
        secs,
        detail,
    })
}

/// Worker threads of one cell: `--workers`, plus two for the A380-800 when the
/// thread budget has room, since it is the long pole of a parallel run.
fn workers_for(preset: &str, options: &Options) -> usize {
    let spare =
        THREAD_CAP.saturating_sub(options.cells.min(options.presets.len()) * options.workers);
    if preset == "A380-800" && spare >= 2 {
        options.workers + 2
    } else {
        options.workers
    }
}

/// Whether a `smoke_cell` row passes, and a one-line reading of it.
fn judge(row: &Value) -> (bool, String) {
    let baseline = &row["baseline"];
    let search = &row["search"];
    let number = |value: &Value, digits: usize| {
        value
            .as_f64()
            .map_or("-".to_owned(), |x| format!("{x:.digits$}"))
    };
    let baseline_ok = baseline["ok"] == true;
    let outcome = search["outcome"].as_str().unwrap_or("missing");
    let ok = baseline_ok && matches!(outcome, "feasible" | "no_feasible_design");
    let mut detail = format!(
        "baseline {} (mass {} kg, SM {}, {} s); search {outcome}",
        if baseline_ok { "ok" } else { "FAILED" },
        number(&baseline["total_mass_kg"], 0),
        number(&baseline["static_margin"], 3),
        number(&row["baseline_wall_s"], 0),
    );
    match outcome {
        "feasible" => detail.push_str(&format!(
            " ({}, best cost {}, {} analyses)",
            search["termination"].as_str().unwrap_or("-"),
            number(&search["best_cost"], 4),
            search["analysis_evaluations"]
        )),
        "no_feasible_design" => detail.push_str(&format!(
            " ({} candidates evaluated)",
            search["evaluated_candidates"]
        )),
        _ => {}
    }
    for error in [&baseline["error"], &search["error"]] {
        if let Some(text) = error.as_str() {
            detail.push_str(&format!("; error: {text}"));
        }
    }
    (ok, detail)
}

/// Build `bin` of alas-acceptance in the test profile (the one the test
/// builds already compiled the libraries for) and return its path.
fn build(root: &Path, bin: &str) -> Result<PathBuf, String> {
    println!("\ncargo build --profile test -p alas-acceptance --bin {bin}");
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "build",
            "--profile",
            "test",
            "-p",
            "alas-acceptance",
            "--bin",
            bin,
            "--message-format",
            "json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!("building {bin} failed"));
    }
    executable_in(&String::from_utf8_lossy(&output.stdout), bin)
        .ok_or_else(|| format!("cargo reported no executable for {bin}"))
}

/// The executable of the `compiler-artifact` message for binary `bin`.
fn executable_in(messages: &str, bin: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == bin)
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
}

/// With `--no-build`: the newest copy of `bin` under the target directory.
fn existing(root: &Path, bin: &str) -> Result<PathBuf, String> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let path = target
        .join("debug")
        .join(format!("{bin}{}", std::env::consts::EXE_SUFFIX));
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} is not built; run without --no-build",
            path.display()
        ))
    }
}

fn preset_names(exe: &Path) -> Result<Vec<String>, String> {
    let output = Command::new(exe)
        .arg("--list")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let names: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        return Err("smoke_cell --list listed no preset".to_owned());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn the_thread_budget_is_enforced() {
        let root = Path::new(".");
        assert!(parse(root, &args(&[])).is_ok());
        assert!(parse(root, &args(&["--cells", "8", "--workers", "2"])).is_err());
        assert!(parse(root, &args(&["--cells", "5", "--workers", "3"])).is_ok());
        assert!(parse(root, &args(&["--cells", "0"])).is_err());
        assert!(parse(root, &args(&["--nope"])).is_err());
    }

    #[test]
    fn the_executable_is_read_from_cargo_messages() {
        let messages = concat!(
            r#"{"reason":"compiler-artifact","target":{"name":"other"},"executable":"/x/other"}"#,
            "\n",
            r#"{"reason":"compiler-artifact","target":{"name":"audit"},"executable":"/x/audit.exe"}"#,
            "\n",
            r#"{"reason":"build-finished","success":true}"#
        );
        assert_eq!(
            executable_in(messages, "audit"),
            Some(PathBuf::from("/x/audit.exe"))
        );
        assert_eq!(executable_in(messages, "missing"), None);
    }
}
