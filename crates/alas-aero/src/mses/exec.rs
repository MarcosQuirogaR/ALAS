// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the subprocess.run(...) calls in
// native aerodynamic model/aerodynamics/aero_2D/mses.py and alas/physics/mses_analysis.py.

//! Running one external tool: feed it keystrokes, capture its output, hold it
//! to a timeout, and never let it flash a console window.
//!
//! This is the call-site half of the split `alas-exec` documents: that crate
//! owns the windowless flag, and capturing output, feeding stdin and enforcing
//! a timeout stay here, as they did at upstream's `subprocess.run(...,
//! input=..., capture_output=True, timeout=...)`. The standard library has no
//! wait-with-timeout, so this drains stdout and stderr on their own threads
//! (a single tool can outrun one pipe's buffer while the other blocks) and
//! polls the child for completion until the deadline, killing it if it passes.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_exec::SupervisedSpawn;
use thiserror::Error;

/// How often the run loop checks whether the child has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(15);

/// Distinguishes work directories created within one process.
static WORKDIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A private working directory that deletes itself when dropped.
///
/// The standard counterpart of upstream's `tempfile.TemporaryDirectory`: MSES
/// writes its mesh, deck and dump files into a directory it treats as scratch,
/// and it must be gone afterwards. There is no `tempfile` crate here (a
/// dependency this workspace declines to add for one directory), so the name is
/// made unique from the process id, a monotonic counter and the wall clock, and
/// [`Drop`] removes the tree: best-effort, since a caller cannot act on a
/// failed cleanup of a directory that is about to be forgotten anyway.
pub struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    /// Create a fresh, uniquely named directory under the system temp dir.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] if the directory cannot be created.
    pub fn new(prefix: &str) -> Result<Self, std::io::Error> {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let count = WORKDIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}{}_{nanos}_{count}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// The directory's path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A finished tool run: its exit status and captured stdout.
///
/// stderr is drained (so a chatty tool cannot deadlock on a full pipe) but not
/// returned: nothing in the paths this drives reads it. Upstream inspects it
/// only in `mset`'s X11-launch-failure branch, which is Linux-only and has no
/// counterpart where these binaries run windowless on Windows.
#[derive(Debug)]
pub struct ToolRun {
    /// The process exit status.
    pub status: ExitStatus,
    /// Everything the tool wrote to stdout.
    pub stdout: String,
}

/// Why a tool run could not be completed and read back.
#[derive(Debug, Error)]
pub enum RunError {
    /// The executable could not be spawned (missing, not executable, ...).
    #[error("could not start {command}: {source}")]
    Spawn {
        /// The command that failed to start.
        command: String,
        /// The underlying spawn error.
        source: std::io::Error,
    },
    /// An I/O error while feeding stdin or waiting on the child.
    #[error("I/O error running {command}: {source}")]
    Io {
        /// The command being run.
        command: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The child outlived its timeout and was killed.
    #[error("{command} did not finish within {seconds} s and was terminated")]
    Timeout {
        /// The command that timed out.
        command: String,
        /// The timeout it exceeded, in seconds.
        seconds: f64,
    },
    /// A stdout/stderr reader thread panicked, should not happen.
    #[error("output reader for {command} failed")]
    Reader {
        /// The command whose output could not be read.
        command: String,
    },
    /// The caller requested cooperative cancellation while the child was
    /// running; the process tree was terminated before returning.
    #[error("{command} was cancelled and terminated")]
    Cancelled {
        /// The command that was cancelled.
        command: String,
    },
    /// The caller supplied a non-finite, non-positive, or unrepresentable
    /// process timeout.
    #[error("invalid process timeout {seconds:?} s; expected a finite positive value")]
    InvalidTimeout {
        /// The invalid timeout in seconds.
        seconds: f64,
    },
}

/// [`run_tool_with_cancel`] without a cancellation flag: the form the unit
/// tests drive the supervisor through. Product callers always carry the flag,
/// so this is compiled with the tests only.
#[cfg(test)]
pub fn run_tool(
    exe: &Path,
    args: &[&str],
    cwd: &Path,
    stdin_input: &str,
    timeout_seconds: f64,
) -> Result<ToolRun, RunError> {
    run_tool_with_cancel(exe, args, cwd, stdin_input, timeout_seconds, None)
}

/// Run `exe` with `args` in `cwd`, feeding `stdin_input` and capturing output,
/// killed if it runs past `timeout_seconds`, while observing an optional
/// cooperative cancellation flag.
///
/// Returns the run's exit status and streams; the caller decides whether a
/// non-zero status is a failure (upstream's `check=True` call sites all treat
/// it as one). Cancellation kills
/// the complete process tree and drains both pipes before returning, so a
/// legacy launcher cannot keep MSES/MPlot descendants or file handles alive.
pub fn run_tool_with_cancel(
    exe: &Path,
    args: &[&str],
    cwd: &Path,
    stdin_input: &str,
    timeout_seconds: f64,
    cancel: Option<&AtomicBool>,
) -> Result<ToolRun, RunError> {
    run_tool_with_env_and_cancel(exe, args, cwd, stdin_input, timeout_seconds, cancel, &[])
}

/// Run an external tool with a small, explicit set of process-local
/// environment overrides.
///
/// This is used for solver resources such as MSES's `MSES_OSMAP` hook. The
/// The ledger role for one MSES-suite launch: `MSES mset`, `MSES mses`, `MSES mplot`.
fn mses_role(exe: &Path) -> String {
    let tool = exe
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    format!("MSES {tool}")
}

/// overrides are applied only to the child process; ALAS never changes the
/// host environment, so one run cannot leak a map path into another project or
/// into the user's shell.
pub fn run_tool_with_env_and_cancel(
    exe: &Path,
    args: &[&str],
    cwd: &Path,
    stdin_input: &str,
    timeout_seconds: f64,
    cancel: Option<&AtomicBool>,
    environment: &[(&str, &OsStr)],
) -> Result<ToolRun, RunError> {
    let timeout =
        Duration::try_from_secs_f64(timeout_seconds).map_err(|_| RunError::InvalidTimeout {
            seconds: timeout_seconds,
        })?;
    if timeout_seconds <= 0.0 {
        return Err(RunError::InvalidTimeout {
            seconds: timeout_seconds,
        });
    }
    // `ALAS_TOOL_TIMEOUT_SCALE` (default 1, exact) stretches every external-tool deadline.
    let scaled =
        alas_exec::tool_runtime::RuntimeSettings::current().scaled_timeout_s(timeout_seconds);
    let timeout = Duration::try_from_secs_f64(scaled).unwrap_or(timeout);
    let command = exe.display().to_string();
    // Opt-in result cache (`ALAS_TOOL_CACHE_DIR`). MSES is deterministic and
    // reads only the files in `cwd`, its keystrokes and `MSES_OSMAP`, so the
    // key is exactly those plus the executable. Only exit-0 runs are stored.
    let cache = mses_cache(exe, args, cwd, stdin_input, environment);
    if let Some(stdout) = cache
        .as_ref()
        .and_then(|cache| cache.restore())
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        return Ok(ToolRun {
            status: success_status(),
            stdout,
        });
    }
    let mut command_builder = Command::new(exe);
    let mut child = command_builder
        .args(args)
        .current_dir(cwd)
        .envs(environment.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group()
        .spawn_supervised(&mses_role(exe))
        .map_err(|source| RunError::Spawn {
            command: command.clone(),
            source,
        })?;

    // Feed all keystrokes on a thread, then close stdin so the tool stops
    // waiting on it. Writing inline would block forever on a tool that stops
    // reading with a full pipe, outside the deadline below; the thread lets
    // the deadline kill the tree, which breaks the pipe and ends the writer.
    let stdin_pipe = child.stdin.take().ok_or_else(|| RunError::Io {
        command: command.clone(),
        source: std::io::Error::other("stdin pipe was not captured"),
    })?;
    let stdin_bytes = stdin_input.as_bytes().to_vec();
    let stdin_writer = thread::spawn(move || {
        let mut stdin_pipe = stdin_pipe;
        // A tool that exits before reading its keystrokes closes the pipe
        // first; on Linux the write then fails with EPIPE, a race Windows
        // rarely shows. Its exit status and transcript decide the run, so a
        // broken pipe here is not itself a failure.
        stdin_pipe
            .write_all(&stdin_bytes)
            .or_else(|error| match error.kind() {
                std::io::ErrorKind::BrokenPipe => Ok(()),
                _ => Err(error),
            })
    });

    let mut stdout_pipe = child.stdout.take().ok_or_else(|| RunError::Io {
        command: command.clone(),
        source: std::io::Error::other("stdout pipe was not captured"),
    })?;
    let mut stderr_pipe = child.stderr.take().ok_or_else(|| RunError::Io {
        command: command.clone(),
        source: std::io::Error::other("stderr pipe was not captured"),
    })?;
    // Raw bytes decoded lossily: a non-UTF-8 byte must not discard the whole
    // transcript (read_to_string would drop everything read so far).
    let stdout_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(|source| RunError::Io {
            command: command.clone(),
            source,
        })? {
            Some(status) => break status,
            None => {
                if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    kill_process_tree(child.id());
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(RunError::Cancelled { command });
                }
                if Instant::now() >= deadline {
                    // MSES is a launcher-style legacy executable on some
                    // installations.  Killing only the immediate process
                    // leaves a descendant holding the pipes or the working
                    // directory, which makes the next point/sweep flaky.
                    kill_process_tree(child.id());
                    let _ = child.wait();
                    return Err(RunError::Timeout {
                        command,
                        seconds: timeout_seconds,
                    });
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    };

    stdin_writer
        .join()
        .map_err(|_| RunError::Reader {
            command: command.clone(),
        })?
        .map_err(|source| RunError::Io {
            command: command.clone(),
            source,
        })?;
    let raw_stdout = stdout_reader.join().map_err(|_| RunError::Reader {
        command: command.clone(),
    })?;
    // Joined so the pipe is fully drained before returning; its contents are
    // discarded (see `ToolRun`).
    let _ = stderr_reader.join().map_err(|_| RunError::Reader {
        command: command.clone(),
    })?;

    let stdout = normalize_newlines(raw_stdout);
    if status.success() {
        if let Some(cache) = &cache {
            cache.store(stdout.as_bytes());
        }
    }
    Ok(ToolRun { status, stdout })
}

/// Cache handle for one MSES-suite launch, `None` when the cache is off.
fn mses_cache(
    exe: &Path,
    args: &[&str],
    cwd: &Path,
    stdin_input: &str,
    environment: &[(&str, &OsStr)],
) -> Option<alas_exec::tool_cache::DirCache> {
    let mut environment_text = Vec::new();
    for (name, value) in environment {
        environment_text.extend_from_slice(name.as_bytes());
        environment_text.push(b'=');
        // An osmap hook names a file whose content, not path, is the input.
        let path = Path::new(value);
        match std::fs::read(path) {
            Ok(content) if path.is_file() => environment_text.extend_from_slice(&content),
            _ => environment_text.extend_from_slice(value.to_string_lossy().as_bytes()),
        }
        environment_text.push(0);
    }
    let arguments = args.join("\u{1f}");
    let program = exe
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    alas_exec::tool_cache::DirCache::begin(
        "mses",
        exe,
        cwd,
        &[],
        &[
            program.as_bytes(),
            arguments.as_bytes(),
            stdin_input.as_bytes(),
            &environment_text,
        ],
    )
}

/// A successful exit status, for replaying a cached run.
fn success_status() -> ExitStatus {
    #[cfg(windows)]
    {
        std::os::windows::process::ExitStatusExt::from_raw(0)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::process::ExitStatusExt::from_raw(0)
    }
}

/// Translate `\r\n` and lone `\r` to `\n`, reproducing what upstream's
/// `subprocess.run(..., text=True)` does with universal newlines.
///
/// The `key = value` summary scanner keys off `\n` and spaces, so a trailing
/// `\r` on the last value of a Windows line would otherwise make it fail to
/// parse (and read back as NaN), exactly the bytes Python never sees, because
/// its text-mode capture has already normalized them.
fn normalize_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

// These tests drive a real subprocess they build here, so a failed unwrap is
// the spawn/timeout behaviour failing in the test environment, not a library
// invariant being broken.
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn shell() -> (&'static str, Vec<&'static str>) {
        ("cmd", vec!["/C"])
    }

    #[cfg(not(windows))]
    fn shell() -> (&'static str, Vec<&'static str>) {
        ("sh", vec!["-c"])
    }

    #[test]
    fn captures_stdout_of_a_short_command() {
        let (program, mut args) = shell();
        args.push("echo hello");
        let cwd = std::env::temp_dir();
        let run = run_tool(Path::new(program), &args, &cwd, "", 10.0).unwrap();
        assert!(run.status.success());
        assert!(run.stdout.contains("hello"));
    }

    #[test]
    fn a_slow_command_is_killed_at_the_timeout() {
        // A command that sleeps far longer than the timeout must return the
        // Timeout error rather than blocking the test.
        #[cfg(windows)]
        let script = "ping -n 6 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 5";
        let (program, mut args) = shell();
        args.push(script);
        let cwd = std::env::temp_dir();
        let error = run_tool(Path::new(program), &args, &cwd, "", 0.3).unwrap_err();
        assert!(matches!(error, RunError::Timeout { .. }));
    }

    /// With the opt-in cache on, an identical launch replays the first run's
    /// stdout and output files instead of running the tool again. The command
    /// prints a clock reading and writes it to a file, so a re-run would differ.
    #[test]
    fn an_identical_launch_is_replayed_from_the_opt_in_cache() {
        use alas_exec::tool_runtime::RuntimeSettings;
        let scratch = WorkDir::new("alas_mses_cache_test_").expect("scratch dir");
        let cache_root = scratch.path().join("cache");
        let work = scratch.path().join("work");
        std::fs::create_dir_all(&work).expect("work dir");
        RuntimeSettings::set_override(Some(RuntimeSettings {
            cache_dir: Some(cache_root),
            ..RuntimeSettings::default()
        }));
        #[cfg(windows)]
        let script = "echo %RANDOM%%RANDOM%%TIME% > stamp.txt & type stamp.txt";
        #[cfg(not(windows))]
        let script = "date +%s%N > stamp.txt; cat stamp.txt";
        let (program, mut args) = shell();
        args.push(script);
        let first = run_tool(Path::new(program), &args, &work, "", 20.0).expect("first run");
        let first_stamp = std::fs::read_to_string(work.join("stamp.txt")).expect("stamp");
        std::fs::remove_file(work.join("stamp.txt")).expect("remove stamp");
        // The first run left its output in `work`; clear it so the replayed
        // directory looks like the one the first run started from.
        let second = run_tool(Path::new(program), &args, &work, "", 20.0).expect("second run");
        let second_stamp = std::fs::read_to_string(work.join("stamp.txt")).expect("replayed");
        RuntimeSettings::set_override(None);
        assert!(first.status.success() && second.status.success());
        assert_eq!(first.stdout, second.stdout);
        assert_eq!(first_stamp, second_stamp);
    }

    #[test]
    fn invalid_timeouts_are_rejected_before_process_launch() {
        let cwd = std::env::temp_dir();
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let error = run_tool(
                Path::new("this-program-must-not-launch"),
                &[],
                &cwd,
                "",
                seconds,
            )
            .expect_err("invalid timeout must be rejected");
            assert!(matches!(error, RunError::InvalidTimeout { .. }));
        }
    }
}
