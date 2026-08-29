// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Driving the built NASTRAN-95 solver, and reading its print file back.
//!
//! The run contract is nothing like the modern one [`crate::nastran::run`]
//! drives. There is no deck argument and no result file keyword: the solver
//! reads the deck on **stdin**, writes the print file on **stdout**, and is
//! configured entirely through environment variables -- `DBMEM`, `OCMEM`,
//! `RFDIR`, `DIRCTY` and the rest of `mds/nastrn.f`'s `GETENV` list. Four of
//! those cost a run each to discover and are honoured here: `NASINFO` is read
//! from `$RFDIR` and not the working directory, so a copy with its timing
//! constants switched on is staged there; most `/DOSNAM/` paths are
//! `CHARACTER*72`, but the rigid-format loader has its own 44-byte `RFDIR` and
//! destination buffer. Its longest shipped member is `AERO10`, so the staged
//! directory itself can occupy at most 37 bytes. A long scratch directory still
//! comes back empty rather than truncated, and a long rigid-format directory
//! truncates a member name and fails in `RFOPEN`; this runner checks the latter
//! before it launches and accepts a separately configured short stage directory.
//! When that stage is configured, the solver's transient work directory is
//! placed beside it rather than beneath the retained artifact tree: the legacy
//! `DOSNAM` buffers are only 72 bytes, while a useful desktop output directory
//! commonly exceeds that once `run.log` or `scr` is appended.
//! `OCMEM` may select any allocation up to the executable's fixed open-core
//! array. Current local builds write that limit beside `nastran.exe`, letting
//! the runner use the compiled capacity by default and reject invalid requests
//! before the Fortran program exits successfully without results.
//! The deck is fed with its carriage returns stripped, since a bare `CR` lands
//! in column 81 and the scanner rejects the card; and the runtime directory
//! carrying `libgfortran` has to be on `PATH`.
//!
//! The parser is shared with the modern solver on purpose: NASTRAN-95 and MSC
//! print the same `D I S P L A C E M E N T   V E C T O R` and
//! `R E A L   E I G E N V A L U E S` tables, in the same columns, because one
//! is the other's ancestor. So the cross-solver comparison reads both outputs
//! through [`read_displacement_tables`] and [`read_eigenvalues`], and any
//! difference it finds is in the solve and not in two different parsers.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};

use crate::nastran::text::{self};

/// How often the run loop checks whether the solver has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// `RFOPEN` declares both `RFDIR` and its assembled filename as
/// `CHARACTER*44`. `AERO10` is the longest rigid-format member, and the slash
/// is added by the loader, leaving 37 bytes for the directory itself.
const MAX_RIGID_FORMAT_DIR_BYTES: usize = 37;
const MAX_DOSNAM_PATH_BYTES: usize = 72;
/// Older locally built executables do not advertise their fixed `COMMON
/// /ZZZZZZ/` allocation. Retain their historic cap rather than claiming a
/// larger allocation the binary cannot honor.
const LEGACY_MAX_OPEN_CORE_WORDS: u64 = 14_000_000;
const OPEN_CORE_LIMIT_FILE: &str = "nastran95-open-core.txt";

/// Where the built solver and its runtime are, resolved from the environment.
///
/// `ALAS_NASTRAN95_DIR` is the checked-out solver tree -- the executable is
/// `build/bin/nastran.exe` beneath it and the run-time files are its `rf/` --
/// and `ALAS_NASTRAN95_RUNTIME` is the directory holding `libgfortran`, which
/// has to be on `PATH` for the executable to load. `ALAS_NASTRAN95_RF_STAGE`
/// may name a dedicated short absolute directory for copied rigid-format files
/// when the requested run directory is too long for `RFOPEN`. Both solver
/// variables unset means no solver, which a caller treats as "skip", not
/// "fail".
#[derive(Debug, Clone)]
pub struct Nastran95Solver {
    /// The `nastran.exe` to run.
    pub exe: PathBuf,
    /// The `rf/` directory `NASINFO` is staged from.
    pub rf_source: PathBuf,
    /// The directory to prepend to `PATH`, or `None` if the runtime is already
    /// reachable.
    pub runtime: Option<PathBuf>,
    /// Optional short absolute directory holding the staged rigid-format files.
    pub rf_stage: Option<PathBuf>,
    /// Explicit open-core allocation for this solver, if a caller configured it.
    pub open_core_words: Option<String>,
    /// Largest run-time OCMEM allocation compiled into this executable.
    pub max_open_core_words: u64,
}

impl Nastran95Solver {
    /// Construct the local solver from an explicit installation and optional
    /// runtime/staging paths, if the required executable and rigid formats exist.
    pub fn from_paths(
        dir: &Path,
        runtime: Option<&Path>,
        rf_stage: Option<&Path>,
        open_core_words: Option<&str>,
    ) -> Option<Self> {
        if dir.as_os_str().is_empty() {
            return None;
        }
        let exe = dir.join("build").join("bin").join("nastran.exe");
        let rf_source = dir.join("rf");
        if !exe.exists() || !rf_source.join("NASINFO").exists() {
            return None;
        }
        let max_open_core_words = compiled_open_core_limit(&exe);
        Some(Self {
            exe,
            rf_source,
            runtime: runtime.map(Path::to_path_buf),
            rf_stage: rf_stage.map(Path::to_path_buf),
            open_core_words: open_core_words
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            max_open_core_words,
        })
    }

    /// The solver named by the environment, if it is present on disk.
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var("ALAS_NASTRAN95_DIR").ok()?);
        let runtime = std::env::var_os("ALAS_NASTRAN95_RUNTIME").map(PathBuf::from);
        let rf_stage = std::env::var_os("ALAS_NASTRAN95_RF_STAGE").map(PathBuf::from);
        let open_core_words = std::env::var("ALAS_NASTRAN95_OCMEM").ok();
        Self::from_paths(
            &dir,
            runtime.as_deref(),
            rf_stage.as_deref(),
            open_core_words.as_deref(),
        )
    }

    /// Return a short transient run directory when a short rigid-format stage
    /// is configured, keeping the long user-facing artifact directory free to
    /// retain the BDF and F06 after the solver exits.
    pub(crate) fn workspace_for(&self, artifact_dir: &Path) -> Result<PathBuf, String> {
        let Some(stage) = self.rf_stage.as_deref() else {
            return Ok(artifact_dir.to_path_buf());
        };
        let Some(parent) = stage.parent() else {
            return Err(format!(
                "NASTRAN-95 RF staging directory has no parent: {}",
                stage.display()
            ));
        };
        let Some(solution) = artifact_dir.file_name() else {
            return Err(format!(
                "cannot derive NASTRAN-95 solution name from {}",
                artifact_dir.display()
            ));
        };
        Ok(parent
            .join("nas-run")
            .join(std::process::id().to_string())
            .join(solution))
    }
}

/// What one solve produced, or why it did not.
#[derive(Debug, Clone)]
pub enum RunOutcome {
    /// The print file the solver wrote to stdout.
    Print(String),
    /// The solve did not produce a usable print file; the string says why.
    Failed(String),
}

/// Solve `deck` in `work_dir`, returning the print file or the reason it failed.
///
/// `work_dir` must fit the solver's legacy path buffers (see the module doc) and
/// is emptied first, so a caller passes a scratch directory it owns. Relative
/// paths are resolved before the child changes its working directory. Never
/// returns an `Err`: every failure is a [`RunOutcome::Failed`] carrying a
/// message, the contract [`crate::nastran::run`] keeps for the same reason.
pub fn run_nastran95(
    solver: &Nastran95Solver,
    deck: &str,
    work_dir: &Path,
    timeout_seconds: f64,
) -> RunOutcome {
    let timeout = match Duration::try_from_secs_f64(timeout_seconds) {
        Ok(timeout) if !timeout.is_zero() => timeout,
        Ok(_) | Err(_) => {
            return RunOutcome::Failed(format!(
                "nastran.exe timeout must be finite, positive, and representable, got {timeout_seconds}"
            ));
        }
    };
    let open_core = match open_core_words(
        solver.open_core_words.as_deref(),
        solver.max_open_core_words,
    ) {
        Ok(words) => words,
        Err(error) => return RunOutcome::Failed(error),
    };

    let work_dir = match absolute_path(work_dir) {
        Ok(path) => path,
        Err(error) => return RunOutcome::Failed(error),
    };
    if let Err(error) = validate_dosnam_paths(&work_dir) {
        return RunOutcome::Failed(error);
    }
    let rf = match rigid_format_stage_dir(&work_dir, solver.rf_stage.as_deref()) {
        Ok(path) => path,
        Err(error) => return RunOutcome::Failed(error),
    };
    if let Err(error) = validate_rigid_format_stage(&rf) {
        return RunOutcome::Failed(error);
    }
    if let Err(error) = stage(solver, &work_dir, &rf) {
        return RunOutcome::Failed(format!("could not stage the run: {error}"));
    }
    let scratch = work_dir.join("scr");

    let mut command = Command::new(&solver.exe);
    command
        .current_dir(&work_dir)
        .env("DBMEM", "0")
        .env("OCMEM", &open_core)
        .env("RFDIR", short(&rf))
        .env("DIRCTY", short(&scratch))
        .env("LOGNM", short(&work_dir.join("run.log")))
        .env("OPTPNM", short(&work_dir.join("run.optp")))
        .env("NPTPNM", short(&work_dir.join("run.nptp")))
        .env("PLTNM", "none")
        .env("DICTNM", short(&work_dir.join("run.dic")))
        .env("PUNCHNM", short(&work_dir.join("run.pch")));
    for unit in 11..=21 {
        command.env(format!("FTN{unit}"), "none");
    }
    for unit in 1..=10 {
        command.env(format!("SOF{unit}"), "none");
    }
    if let Some(runtime) = &solver.runtime {
        let path = std::env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("{};{path}", runtime.display()));
    }

    // A bare carriage return lands in column 81 and fails the scanner.
    let stdin_text = deck.replace('\r', "");
    supervise(command, &stdin_text, timeout)
}

/// Reject legacy `DOSNAM` paths before NASTRAN silently truncates a filename
/// and writes its log under an unrelated one-character name.
fn validate_dosnam_paths(work_dir: &Path) -> Result<(), String> {
    let names = [
        "scr", "run.log", "run.optp", "run.nptp", "run.dic", "run.pch",
    ];
    for name in names {
        let path = work_dir.join(name);
        let displayed = short(&path);
        let bytes = displayed.len();
        if bytes > MAX_DOSNAM_PATH_BYTES {
            return Err(format!(
                "NASTRAN-95 work path is {bytes} bytes, but DOSNAM permits at most \
                 {MAX_DOSNAM_PATH_BYTES}: {displayed}. Configure a short RF staging directory \
                 so ALAS can place the transient run beside it."
            ));
        }
    }
    Ok(())
}

/// Validate the allocation before staging a run. The Fortran executable exits
/// with code zero after printing its own cap message, which otherwise looks
/// like a missing-result failure to the caller.
fn open_core_words(configured: Option<&str>, maximum: u64) -> Result<String, String> {
    let owned_default;
    let raw = match configured {
        Some(value) => value,
        None => {
            owned_default = maximum.to_string();
            &owned_default
        }
    };
    let words = raw.parse::<u64>().map_err(|error| {
        format!("NASTRAN-95 OCMEM must be a positive integer number of words, got {raw:?}: {error}")
    })?;
    if words == 0 || words > maximum {
        return Err(format!(
            "NASTRAN-95 OCMEM must be between 1 and {maximum} words for this local build, got {words}. Rebuild nastran.exe with a larger COMMON /ZZZZZZ/ allocation to run a larger mesh."
        ));
    }
    Ok(words.to_string())
}

/// Read the limit emitted beside a current executable. A missing or malformed
/// marker means this is an older build whose 14M-word COMMON allocation is the
/// only trustworthy contract.
fn compiled_open_core_limit(executable: &Path) -> u64 {
    executable
        .parent()
        .and_then(|dir| std::fs::read_to_string(dir.join(OPEN_CORE_LIMIT_FILE)).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|words| *words > 0)
        .unwrap_or(LEGACY_MAX_OPEN_CORE_WORDS)
}

/// Pick the directory holding copied rigid formats for one run.
///
/// Keeping the default beneath `work_dir` makes a short standalone run wholly
/// self-contained. A caller that stores result artifacts deep under a user
/// output tree can instead set `ALAS_NASTRAN95_RF_STAGE` to an ALAS-owned short
/// absolute directory; only copied rigid formats and the patched `NASINFO` land
/// there.
fn rigid_format_stage_dir(work_dir: &Path, configured: Option<&Path>) -> Result<PathBuf, String> {
    match configured {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(path) => Err(format!(
            "NASTRAN-95 RF staging directory must be absolute, got {}",
            path.display()
        )),
        None => Ok(work_dir.join("rf")),
    }
}

/// Resolve before `Command::current_dir` changes how a relative environment path
/// would be interpreted by the Fortran executable.
fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| {
            format!(
                "cannot resolve NASTRAN-95 work directory {}: {error}",
                path.display()
            )
        })
}

/// Reject a rigid-format path that would truncate `AERO10` inside `RFOPEN`.
fn validate_rigid_format_stage(rf: &Path) -> Result<(), String> {
    let displayed = short(rf);
    let bytes = displayed.len();
    if bytes <= MAX_RIGID_FORMAT_DIR_BYTES {
        return Ok(());
    }
    Err(format!(
        "NASTRAN-95 rigid-format directory is {bytes} bytes, but RFOPEN permits at most \
         {MAX_RIGID_FORMAT_DIR_BYTES}; use a shorter work directory or set \
         ALAS_NASTRAN95_RF_STAGE to a dedicated short directory (got {displayed})"
    ))
}

/// Empty `work_dir`, create its scratch, and stage `rf/` with the timing
/// constants switched on.
fn stage(solver: &Nastran95Solver, work_dir: &Path, rf: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(work_dir);
    std::fs::create_dir_all(work_dir.join("scr"))?;
    std::fs::create_dir_all(rf)?;
    for entry in std::fs::read_dir(&solver.rf_source)? {
        let entry = entry?;
        let target = rf.join(entry.file_name());
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    // Set TIM = 16 in section two so the run uses the sixteen GINO timing
    // constants NASINFO already ships and skips the TMTSIO benchmark.
    let nasinfo = solver.rf_source.join("NASINFO");
    let text = std::fs::read_to_string(&nasinfo)?;
    let patched = text.replace("TIM =    -99 ", "TIM =     16 ");
    std::fs::write(rf.join("NASINFO"), patched)?;
    Ok(())
}

/// A path as the string the solver's `GETENV` reads, with the extended-length
/// prefix stripped -- the classic scanner does not accept it.
fn short(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

/// Spawn, feed the deck, hold to the timeout, and return stdout as the print
/// file.
fn supervise(mut command: Command, deck: &str, timeout: Duration) -> RunOutcome {
    let spawned = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group()
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return RunOutcome::Failed(format!("failed to launch nastran.exe: {error}")),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let owned = deck.to_owned();
        // Write on a thread: a deck larger than the pipe buffer would otherwise
        // deadlock against a solver already reading and echoing it.
        thread::spawn(move || {
            let _ = stdin.write_all(owned.as_bytes());
        });
    }
    let stdout_reader = child.stdout.take().map(drain);
    let stderr_reader = child.stderr.take().map(drain);

    let Some(deadline) = Instant::now().checked_add(timeout) else {
        return RunOutcome::Failed("nastran.exe timeout exceeds the clock range".to_owned());
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_process_tree(child.id());
                    let _ = child.wait();
                    return RunOutcome::Failed(format!(
                        "nastran.exe timed out after {:.0}s (tree force-killed)",
                        timeout.as_secs_f64()
                    ));
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                return RunOutcome::Failed(format!("failed while waiting on nastran.exe: {error}"));
            }
        }
    };

    let stdout = stdout_reader.map(join).unwrap_or_default();
    let stderr = stderr_reader.map(join).unwrap_or_default();
    if !status.success() {
        let code = status
            .code()
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
        return RunOutcome::Failed(format!(
            "nastran.exe exited with code {code} (stdout tail: {}; stderr tail: {})",
            tail(&stdout),
            tail(&stderr)
        ));
    }
    let fatals = fatal_lines(&stdout);
    if !fatals.is_empty() {
        return RunOutcome::Failed(format!(
            "print file reports {} fatal message(s):\n{}",
            fatals.len(),
            fatals.join("\n")
        ));
    }
    if read_displacement_tables(&stdout).is_empty() && read_eigenvalues(&stdout).is_empty() {
        return RunOutcome::Failed(format!(
            "nastran.exe wrote no result table (stderr tail: {})",
            tail(&stderr)
        ));
    }
    RunOutcome::Print(stdout)
}

/// Every fatal message the print file carries, the way [`crate::nastran::run`]
/// scans for them -- across form feeds, not only newlines.
fn fatal_lines(print: &str) -> Vec<String> {
    text::splitlines(print)
        .into_iter()
        .filter(|line| line.contains("USER FATAL") || line.contains("SYSTEM FATAL"))
        .map(|line| text::strip(line).to_owned())
        .collect()
}

fn tail(text_input: &str) -> String {
    let lines = text::splitlines(text::strip(text_input));
    lines[lines.len().saturating_sub(6)..].join(" | ")
}

/// The displacement vector for each subcase, keyed by grid.
///
/// A subcase's table is printed as a contiguous run of grid rows in ascending
/// grid order, paginated by form feeds that repeat the title; a new subcase
/// restarts the grid order. So the parser collects rows while inside a
/// displacement section and opens a fresh table whenever a grid identifier drops
/// below the last one seen -- which separates a paginated continuation from a
/// new subcase without needing the page's subcase banner, and which the many
/// intervening `SPCFORCE`/`STRESS` tables of a modern deck cannot confuse,
/// because those are not displacement sections.
pub fn read_displacement_tables(print: &str) -> Vec<Vec<(i64, [f64; 6])>> {
    let mut tables: Vec<Vec<(i64, [f64; 6])>> = Vec::new();
    let mut in_displacement = false;
    let mut last_grid = i64::MAX;
    for line in text::splitlines(print) {
        if line.contains("D I S P L A C E M E N T   V E C T O R") {
            in_displacement = true;
            continue;
        }
        // Any other tabular section header ends the displacement span.
        if is_section_header(line) {
            in_displacement = false;
            continue;
        }
        if !in_displacement {
            continue;
        }
        if let Some((grid, row)) = displacement_row(line) {
            if grid <= last_grid || tables.is_empty() {
                tables.push(Vec::new());
            }
            last_grid = grid;
            if let Some(table) = tables.last_mut() {
                table.push((grid, row));
            }
        }
    }
    tables
}

/// The real eigenvector table for one extracted SOL 103 mode, keyed by grid.
///
/// NASTRAN-95 prints modal shapes under `REAL EIGENVECTOR`, not under the
/// modern solver's `DISPLACEMENT VECTOR` heading.  The same mode heading is
/// repeated at each printed page, so its `NO.` field, rather than the heading
/// count, keys the table. The row layout is otherwise the same six
/// translations/rotations.
pub fn read_eigenvector_tables(print: &str) -> Vec<Vec<(i64, [f64; 6])>> {
    let mut tables: Vec<Vec<(i64, [f64; 6])>> = Vec::new();
    let mut in_eigenvector = false;
    let mut current_table = None;
    for line in text::splitlines(print) {
        if line.contains("R E A L   E I G E N V E C T O R") {
            current_table = line
                .split_whitespace()
                .last()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|&mode| mode > 0)
                .map(|mode| mode - 1);
            if let Some(index) = current_table {
                while tables.len() <= index {
                    tables.push(Vec::new());
                }
                in_eigenvector = true;
            } else {
                in_eigenvector = false;
            }
            continue;
        }
        if line.contains("R E A L   E I G E N V A L U E S") {
            in_eigenvector = false;
            continue;
        }
        if in_eigenvector {
            if let Some((grid, row)) = displacement_row(line) {
                if let Some(table) = current_table.and_then(|index| tables.get_mut(index)) {
                    table.push((grid, row));
                }
            }
        }
    }
    tables
}

/// The displacement of one grid in one subcase, or `None` if it is not reported.
pub fn displacement_of(
    tables: &[Vec<(i64, [f64; 6])>],
    subcase: usize,
    grid: i64,
) -> Option<[f64; 6]> {
    tables
        .get(subcase)?
        .iter()
        .find(|&&(id, _)| id == grid)
        .map(|&(_, row)| row)
}

/// The real eigenvalues, one per extracted mode, in the order the table lists
/// them (ascending eigenvalue).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    /// The eigenvalue (radians-per-second squared).
    pub eigenvalue: f64,
    /// The cyclic frequency, Hz -- the fifth column.
    pub cyclic_hz: f64,
}

/// Read the `R E A L   E I G E N V A L U E S` table.
pub fn read_eigenvalues(print: &str) -> Vec<Mode> {
    let mut modes = Vec::new();
    let mut in_table = false;
    let mut started = false;
    for line in text::splitlines(print) {
        if line.contains("R E A L   E I G E N V A L U E S") {
            in_table = true;
            started = false;
            continue;
        }
        if !in_table {
            continue;
        }
        if let Some(mode) = eigenvalue_row(line) {
            started = true;
            modes.push(mode);
        } else if started && !line.trim().is_empty() && !is_page_furniture(line) {
            in_table = false;
        }
    }
    modes
}

/// A line's tokens, if it is `<grid> G <six reals>`.
fn displacement_row(line: &str) -> Option<(i64, [f64; 6])> {
    let mut tokens = line.split_whitespace();
    let grid: i64 = tokens.next()?.parse().ok()?;
    if tokens.next()? != "G" {
        return None;
    }
    let mut row = [0.0; 6];
    for slot in &mut row {
        *slot = tokens.next()?.parse().ok()?;
    }
    Some((grid, row))
}

/// A line's tokens, if it is an eigenvalue row `<mode> <order> <eigenvalue>
/// <radian> <cyclic> ...`.
fn eigenvalue_row(line: &str) -> Option<Mode> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }
    let _mode_no: i64 = tokens[0].parse().ok()?;
    let _order: i64 = tokens[1].parse().ok()?;
    let eigenvalue: f64 = tokens[2].parse().ok()?;
    let cyclic_hz: f64 = tokens[4].parse().ok()?;
    Some(Mode {
        eigenvalue,
        cyclic_hz,
    })
}

/// A line naming a different tabular section, which ends a displacement span.
fn is_section_header(line: &str) -> bool {
    const SECTIONS: [&str; 6] = [
        "F O R C E S",
        "S T R E S S E S",
        "R E A L   E I G E N V A L U E S",
        "E I G E N V A L U E",
        "O L O A D",
        "S O R T E D",
    ];
    SECTIONS.iter().any(|section| line.contains(section))
}

/// A line that is part of a table's paginated furniture rather than a data row.
fn is_page_furniture(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('+')
        || trimmed.starts_with('*')
        || line.contains("MESSAGE")
        || line.contains("MODE")
        || line.contains("NO.")
        || line.contains("EIGENVALUE")
}

fn drain<R: Read + Send + 'static>(mut stream: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stream.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    })
}

fn join(handle: thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rigid_format_directory_that_would_truncate_a_member_is_rejected() {
        let path = Path::new("C:/a-deliberately-long-nastran95-rigid-format-stage");
        let error =
            validate_rigid_format_stage(path).expect_err("the legacy buffer is only 44 bytes");
        assert!(error.contains("RFOPEN permits at most"), "{error}");
        assert!(error.contains("ALAS_NASTRAN95_RF_STAGE"), "{error}");
    }

    #[test]
    fn a_relative_work_directory_is_resolved_before_it_reaches_the_child() {
        let work = absolute_path(Path::new("relative-nastran95-work"))
            .expect("the current directory is available to a test");
        assert!(work.is_absolute(), "{}", work.display());
        assert!(
            work.ends_with("relative-nastran95-work"),
            "{}",
            work.display()
        );
    }

    #[test]
    fn a_configured_rf_stage_moves_transient_work_out_of_a_long_artifact_tree() {
        let solver = Nastran95Solver {
            exe: PathBuf::from("unused-nastran95.exe"),
            rf_source: PathBuf::from("unused-rf"),
            runtime: None,
            rf_stage: Some(PathBuf::from("C:/nas-rf")),
            open_core_words: None,
            max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
        };
        let work = solver
            .workspace_for(Path::new("C:/a/deep/artifact/tree/nastran95/sol101"))
            .expect("a rooted RF stage has a parent");
        assert!(
            work.ancestors()
                .any(|path| path.file_name().is_some_and(|name| name == "nas-run")),
            "{}",
            work.display()
        );
        assert!(work.ends_with("sol101"), "{}", work.display());
        assert!(!work.starts_with("C:/a/deep"), "{}", work.display());
    }

    #[test]
    fn an_overlong_dosnam_path_is_rejected_before_staging() {
        let work =
            Path::new("C:/this-is-an-intentionally-long-nastran95-working-directory-name/sol101");
        let error = validate_dosnam_paths(work).expect_err("DOSNAM has a 72-byte limit");
        assert!(error.contains("DOSNAM permits at most"), "{error}");
    }

    #[test]
    fn an_invalid_timeout_is_reported_before_the_runner_touches_the_filesystem() {
        let solver = Nastran95Solver {
            exe: PathBuf::from("unused-nastran95.exe"),
            rf_source: PathBuf::from("unused-rf"),
            runtime: None,
            rf_stage: None,
            open_core_words: None,
            max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
        };
        for timeout in [f64::NAN, f64::MAX] {
            let outcome = run_nastran95(&solver, "", Path::new("not-created"), timeout);
            let RunOutcome::Failed(error) = outcome else {
                panic!("an invalid timeout must not launch a solve");
            };
            assert!(error.contains("representable"), "{error}");
        }
    }

    #[test]
    fn an_open_core_allocation_beyond_the_local_build_cap_is_rejected_before_staging() {
        let solver = Nastran95Solver {
            exe: PathBuf::from("unused-nastran95.exe"),
            rf_source: PathBuf::from("unused-rf"),
            runtime: None,
            rf_stage: None,
            open_core_words: Some("32000000".to_owned()),
            max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
        };
        let outcome = run_nastran95(&solver, "", Path::new("not-created"), 30.0);
        let RunOutcome::Failed(error) = outcome else {
            panic!("an invalid open-core allocation must not launch a solve");
        };
        assert!(error.contains("14000000"), "{error}");
        assert!(!Path::new("not-created").exists());
    }

    #[test]
    fn an_unset_open_core_uses_the_entire_compiled_allocation() {
        let words = open_core_words(None, 64_000_000).expect("a positive compiled limit");
        assert_eq!(words, "64000000");
    }

    #[test]
    fn a_nonzero_exit_is_not_accepted_even_if_stdout_looks_like_a_result_table() {
        let outcome = supervise(
            nonzero_command_with_a_displacement_table(),
            "ignored",
            Duration::from_secs(5),
        );
        let RunOutcome::Failed(error) = outcome else {
            panic!("a non-zero solver exit must never be a usable solve");
        };
        assert!(error.contains("exited with code 7"), "{error}");
    }

    #[test]
    fn a_displacement_table_is_read_by_grid() {
        let print = "\
                                             D I S P L A C E M E N T   V E C T O R
      POINT ID.   TYPE          T1             T2             T3
             1      G      0.0            0.0            0.0            0.0            0.0            0.0
             2      G      1.0E-3         0.0            5.0E-2         0.0           -1.0E-1         0.0
";
        let tables = read_displacement_tables(print);
        assert_eq!(tables.len(), 1);
        let row = displacement_of(&tables, 0, 2).unwrap();
        assert!((row[2] - 5.0e-2).abs() < 1e-12);
        assert!(displacement_of(&tables, 0, 9).is_none());
    }

    #[test]
    fn eigenvector_page_headers_append_to_the_mode_they_name() {
        let print = "\
 R E A L   E I G E N V E C T O R   N O .          1
             1      G      0.0  0.0  1.0  0.0  0.0  0.0
 R E A L   E I G E N V E C T O R   N O .          1
             2      G      0.0  0.0  1.5  0.0  0.0  0.0
 R E A L   E I G E N V E C T O R   N O .          2
             1      G      0.0  0.0  2.0  0.0  0.0  0.0
";
        let tables = read_eigenvector_tables(print);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0][0].1[2], 1.0);
        assert_eq!(tables[0][1].1[2], 1.5);
        assert_eq!(tables[1][0].1[2], 2.0);
    }

    #[test]
    fn pagination_keeps_one_subcase_together_and_a_grid_reset_starts_the_next() {
        // Two pages of subcase 1 (grids ascending across a repeated title), then
        // an SPCFORCE section, then subcase 2 restarting from grid 1.
        let print = "\
 D I S P L A C E M E N T   V E C T O R
             1      G      0.0  0.0  1.0  0.0  0.0  0.0
             2      G      0.0  0.0  2.0  0.0  0.0  0.0
 D I S P L A C E M E N T   V E C T O R
             3      G      0.0  0.0  3.0  0.0  0.0  0.0
 F O R C E S   O F   S I N G L E   P O I N T   C O N S T R A I N T
             1      G      9.0  0.0  0.0  0.0  0.0  0.0
 D I S P L A C E M E N T   V E C T O R
             1      G      0.0  0.0  7.0  0.0  0.0  0.0
             3      G      0.0  0.0  9.0  0.0  0.0  0.0
";
        let tables = read_displacement_tables(print);
        assert_eq!(tables.len(), 2, "{tables:?}");
        assert_eq!(tables[0].len(), 3);
        assert!((displacement_of(&tables, 0, 3).unwrap()[2] - 3.0).abs() < 1e-12);
        // The SPCFORCE row for grid 1 must not have polluted the displacements.
        assert!((displacement_of(&tables, 1, 1).unwrap()[2] - 7.0).abs() < 1e-12);
        assert!((displacement_of(&tables, 1, 3).unwrap()[2] - 9.0).abs() < 1e-12);
    }

    #[test]
    fn the_eigenvalue_table_yields_cyclic_frequencies_in_order() {
        let print = "\
                                              R E A L   E I G E N V A L U E S
   MODE    EXTRACTION       EIGENVALUE            RADIAN              CYCLIC
    NO.       ORDER
        1         2        3.237408E+01        5.689823E+00        9.055634E-01        1.0E+04        3.3E+05
        2         1        2.022407E+02        1.422113E+01        2.263364E+00        1.0E+04        2.0E+06
 SORTED BULK
";
        let modes = read_eigenvalues(print);
        assert_eq!(modes.len(), 2);
        assert!((modes[0].cyclic_hz - 9.055634e-1).abs() < 1e-9);
        assert!((modes[1].cyclic_hz - 2.263364).abs() < 1e-9);
        assert!((modes[0].eigenvalue - 3.237408e1).abs() < 1e-6);
    }

    #[test]
    fn a_print_file_with_no_tables_reads_as_empty() {
        assert!(read_displacement_tables("nothing here").is_empty());
        assert!(read_eigenvalues("nothing here").is_empty());
    }

    #[test]
    fn a_fatal_message_across_a_form_feed_is_found() {
        let print = "some output\u{c}   *** USER FATAL MESSAGE 9994 (IFP)   ";
        assert_eq!(fatal_lines(print), ["*** USER FATAL MESSAGE 9994 (IFP)"]);
    }

    #[cfg(windows)]
    fn nonzero_command_with_a_displacement_table() -> Command {
        let mut command = Command::new("cmd.exe");
        command.args([
            "/d",
            "/c",
            "echo D I S P L A C E M E N T   V E C T O R & echo 1 G 0 0 0 0 0 0 & exit /b 7",
        ]);
        command
    }

    #[cfg(not(windows))]
    fn nonzero_command_with_a_displacement_table() -> Command {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf 'D I S P L A C E M E N T   V E C T O R\\n1 G 0 0 0 0 0 0\\n'; exit 7",
        ]);
        command
    }
}
