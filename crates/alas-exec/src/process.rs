// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/proc.py
// Reference: alas @ rust-port-baseline.

//! Windowless subprocess configuration.
//!
//! [`NoConsoleWindow::no_window`] configures a [`std::process::Command`] to run
//! without allocating a console window on Windows, and is a no-op on every
//! other platform, where the flag does not exist. It is the faithful
//! counterpart of upstream's `no_window_kwargs`, which spreads
//! `CREATE_NO_WINDOW` into a single `subprocess` call.
//!
//! Upstream also carries `install_no_console_default`, which monkeypatches
//! `subprocess.Popen.__init__` process-wide so that spawns it does *not* own:
//! the ones issued from inside native aerodynamic model's own MSES wrapper, also run
//! windowless. That has no counterpart here and needs none: this port drives
//! the external binaries itself, through this crate, rather than through a
//! third-party library that spawns its own processes. There is no spawn outside
//! this crate's reach to patch, so reproducing the global patch would mean
//! reproducing a workaround for a problem the translation removes. The guard
//! logic that patch carried: respect an explicit `startupinfo`, never combine
//! the flag with `CREATE_NEW_CONSOLE`/`DETACHED_PROCESS`: guarded exactly the
//! third-party spawns that are gone with it.
//!
//! Whether to capture output, feed stdin or bound the run stays at the call
//! sites, as it did upstream: those are per-call decisions. How a bounded run
//! is supervised is not, so every external-tool runner shares one policy and
//! one implementation of it: [`timeout_from_seconds`] validates the configured
//! timeout before launch, [`wait_with_timeout`] enforces it and tree-kills on
//! expiry, and [`drain`] keeps piped output from stalling the child meanwhile.
//!
//! [`kill_process_tree`] is the other half this crate owns, and it is here for
//! the same reason the windowless flag is: it is a property of *how a process
//! is spawned*, not of what any one tool does with its output. Killing a child
//! is not enough for the tools this workspace drives: `nastran.exe` is a
//! launcher that forks the real solver, and upstream records having watched the
//! orphan keep burning CPU and holding a licence seat after the parent was
//! killed. A tree kill needs the spawn to have been set up for it on Unix,
//! which is why [`NewProcessGroup::new_process_group`] and
//! [`kill_process_tree`] are one pair of things rather than two.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often [`wait_with_timeout`] polls a supervised child: short against
/// every external tool's run time, long enough not to spin a core while the
/// tool works.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// A configured timeout that cannot be used as a deadline.
///
/// Raised before the tool is launched, so a mistyped or corrupted setting is
/// reported as a configuration error naming the tool rather than turning into
/// an instant kill, a plausible-looking timeout or an unbounded run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{tool} timeout must be a finite number of seconds greater than zero, got {seconds}")]
pub struct InvalidTimeout {
    /// The external tool whose timeout was refused, as a user knows it.
    pub tool: String,
    /// The refused value, in seconds.
    pub seconds: f64,
}

/// Convert a configured timeout in seconds into the [`Duration`] every
/// external-tool runner supervises its child with.
///
/// This is the single timeout policy for external processes:
///
/// * a finite value greater than zero is the deadline, measured from the
///   moment the child is spawned;
/// * zero, a negative value, NaN and either infinity are refused with
///   [`InvalidTimeout`] before anything is launched. No configuration field,
///   settings help text or tool card gives any of them a "no limit" meaning,
///   so there is no spelling of "run forever";
/// * a finite value beyond what a `Duration` holds saturates at
///   [`Duration::MAX`] rather than panicking in `Duration::from_secs_f64`,
///   and [`wait_with_timeout`] treats a deadline past the platform clock's
///   range as one that never arrives. Such a value is already longer than
///   any process lives.
///
/// There is no minimum: a small positive value is honoured as given, to the
/// resolution of the poll period.
pub fn timeout_from_seconds(tool: &str, seconds: f64) -> Result<Duration, InvalidTimeout> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(InvalidTimeout {
            tool: tool.to_owned(),
            seconds,
        });
    }
    Ok(Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX))
}

/// The directory a process is started in when it works beside `path`.
///
/// `Path::parent` of a bare file name is the empty path, which
/// `Command::current_dir` rejects at spawn time; the current directory is
/// the directory such a relative path already names.
pub(crate) fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// `path` made absolute against the current directory.
///
/// A path handed to a child that is also started in a different working
/// directory must not be relative, or the child resolves it a second time
/// below that directory. The path is joined, not normalised, so the child
/// sees the caller's spelling behind the current directory. Falls back to
/// `path` when the current directory is unreadable.
pub(crate) fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// The Windows `CREATE_NO_WINDOW` process-creation flag.
///
/// Suppresses the console window a windowed process would otherwise allocate
/// for a console subprocess. Hard-coded because `std` does not re-export it and
/// this crate takes no `winapi` dependency for one `u32`; defined only on
/// Windows, where the flag exists.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// True on Windows, where the windowless-spawn flag exists and is wanted.
///
/// The console-window problem [`NoConsoleWindow::no_window`] addresses is
/// Windows-only, so a caller uses this to decide whether the concern applies at
/// all before, say, reporting that a tool ran windowless.
pub fn is_windows() -> bool {
    cfg!(windows)
}

/// Configures a [`Command`] to run without allocating a console window.
pub trait NoConsoleWindow {
    /// Suppress the console window this command would otherwise allocate.
    ///
    /// Sets [`CREATE_NO_WINDOW`] on Windows; a no-op on every other platform,
    /// where no such flag exists. Returns `self` so it reads the same in a
    /// builder chain on either platform. Only a spawn that genuinely wants a
    /// console should skip it, none in this workspace do.
    fn no_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for Command {
    #[cfg(windows)]
    fn no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(CREATE_NO_WINDOW)
    }

    #[cfg(not(windows))]
    fn no_window(&mut self) -> &mut Self {
        self
    }
}

/// Puts a [`Command`]'s child in a process group of its own.
///
/// The Unix half of what [`kill_process_tree`] needs: a signal can be sent to a
/// whole process group at once, but only if the child started one, and only a
/// group the child leads can be signalled without also hitting this process.
/// Upstream spells this `start_new_session=True`. A no-op on Windows, where
/// `taskkill /T` walks the parent-child chain at kill time and nothing has to
/// be arranged at spawn time.
pub trait NewProcessGroup {
    /// Start the child in its own process group, so its descendants can be
    /// signalled together. Returns `self` for chaining.
    fn new_process_group(&mut self) -> &mut Self;
}

impl NewProcessGroup for Command {
    #[cfg(unix)]
    fn new_process_group(&mut self) -> &mut Self {
        use std::os::unix::process::CommandExt;
        // 0 means "a new group led by the child", which is what makes the
        // child's pid usable as the group id below.
        self.process_group(0)
    }

    #[cfg(not(unix))]
    fn new_process_group(&mut self) -> &mut Self {
        self
    }
}

/// Force-kills the process `pid` and every descendant it has.
///
/// `Child::kill` signals only the immediate child, which is not enough for the
/// launcher-plus-solver shape the external tools here have (see the module
/// docs). Both platforms therefore delegate to the tool the system already
/// ships for it rather than to a process-inspection crate:
///
/// * Windows runs `taskkill /F /T`, whose `/T` is its own recursive tree kill.
///   This is upstream's approach, and upstream's reason for it holds here
///   too; there is no `psutil` equivalent in the dependency set, and a tree
///   walk is not worth one.
/// * Unix signals the process group `pid` leads, which is the group
///   [`NewProcessGroup::new_process_group`] arranged at spawn. It shells out to
///   `kill` for the symmetric reason: sending a signal from Rust needs `libc`,
///   the workspace forbids `unsafe`, and `kill` is specified by POSIX to accept
///   a negative pid as a process group.
///
/// Best effort, and returns nothing: the caller is on a path where the child
/// has already failed its timeout, and there is no better outcome available if
/// the kill itself cannot be issued. A child that has already exited is not an
/// error: that is a race this is expected to lose sometimes.
pub fn kill_process_tree(pid: u32) {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("taskkill");
        command.args(["/F", "/T", "/PID", &pid.to_string()]);
        command
    } else {
        let mut command = Command::new("kill");
        command.args(["-s", "KILL", "--", &format!("-{pid}")]);
        command
    };
    // The kill is itself a spawn, and a console window flashing up because a
    // solve timed out would be the second-worst thing about that solve.
    let taskkill_succeeded = command
        .no_window()
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if cfg!(windows) && !taskkill_succeeded {
        // Some Windows hosts deny taskkill even for a child this process owns.
        // PowerShell's process API can still stop it, so use the same built-in
        // tooling to walk descendants before stopping the root process.
        let script = format!(
            "$ErrorActionPreference = 'SilentlyContinue'; function Stop-Tree([int]$p) {{ Get-CimInstance Win32_Process -Filter \"ParentProcessId = $p\" | ForEach-Object {{ Stop-Tree $_.ProcessId }}; Stop-Process -Id $p -Force }}; Stop-Tree {pid}",
            pid = pid
        );
        let mut fallback = Command::new("powershell");
        fallback.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ]);
        let _ = fallback.no_window().output();
    }
}

/// Kill `child` and its descendants, then reap it.
///
/// The `Child` stays alive, and so keeps its OS handle open, until the tree
/// kill has been issued, so the PID cannot be reused by an unrelated process
/// in between. The reaped exit status carries nothing a caller can act on.
pub fn stop_process_tree(child: &mut Child) {
    kill_process_tree(child.id());
    let _ = child.wait();
}

/// How a child supervised by [`wait_with_timeout`] stopped.
#[derive(Debug)]
pub enum DeadlineWait {
    /// The process exited on its own before the deadline.
    Exited(ExitStatus),
    /// The deadline passed; the process tree was killed and reaped.
    TimedOut,
    /// The operating system could not report the process state; the process
    /// tree was killed and reaped so no orphan outlives the caller.
    PollFailed(std::io::Error),
}

/// Wait up to `timeout` for `child`, killing and reaping its whole process
/// tree when the timeout passes or its state can no longer be polled.
///
/// `timeout` comes from [`timeout_from_seconds`], which holds the policy. A
/// deadline beyond the platform clock's range is never reached, where
/// `Instant + Duration` would panic. The child should have been spawned with
/// [`NewProcessGroup::new_process_group`] so the tree kill reaches its
/// descendants on Unix, and any piped output should be read with [`drain`]
/// while this waits.
pub fn wait_with_timeout(child: &mut Child, timeout: Duration) -> DeadlineWait {
    let deadline = Instant::now().checked_add(timeout);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return DeadlineWait::Exited(status),
            Ok(None) if deadline.is_none_or(|deadline| Instant::now() < deadline) => {
                thread::sleep(POLL_INTERVAL);
            }
            Ok(None) => {
                stop_process_tree(child);
                return DeadlineWait::TimedOut;
            }
            Err(error) => {
                stop_process_tree(child);
                return DeadlineWait::PollFailed(error);
            }
        }
    }
}

/// Read a child's piped output to its end on a separate thread.
///
/// A piped stream has a small OS buffer. A tool that writes more than that
/// while the parent only polls for exit blocks on the write and never exits,
/// so a verbose but healthy run would end as a timeout. Draining each piped
/// stream concurrently keeps the pipes empty. `None` (a stream that was not
/// piped) yields no bytes.
pub fn drain<R: Read + Send + 'static>(stream: Option<R>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stream) = stream {
            // A read error truncates a diagnostic tail; the exit status and
            // the produced artifacts remain the success criteria.
            let _ = stream.read_to_end(&mut buffer);
        }
        buffer
    })
}

/// The text a [`drain`] thread collected, decoded lossily.
///
/// A reader thread that panicked yields empty text: the stream is a
/// diagnostic, not a result.
pub fn drained_text(handle: JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&handle.join().unwrap_or_default()).into_owned()
}

// A test spawns a process it built here directly, so a failed expect is the
// spawn failing in the test environment, not a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_that_is_not_finite_and_positive_is_refused_naming_the_tool() {
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -0.0, -5.0] {
            let error = timeout_from_seconds("AVL", seconds)
                .expect_err("only a finite positive timeout is a deadline");
            assert_eq!(error.tool, "AVL");
            assert!(
                error.seconds.to_bits() == seconds.to_bits(),
                "{seconds}: {error}"
            );
            let message = error.to_string();
            assert!(message.starts_with("AVL timeout must be"), "{message}");
        }
    }

    #[test]
    fn a_finite_positive_timeout_is_kept_exactly_and_saturates_instead_of_panicking() {
        assert_eq!(
            timeout_from_seconds("AVL", 2.5),
            Ok(Duration::from_millis(2_500))
        );
        // No floor: a small value is not rounded up to a minimum.
        assert_eq!(
            timeout_from_seconds("AVL", 0.01),
            Ok(Duration::from_millis(10))
        );
        assert_eq!(timeout_from_seconds("AVL", 1.0e300), Ok(Duration::MAX));
    }

    fn sleeping_child() -> std::process::Child {
        #[cfg(windows)]
        let script = "ping -n 30 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 30";
        let (program, mut args) = shell();
        args.push(script);
        Command::new(program)
            .args(&args)
            .no_window()
            .new_process_group()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests")
    }

    #[test]
    fn a_passed_deadline_kills_the_child_tree_and_reports_a_timeout() {
        let mut child = sleeping_child();
        let started = Instant::now();
        let outcome = wait_with_timeout(&mut child, Duration::from_millis(200));
        assert!(matches!(outcome, DeadlineWait::TimedOut), "{outcome:?}");
        assert!(started.elapsed() < Duration::from_secs(20));
        // Reaped: the child has an exit status now.
        assert!(child.try_wait().is_ok_and(|status| status.is_some()));
    }

    #[test]
    fn a_deadline_beyond_the_clock_range_never_expires() {
        let mut child = trivially_successful_command()
            .no_window()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests");
        let outcome = wait_with_timeout(&mut child, Duration::MAX);
        assert!(
            matches!(outcome, DeadlineWait::Exited(status) if status.success()),
            "{outcome:?}"
        );
    }

    #[test]
    fn heavy_piped_output_does_not_stall_a_supervised_child() {
        // About 1 MB on stdout and a little on stderr, far above any OS pipe
        // buffer; without concurrent draining the child blocks on its write
        // and the wait ends as a timeout.
        #[cfg(windows)]
        let script =
            "for /L %i in (1,1,20000) do @echo 0123456789012345678901234567890123456789012345678";
        #[cfg(not(windows))]
        let script = "i=0; while [ $i -lt 20000 ]; do echo 0123456789012345678901234567890123456789012345678; i=$((i+1)); done";
        let (program, mut args) = shell();
        args.push(script);
        let mut child = Command::new(program)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .no_window()
            .new_process_group()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests");
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let outcome = wait_with_timeout(&mut child, Duration::from_secs(120));
        assert!(
            matches!(outcome, DeadlineWait::Exited(status) if status.success()),
            "{outcome:?}"
        );
        assert!(drained_text(stdout).len() > 500_000);
        assert!(drained_text(stderr).is_empty());
    }

    #[test]
    fn a_bare_file_name_runs_in_the_current_directory() {
        assert_eq!(parent_directory(Path::new("case.avl")), Path::new("."));
        assert_eq!(
            parent_directory(Path::new("outputs/case.avl")),
            Path::new("outputs")
        );
        let relative = absolute_path(Path::new("outputs/request.txt"));
        assert!(relative.is_absolute());
        assert!(relative.ends_with("outputs/request.txt"));
    }

    #[test]
    fn is_windows_matches_the_build_target() {
        assert_eq!(is_windows(), cfg!(windows));
    }

    #[cfg(windows)]
    #[test]
    fn create_no_window_is_the_documented_win32_flag() {
        // The value winbase.h defines for CREATE_NO_WINDOW. Pinned here so a
        // typo in the constant is a test failure rather than a silently
        // console-flashing spawn nobody notices until they run the app.
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }

    #[test]
    fn a_windowless_command_still_runs_and_preserves_its_exit_status() {
        // Whether a window actually appears is unobservable from a test, so
        // this pins the other half of the contract: applying the flag must not
        // change what the command does; it still spawns and its exit status
        // is preserved.
        let mut command = trivially_successful_command();
        let status = command
            .no_window()
            .status()
            .expect("the OS shell is present in any environment that runs these tests");
        assert!(status.success());

        let mut command = trivially_failing_command();
        let status = command
            .no_window()
            .status()
            .expect("the OS shell is present in any environment that runs these tests");
        assert!(!status.success());
    }

    #[cfg(windows)]
    fn trivially_successful_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit", "0"]);
        command
    }

    #[cfg(windows)]
    fn trivially_failing_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit", "1"]);
        command
    }

    #[cfg(not(windows))]
    fn trivially_successful_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        command
    }

    #[cfg(not(windows))]
    fn trivially_failing_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 1"]);
        command
    }

    #[test]
    fn killing_a_tree_stops_the_process_that_leads_it() {
        // The descendant half of the contract cannot be observed portably;
        // it needs a launcher that forks a solver, which is the very thing this
        // machine has no install of. What is observable is that the call
        // reaches the right process at all: a child that would otherwise run
        // for a minute is gone directly afterwards.
        #[cfg(windows)]
        let script = "ping -n 60 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 60";
        let (program, mut args) = shell();
        args.push(script);
        let mut child = Command::new(program)
            .args(&args)
            .no_window()
            .new_process_group()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests");

        kill_process_tree(child.id());
        let status = child
            .wait()
            .expect("a spawned child can always be waited on");
        assert!(!status.success());
    }

    #[test]
    fn killing_a_process_that_has_already_exited_is_not_an_error() {
        // The race this is expected to lose: a run times out, the child exits
        // on its own before the kill is issued, and the kill must stay quiet.
        let mut child = trivially_successful_command()
            .no_window()
            .new_process_group()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests");
        let _ = child.wait();
        kill_process_tree(child.id());
    }

    #[cfg(windows)]
    fn shell() -> (&'static str, Vec<&'static str>) {
        ("cmd", vec!["/C"])
    }

    #[cfg(not(windows))]
    fn shell() -> (&'static str, Vec<&'static str>) {
        ("sh", vec!["-c"])
    }
}
