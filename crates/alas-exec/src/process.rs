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
//! `subprocess.Popen.__init__` process-wide so that spawns it does *not* own --
//! the ones issued from inside native aerodynamic model's own MSES wrapper -- also run
//! windowless. That has no counterpart here and needs none: this port drives
//! the external binaries itself, through this crate, rather than through a
//! third-party library that spawns its own processes. There is no spawn outside
//! this crate's reach to patch, so reproducing the global patch would mean
//! reproducing a workaround for a problem the translation removes. The guard
//! logic that patch carried -- respect an explicit `startupinfo`, never combine
//! the flag with `CREATE_NEW_CONSOLE`/`DETACHED_PROCESS` -- guarded exactly the
//! third-party spawns that are gone with it.
//!
//! Capturing output, feeding stdin and enforcing a timeout stay at the call
//! sites, as they did upstream: they are per-call decisions, and a spawn that
//! wants them says so where it spawns.
//!
//! [`kill_process_tree`] is the other half this crate owns, and it is here for
//! the same reason the windowless flag is: it is a property of *how a process
//! is spawned*, not of what any one tool does with its output. Killing a child
//! is not enough for the tools this workspace drives -- `nastran.exe` is a
//! launcher that forks the real solver, and upstream records having watched the
//! orphan keep burning CPU and holding a licence seat after the parent was
//! killed. A tree kill needs the spawn to have been set up for it on Unix,
//! which is why [`NewProcessGroup::new_process_group`] and
//! [`kill_process_tree`] are one pair of things rather than two.

use std::process::Command;

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
    /// console should skip it -- none in this workspace do.
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
///   too -- there is no `psutil` equivalent in the dependency set, and a tree
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
/// error -- that is a race this is expected to lose sometimes.
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

// A test spawns a process it built here directly, so a failed expect is the
// spawn failing in the test environment, not a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

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
        // change what the command does -- it still spawns and its exit status
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
        // The descendant half of the contract cannot be observed portably --
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
