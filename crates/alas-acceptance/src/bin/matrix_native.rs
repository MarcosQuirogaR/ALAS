// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native acceptance matrix runner: every registered preset x the three
//! user-facing MTOW modes, each cell one optimization run with its native
//! baseline, under evaluation budgets from a table file.
//!
//! Each cell is one `optimization_preset_audit measure` child process
//! (`--stop-on-evaluations`, so the search stops on evaluation counts only and
//! the result is independent of host load, worker count and cell parallelism).
//! `--cells-parallel P` cells run at once with `--workers W` search lanes each;
//! `P x W` must not exceed [`MAX_THREADS`]. A cell is skipped when its
//! `measurement_result.json` records `status` success or infeasible (resume);
//! `--force` reruns everything.
//!
//! Outputs under `--output-dir`: `cells/<preset>@<mode>/` (the child's own
//! artefacts plus `runner_cell.json`), `matrix_summary.json` and
//! `matrix_summary.csv`, both rebuilt from the cell directories after every
//! finished cell. `matrix_native compare A.json B.json` lists cells whose
//! determinism digest differs between two summaries.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[path = "matrix_native/budget.rs"]
mod budget;
#[path = "matrix_native/summary.rs"]
mod summary;

use budget::BudgetTable;

/// Total thread budget of the host shared with other lanes: `P x W` is capped
/// here (16 logical cores, one left for the OS and the runner).
const MAX_THREADS: u64 = 15;

/// Seconds the child gets after its own cooperative `--timeout-s` guard fires
/// before the runner kills it.
const KILL_GRACE_S: u64 = 120;

const USAGE: &str = "usage:\n  matrix_native run --output-dir DIR [--presets all|A,B] [--modes all|fixed_requirement,mtow_band,payload_adjusted] [--cells-parallel P] [--workers W] [--budget matrix|FILE] [--cell-timeout-s N] [--seed N] [--force] [--dry-run] [--summary-only] [--audit-exe PATH]\n  matrix_native compare SUMMARY_A.json SUMMARY_B.json\n\nP x W must be <= 15. Defaults: presets all, modes all, P 3, W 5, budget matrix (embedded table), cell timeout 900 s, seed from the table.";

const MODES: [&str; 3] = ["fixed_requirement", "mtow_band", "payload_adjusted"];

/// One matrix cell.
#[derive(Debug, Clone)]
pub struct Cell {
    /// Aircraft preset name.
    pub preset: String,
    /// User-facing MTOW mode name.
    pub mode: String,
    /// Screening evaluation budget.
    pub screening: u64,
    /// Refinement evaluation budget.
    pub refinement: u64,
    /// Relative cost used only to order the queue (heaviest first).
    pub weight: f64,
}

impl Cell {
    /// Directory-safe cell identifier `preset@mode`.
    pub fn id(&self) -> String {
        format!("{}@{}", self.preset, self.mode)
    }
}

struct RunArgs {
    output_dir: PathBuf,
    presets: Vec<String>,
    modes: Vec<String>,
    cells_parallel: u64,
    workers: u64,
    budget: String,
    cell_timeout_s: u64,
    seed: Option<u64>,
    force: bool,
    dry_run: bool,
    summary_only: bool,
    audit_exe: Option<PathBuf>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn list(value: &str, all: Vec<String>, what: &str) -> io::Result<Vec<String>> {
    if value == "all" {
        return Ok(all);
    }
    let chosen: Vec<String> = value.split(',').map(|s| s.trim().to_owned()).collect();
    for name in &chosen {
        if !all.contains(name) {
            return Err(invalid(format!(
                "unknown {what} '{name}'; available: {}",
                all.join(", ")
            )));
        }
    }
    Ok(chosen)
}

fn parse_run(values: &[String]) -> io::Result<RunArgs> {
    let mut out = None;
    let (mut presets, mut modes) = ("all".to_owned(), "all".to_owned());
    let (mut p, mut w) = (3_u64, 5_u64);
    let mut budget = "matrix".to_owned();
    let mut cell_timeout_s = 900_u64;
    let (mut seed, mut force, mut dry_run, mut audit_exe) = (None, false, false, None);
    let mut summary_only = false;
    let mut it = values.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| invalid(format!("{name} requires a value")))
        };
        let number = |name: &str, text: String| {
            text.parse::<u64>()
                .map_err(|_| invalid(format!("{name} requires an integer")))
        };
        match flag.as_str() {
            "--output-dir" => out = Some(PathBuf::from(value(flag)?)),
            "--presets" => presets = value(flag)?,
            "--modes" => modes = value(flag)?,
            "--cells-parallel" => p = number(flag, value(flag)?)?,
            "--workers" => w = number(flag, value(flag)?)?,
            "--budget" => budget = value(flag)?,
            "--cell-timeout-s" => cell_timeout_s = number(flag, value(flag)?)?,
            "--seed" => seed = Some(number(flag, value(flag)?)?),
            "--audit-exe" => audit_exe = Some(PathBuf::from(value(flag)?)),
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            "--summary-only" => summary_only = true,
            other => return Err(invalid(format!("unrecognized argument '{other}'"))),
        }
    }
    check_threads(p, w)?;
    let all_presets = alas_config::presets::available()
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(RunArgs {
        output_dir: out.ok_or_else(|| invalid("--output-dir is required"))?,
        presets: list(&presets, all_presets, "preset")?,
        modes: list(&modes, MODES.map(str::to_owned).to_vec(), "mode")?,
        cells_parallel: p,
        workers: w,
        budget,
        cell_timeout_s,
        seed,
        force,
        dry_run,
        summary_only,
        audit_exe,
    })
}

/// `P x W` must fit the shared host.
fn check_threads(p: u64, w: u64) -> io::Result<()> {
    if p == 0 || w == 0 {
        return Err(invalid("--cells-parallel and --workers must be at least 1"));
    }
    if p.saturating_mul(w) > MAX_THREADS {
        return Err(invalid(format!(
            "--cells-parallel {p} x --workers {w} = {} exceeds the {MAX_THREADS}-thread budget",
            p.saturating_mul(w)
        )));
    }
    Ok(())
}

fn audit_exe(args: &RunArgs) -> io::Result<PathBuf> {
    if let Some(path) = &args.audit_exe {
        return Ok(path.clone());
    }
    let me = env::current_exe()?;
    let name = format!("optimization_preset_audit{}", env::consts::EXE_SUFFIX);
    let sibling = me.with_file_name(name);
    if sibling.exists() {
        Ok(sibling)
    } else {
        Err(invalid(format!(
            "{} not found next to the runner; build it (cargo build --profile harness -p alas-acceptance --bins) or pass --audit-exe",
            sibling.display()
        )))
    }
}

/// Outcome of one child process, recorded in `runner_cell.json`.
fn run_cell(exe: &Path, args: &RunArgs, table: &BudgetTable, cell: &Cell) -> Value {
    let dir = args.output_dir.join("cells").join(cell.id());
    let _ = fs::remove_file(dir.join("measurement_result.json"));
    if let Err(error) = fs::create_dir_all(&dir) {
        return json!({"runner_status": "spawn_error", "detail": error.to_string()});
    }
    let seed = args.seed.unwrap_or(table.seed);
    let open = |name: &str| fs::File::create(dir.join(name));
    let (Ok(stdout), Ok(stderr)) = (open("stdout.log"), open("stderr.log")) else {
        return json!({"runner_status": "spawn_error", "detail": "cannot create logs"});
    };
    let started = Instant::now();
    let mut command = Command::new(exe);
    command
        .arg("measure")
        .args(["--preset", &cell.preset])
        .arg("--output-dir")
        .arg(&dir)
        .args(["--mtow-mode", &cell.mode])
        .args(["--experiment", &table.name])
        .args(["--seed", &seed.to_string()])
        .arg("--stop-on-evaluations")
        .args(["--screening-max-evaluations", &cell.screening.to_string()])
        .args(["--max-evaluations", &cell.refinement.to_string()])
        .args(["--workers", &args.workers.to_string()])
        .args(["--timeout-s", &args.cell_timeout_s.to_string()])
        .arg("--native-only")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return json!({"runner_status": "spawn_error", "detail": error.to_string()}),
    };
    let hard_limit = Duration::from_secs(args.cell_timeout_s + KILL_GRACE_S);
    let (status, code) = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break ("exited", exit.code()),
            Ok(None) if started.elapsed() > hard_limit => {
                let _ = child.kill();
                let _ = child.wait();
                break ("killed_timeout", None);
            }
            Ok(None) => thread::sleep(Duration::from_millis(250)),
            Err(_) => break ("wait_error", None),
        }
    };
    json!({
        "runner_status": status,
        "exit_code": code,
        "child_wall_s": started.elapsed().as_secs_f64(),
        "budget": {"screening": cell.screening, "refinement": cell.refinement, "table": table.name},
        "seed": seed,
        "workers": args.workers,
        "cell_timeout_s": args.cell_timeout_s,
    })
}

fn run(values: &[String]) -> io::Result<()> {
    let args = parse_run(values)?;
    let table = BudgetTable::load(&args.budget)?;
    let mut cells: Vec<Cell> = Vec::new();
    for preset in &args.presets {
        for mode in &args.modes {
            cells.push(table.cell(preset, mode));
        }
    }
    // Heaviest first: the long cells start early and the tail is short.
    // The order never changes a cell's result.
    let mut queue_order = cells.clone();
    queue_order.sort_by(|a, b| {
        b.weight
            .total_cmp(&a.weight)
            .then_with(|| a.id().cmp(&b.id()))
    });
    let pending: VecDeque<Cell> = queue_order
        .into_iter()
        .filter(|cell| args.force || !summary::cell_done(&args.output_dir, cell))
        .collect();
    let skipped = cells.len() - pending.len();
    eprintln!(
        "[matrix_native] {} cells, {} to run, {} resumed; P={} W={} budget={} ({}) timeout={} s",
        cells.len(),
        pending.len(),
        skipped,
        args.cells_parallel,
        args.workers,
        table.name,
        table.status,
        args.cell_timeout_s
    );
    if args.summary_only {
        let counts = summary::write(&args.output_dir, &cells, &table, None)?;
        println!(
            "summary rebuilt: {} of {} cells completed",
            counts.success, counts.total
        );
        return Ok(());
    }
    if args.dry_run {
        for cell in &pending {
            println!(
                "{} screening={} refinement={}",
                cell.id(),
                cell.screening,
                cell.refinement
            );
        }
        return Ok(());
    }
    fs::create_dir_all(args.output_dir.join("cells"))?;
    let exe = audit_exe(&args)?;
    let queue = Mutex::new(pending);
    let busy_s = Mutex::new(0.0_f64);
    let started = Instant::now();
    let finished = Mutex::new(0_usize);
    thread::scope(|scope| {
        for _ in 0..args.cells_parallel {
            scope.spawn(|| loop {
                let next = queue.lock().ok().and_then(|mut q| q.pop_front());
                let Some(cell) = next else { break };
                let record = run_cell(&exe, &args, &table, &cell);
                let wall = record["child_wall_s"].as_f64().unwrap_or(0.0);
                let dir = args.output_dir.join("cells").join(cell.id());
                let _ = fs::write(
                    dir.join("runner_cell.json"),
                    serde_json::to_vec_pretty(&record).unwrap_or_default(),
                );
                if let Ok(mut total) = busy_s.lock() {
                    *total += wall;
                }
                let count = finished.lock().map_or(0, |mut n| {
                    *n += 1;
                    *n
                });
                eprintln!(
                    "[matrix_native] finished {} ({count}/{}) {} in {wall:.1} s, elapsed {:.0} s",
                    cell.id(),
                    cells.len() - skipped,
                    record["runner_status"].as_str().unwrap_or("?"),
                    started.elapsed().as_secs_f64()
                );
                let _ = summary::write(&args.output_dir, &cells, &table, None);
            });
        }
    });
    let wall = started.elapsed().as_secs_f64();
    let busy = busy_s.into_inner().unwrap_or(0.0);
    let invocation = json!({
        "cells_parallel": args.cells_parallel,
        "workers": args.workers,
        "thread_budget": MAX_THREADS,
        "invocation_wall_s": wall,
        "cells_run": cells.len() - skipped,
        "cells_resumed": skipped,
        "cell_slot_utilization": if wall > 0.0 { busy / (wall * args.cells_parallel as f64) } else { 0.0 },
        "cell_slot_utilization_definition": "sum of child wall times over (invocation wall x cells-parallel); search-lane utilization is per stage in each cell",
    });
    let summary = summary::write(&args.output_dir, &cells, &table, Some(invocation))?;
    println!(
        "matrix_native: {} cells, {} completed, wall {wall:.0} s, summary {}",
        summary.total,
        summary.success,
        args.output_dir.join("matrix_summary.json").display()
    );
    if summary.success < summary.total {
        std::process::exit(3);
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("compare") if args.len() == 3 => summary::compare(&args[1], &args[2]),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
// A failed unwrap in a test is the assertion failing.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_budget_is_enforced() {
        assert!(check_threads(3, 5).is_ok());
        assert!(check_threads(15, 1).is_ok());
        assert!(check_threads(4, 4).is_err());
        assert!(check_threads(0, 4).is_err());
        assert!(check_threads(u64::MAX, 2).is_err());
    }

    #[test]
    fn lists_reject_unknown_names() {
        let all = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(list("all", all.clone(), "x").unwrap().len(), 2);
        assert_eq!(list("b", all.clone(), "x").unwrap(), vec!["b".to_owned()]);
        assert!(list("c", all, "x").is_err());
    }
}
