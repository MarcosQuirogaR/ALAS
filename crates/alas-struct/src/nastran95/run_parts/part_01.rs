// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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

/// Check the imported GNU runtime libraries before the child process starts.
///
/// The dependency list is read from the executable rather than assumed for
/// every NASTRAN-95 build.  This keeps statically linked or differently built
/// solvers usable, while a binary that actually imports the GNU DLLs receives
/// a deterministic diagnostic instead of a Windows loader dialog.  Search
/// order mirrors the Windows loader's useful caller-controlled locations:
/// configured runtime, executable directory, then the inherited `PATH`.
fn validate_runtime_dependencies(
    executable: &Path,
    configured: Option<&Path>,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (executable, configured);
        return Ok(());
    }

    #[cfg(windows)]
    {
        let bytes = std::fs::read(executable).map_err(|error| {
            format!(
                "cannot inspect NASTRAN-95 executable {} for runtime dependencies: {error}",
                executable.display()
            )
        })?;
        let imported = WINDOWS_GNU_RUNTIME_DLLS
            .iter()
            .copied()
            .filter(|name| contains_ascii_case_insensitive(&bytes, name.as_bytes()))
            .collect::<Vec<_>>();
        if imported.is_empty() {
            return Ok(());
        }

        let mut search_dirs = Vec::new();
        if let Some(runtime) = configured {
            search_dirs.push(absolute_path(runtime).unwrap_or_else(|_| runtime.to_path_buf()));
        }
        if let Some(parent) = executable.parent() {
            search_dirs.push(parent.to_path_buf());
        }
        if let Some(path) = std::env::var_os("PATH") {
            search_dirs.extend(std::env::split_paths(&path));
        }
        let mut unique_search_dirs = Vec::new();
        for directory in search_dirs {
            if !directory.as_os_str().is_empty() && !unique_search_dirs.contains(&directory) {
                unique_search_dirs.push(directory);
            }
        }
        let search_dirs = unique_search_dirs;
        let missing = imported
            .iter()
            .filter(|name| {
                !search_dirs
                    .iter()
                    .any(|directory| directory.join(name).is_file())
            })
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        let searched = search_dirs
            .iter()
            .map(|directory| directory.display().to_string())
            .collect::<Vec<_>>();
        Err(format!(
            "NASTRAN-95 executable {} imports missing Windows runtime DLL(s): {}. Searched: {}. Configure ALAS_NASTRAN95_RUNTIME or install the matching GNU Fortran runtime beside the solver; no solver process was spawned.",
            executable.display(),
            missing.join(", "),
            if searched.is_empty() {
                "(none)".to_owned()
            } else {
                searched.join("; ")
            }
        ))
    }
}

/// Resolve a runtime without silently replacing an existing user directory.
/// A stale configured path is recoverable only from the selected solver's
/// adjacent `runtime` directory; no host-wide search is attempted.
fn resolve_runtime(
    solver_root: &Path,
    configured: Option<&Path>,
) -> (Option<PathBuf>, Nastran95RuntimeSource) {
    if let Some(path) = configured {
        if path.is_dir() {
            return (Some(path.to_path_buf()), Nastran95RuntimeSource::Configured);
        }
    }

    let adjacent = solver_root.join("runtime");
    if adjacent.is_dir() {
        return (
            Some(adjacent),
            Nastran95RuntimeSource::Adjacent {
                root: solver_root.to_path_buf(),
            },
        );
    }

    (
        configured.map(Path::to_path_buf),
        if configured.is_some() {
            Nastran95RuntimeSource::Configured
        } else {
            Nastran95RuntimeSource::Unspecified
        },
    )
}

#[cfg(windows)]
fn contains_ascii_case_insensitive(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
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
