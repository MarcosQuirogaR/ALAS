// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Driving the built NASTRAN-95 solver, and reading its print file back.
//!
//! The run contract is nothing like the modern one [`crate::nastran::run`]
//! drives. There is no deck argument and no result file keyword: the solver
//! reads the deck on **stdin**, writes the print file on **stdout**, and is
//! configured entirely through environment variables: `DBMEM`, `OCMEM`,
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
//! The supervised live runner defaults `DBMEM=1` to keep the database in the
//! remaining open core and reduce scratch I/O; `ALAS_NASTRAN95_DBMEM=0` is the
//! explicit opt-out for constrained hosts.
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

mod print_file;
mod process;
mod runtime;
#[cfg(test)]
mod tests;
mod workspace;

pub use print_file::{
    displacement_of, read_displacement_tables, read_eigenvalues, read_eigenvector_tables, Mode,
};
use process::*;
use runtime::*;
use workspace::*;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use alas_exec::process::{
    drain, drained_text, timeout_from_seconds, wait_with_timeout, DeadlineWait, NewProcessGroup,
    NoConsoleWindow,
};

use crate::nastran::text::{self};

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

/// GNU runtime libraries imported by the Windows NASTRAN-95 build.  They are
/// intentionally checked before spawning: Windows otherwise reports the
/// loader failure in a separate modal dialog that gives the desktop caller no
/// typed result to inspect.
#[cfg(windows)]
const WINDOWS_GNU_RUNTIME_DLLS: [&str; 4] = [
    "libgcc_s_seh-1.dll",
    "libgfortran-5.dll",
    "libquadmath-0.dll",
    "libwinpthread-1.dll",
];

/// Where the built solver and its runtime are, resolved from the environment.
///
/// `ALAS_NASTRAN95_DIR` is the checked-out solver tree (the executable is
/// `build/bin/nastran.exe` beneath it and the run-time files are its `rf/`)
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
    /// How the runtime directory was selected.
    pub runtime_source: Nastran95RuntimeSource,
    /// Optional short absolute directory holding the staged rigid-format files.
    pub rf_stage: Option<PathBuf>,
    /// Explicit open-core allocation for this solver, if a caller configured it.
    pub open_core_words: Option<String>,
    /// Largest run-time OCMEM allocation compiled into this executable.
    pub max_open_core_words: u64,
}

/// Provenance for the runtime directory used by a NASTRAN-95 solver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nastran95RuntimeSource {
    /// The caller's configured runtime path is an existing directory.
    Configured,
    /// The configured path was unavailable and the solver-local runtime was
    /// selected instead.
    Adjacent {
        /// Solver installation root containing the `runtime` directory.
        root: PathBuf,
    },
    /// No runtime directory was configured or found. A statically linked or
    /// otherwise differently built solver may still be valid in this state.
    Unspecified,
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
        let (runtime, runtime_source) = resolve_runtime(dir, runtime);
        let max_open_core_words = compiled_open_core_limit(&exe);
        Some(Self {
            exe,
            rf_source,
            runtime,
            runtime_source,
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

    /// Discover the compliance-complete NASTRAN-95 bundle beside the ALAS
    /// executable. Explicit configuration and environment variables remain
    /// higher-priority choices at the call site.
    pub fn from_adjacent_bundle() -> Option<Self> {
        let app_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let solver_dir = app_dir.join("external tools").join("NASTRAN-95");
        Self::from_paths(&solver_dir, None, None, None)
    }

    /// Explain when a stale configured runtime was replaced by the runtime
    /// shipped beside the selected solver. The integration layer can include
    /// this in its status card or run log without rewriting preferences.
    pub fn runtime_warning(&self) -> Option<String> {
        let Nastran95RuntimeSource::Adjacent { root } = &self.runtime_source else {
            return None;
        };
        let directory = self
            .runtime
            .as_deref()
            .map_or_else(|| root.join("runtime"), Path::to_path_buf);
        Some(format!(
            "Configured NASTRAN-95 runtime was unavailable; using adjacent runtime {} (solver root {})",
            directory.display(),
            root.display()
        ))
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
    let timeout = match timeout_from_seconds("nastran.exe", timeout_seconds) {
        Ok(timeout) => timeout,
        Err(error) => return RunOutcome::Failed(error.to_string()),
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
    if let Err(error) = validate_runtime_dependencies(&solver.exe, solver.runtime.as_deref()) {
        return RunOutcome::Failed(error);
    }
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
    // The live default uses the legacy solver's in-memory database.  NASTRAN-
    // 95 reuses the remaining compiled COMMON /ZZZZZZ/ after OCMEM; DBMEM=1
    // removes a large amount of scratch I/O when OCMEM is below the executable
    // maximum. Set ALAS_NASTRAN95_DBMEM=0 to opt out for constrained hosts.
    let dbmem = std::env::var("ALAS_NASTRAN95_DBMEM").unwrap_or_else(|_| "1".to_owned());

    let mut command = Command::new(&solver.exe);
    command
        .current_dir(&work_dir)
        .env("DBMEM", dbmem)
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
        let runtime = absolute_path(runtime).unwrap_or_else(|_| runtime.clone());
        command.env("PATH", format!("{};{path}", runtime.display()));
    }

    // A bare carriage return lands in column 81 and fails the scanner.
    let stdin_text = deck.replace('\r', "");
    supervise(command, &stdin_text, timeout)
}
