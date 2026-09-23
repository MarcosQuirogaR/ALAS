// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/nastran_runner.py (_tail, _kill_process_tree,
// _run_nastran and NastranRunOutcome).
// Reference: alas @ rust-port-baseline.

//! Running one solve, and saying precisely why it did not work.
//!
//! [`run_nastran`] never fails: a solve that times out, exits non-zero, writes
//! no `.f06` or writes one containing a fatal message all come back as a
//! [`NastranRunOutcome`] that is not `ok`, carrying a `detail` a user can act
//! on. That contract is upstream's and it is the point of the module: a
//! pipeline run must not stop because NASTRAN is not installed or a model did
//! not solve, and "did not converge" with nothing attached is useless on a
//! machine the author cannot see.
//!
//! Two decisions here are load-bearing and are upstream's, recorded with the
//! evidence upstream recorded for them:
//!
//! * The scratch directory is passed explicitly rather than left to the
//!   install's own `.rcf`, because a real MSC Nastran Student Edition install
//!   was found shipping `sdirectory=e:`, a drive that did not exist on the
//!   machine it was installed on, which fails before a solve starts. The BDF's
//!   own directory is guaranteed to exist, since the BDF was just written into
//!   it, and is passed absolute because NASTRAN's own validation of the keyword
//!   does not necessarily resolve a relative path against the working directory
//!   the process was launched with.
//! * A timeout kills the whole process tree, not the child. `nastran.exe` is a
//!   front end that forks the actual solver, and killing only the front end
//!   leaves that solver running: observed directly upstream, still consuming
//!   CPU and still holding a licence seat. [`alas_exec::process`] owns that.
//!
//! [`solver_arguments`] builds the frozen reference command line and
//! [`msc_solver_arguments`] builds the installed MSC launcher command line.
//! Both are pure apart from converting a whitespace-bearing installed path to
//! the DOS 8.3 spelling MSC's legacy launcher requires. [`tail`] and
//! [`fatal_lines`] are the text half and are compared
//! against the reference on fixture data; and everything between the spawn and
//! the verdict lives in `supervise`, which takes an already-built command and is
//! therefore driven by stand-ins, one that exits cleanly, one that never
//! terminates. An opt-in installed-solver check establishes the remaining
//! product boundary where MSC is available.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use alas_exec::process::{
    drain, drained_text, timeout_from_seconds, wait_with_timeout, DeadlineWait, NewProcessGroup,
    NoConsoleWindow,
};
use alas_exec::SupervisedSpawn;

use super::text;

/// How many lines of a captured stream a failure report quotes.
const TAIL_LINES: usize = 15;

/// How many fatal messages a failure report quotes before it stops.
const FATAL_LINES_REPORTED: usize = 5;

/// What one solve did, and why it is judged that way.
///
/// Upstream's own docstring makes the case for the `detail` string over a bare
/// bool, and it holds here: the failures this reports (a timeout, a non-zero
/// exit, a missing `.f06`, a fatal message, a process that never launched)
/// are indistinguishable to a caller and completely different to a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NastranRunOutcome {
    /// Whether the solve produced results worth reading.
    pub ok: bool,
    /// Why, in terms a user can act on. `"ok"` when it worked.
    pub detail: String,
}

impl NastranRunOutcome {
    fn failed(detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            detail: detail.into(),
        }
    }

    fn succeeded() -> Self {
        Self {
            ok: true,
            detail: "ok".to_owned(),
        }
    }
}

/// The last `n_lines` lines of `text_input`, or `"(empty)"` if there are none.
///
/// Quoting the tail rather than the whole stream is what makes a failure detail
/// readable: a solver that fails after a banner and a hundred lines of progress
/// says why at the end, not at the start.
pub fn tail(text_input: &str, n_lines: usize) -> String {
    let lines = text::splitlines(text::strip(text_input));
    if lines.is_empty() {
        return "(empty)".to_owned();
    }
    lines[lines.len().saturating_sub(n_lines)..].join("\n")
}

/// Every `USER FATAL MESSAGE` line in an `.f06`, stripped of its padding.
///
/// A NASTRAN run can exit zero having solved nothing; the fatal message in the
/// print file is the only thing that says so. See [`super::text`] for why this
/// does not split on newlines alone.
pub fn fatal_lines(content: &str) -> Vec<String> {
    text::splitlines(content)
        .into_iter()
        .filter(|line| line.contains("USER FATAL MESSAGE"))
        .map(|line| text::strip(line).to_owned())
        .collect()
}

/// The command line one solve is launched with: the deck, then the two keywords.
///
/// The deck is named rather than pathed because the process is launched in its
/// directory. `scratch` is that directory made absolute, for the reason the
/// module docs give.
pub fn solver_arguments(bdf_path: &Path) -> Vec<String> {
    let deck = file_name(bdf_path);
    let directory = bdf_path.parent().unwrap_or(Path::new("."));
    let absolute = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let mut scratch = absolute.display().to_string();
    // canonicalize hands back an extended-length path on Windows, a form the
    // solver's own keyword validation does not accept.
    if let Some(plain) = scratch.strip_prefix(r"\\?\") {
        scratch = plain.to_owned();
    }
    vec![deck, "scr=yes".to_owned(), format!("sdirectory={scratch}")]
}

/// The command tokens for an MSC launcher that needs an explicit solver.
///
/// This is a separate product path rather than a change to
/// [`solver_arguments`], whose three tokens are frozen reference behavior.
/// MSC's launcher constructs a second command line without quoting
/// `a.solver`, so an installed path containing whitespace must be represented
/// by its DOS 8.3 spelling on Windows.
pub fn msc_solver_arguments(bdf_path: &Path, solver_path: &Path) -> Result<Vec<String>, String> {
    let deck = file_name(bdf_path);
    let directory = absolute_directory(bdf_path);
    let work = plain_path(&directory);
    let solver = msc_command_token(solver_path)?;
    Ok(vec![
        deck,
        "scr=no".to_owned(),
        format!("sdirectory={work}"),
        format!("dbs={work}"),
        format!("a.solver={solver}"),
    ])
}

/// Run `exe_path` on `bdf_path`, and report what happened.
///
/// Never returns an error: every failure mode is a non-`ok`
/// [`NastranRunOutcome`]. `timeout_seconds` bounds the whole solve, after which
/// the process tree is force-killed.
pub fn run_nastran(bdf_path: &Path, exe_path: &Path, timeout_seconds: f64) -> NastranRunOutcome {
    run_nastran_with_solver(bdf_path, exe_path, None, timeout_seconds)
}

/// Run MSC Nastran, optionally overriding the solver used by its launcher.
///
/// A missing override preserves the reference launch contract. Supplying one
/// selects the independently validated MSC Student Edition contract, including
/// `dbs` and the `a.solver` token.
pub fn run_nastran_with_solver(
    bdf_path: &Path,
    exe_path: &Path,
    solver_path: Option<&Path>,
    timeout_seconds: f64,
) -> NastranRunOutcome {
    let (program, arguments) = match solver_path {
        Some(solver) => {
            let program = match msc_command_token(exe_path) {
                Ok(token) => token,
                Err(error) => {
                    return NastranRunOutcome::failed(format!(
                        "MSC NASTRAN launcher path is unusable: {error}"
                    ))
                }
            };
            let arguments = match msc_solver_arguments(bdf_path, solver) {
                Ok(arguments) => arguments,
                Err(error) => {
                    return NastranRunOutcome::failed(format!(
                        "MSC NASTRAN solver override is unusable: {error}"
                    ))
                }
            };
            (program, arguments)
        }
        None => (exe_path.display().to_string(), solver_arguments(bdf_path)),
    };
    let work_dir = bdf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut command = Command::new(&program);
    command.args(&arguments).current_dir(&work_dir);
    let described = format!("{program} {}", arguments.join(" "));
    supervise(
        command,
        &Solve {
            command_line: described,
            exe_name: file_name(Path::new(&program)),
            bdf_path: bdf_path.to_path_buf(),
            work_dir,
            timeout_seconds,
        },
    )
}

fn absolute_directory(bdf_path: &Path) -> PathBuf {
    let directory = bdf_path.parent().unwrap_or(Path::new("."));
    std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf())
}

fn plain_path(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// Make an installed executable path safe for MSC's legacy launcher chain.
fn msc_command_token(path: &Path) -> Result<String, String> {
    let original = path.to_string_lossy().into_owned();
    if !original.chars().any(char::is_whitespace) {
        return Ok(original);
    }

    #[cfg(windows)]
    {
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/s", "/c"]);
        command.raw_arg("for %I in (\"%ALAS_MSC_EXECUTABLE_PATH%\") do @echo %~sI");
        let output = command
            .env("ALAS_MSC_EXECUTABLE_PATH", path)
            .output()
            .map_err(|error| format!("could not query its DOS 8.3 path: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "querying its DOS 8.3 path failed with {}",
                output.status
            ));
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return Err(format!(
                "its installed path contains whitespace and this volume has no usable DOS 8.3 name ({token:?})"
            ));
        }
        Ok(token)
    }

    #[cfg(not(windows))]
    {
        Err(format!(
            "its installed path contains whitespace; MSC's launcher needs a DOS 8.3 name on Windows ({original})"
        ))
    }
}

/// What `supervise` needs to know about a solve that its `Command` does not say.
struct Solve {
    command_line: String,
    exe_name: String,
    bdf_path: PathBuf,
    work_dir: PathBuf,
    timeout_seconds: f64,
}

/// Spawn `command`, hold it to the timeout, then judge what it left behind.
fn supervise(mut command: Command, solve: &Solve) -> NastranRunOutcome {
    let timeout = match timeout_from_seconds(&solve.exe_name, solve.timeout_seconds) {
        Ok(timeout) => timeout,
        Err(error) => return NastranRunOutcome::failed(error.to_string()),
    };
    let spawned = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group()
        .spawn_supervised("NASTRAN solve");
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            return NastranRunOutcome::failed(format!(
                "Failed to launch {}: {error}",
                solve.exe_name
            ))
        }
    };

    // Both streams are drained on their own threads: a solver can fill one
    // pipe's buffer while the other is untouched, and a full pipe blocks it
    // forever, which would turn every chatty run into a timeout.
    let stdout_reader = drain(child.stdout.take());
    let stderr_reader = drain(child.stderr.take());

    let status = match wait_with_timeout(&mut child, timeout) {
        DeadlineWait::Exited(status) => status,
        // Whatever the solver had buffered is discarded with it.
        DeadlineWait::TimedOut => {
            return NastranRunOutcome::failed(format!(
                "Timed out after {:.0}s running: {} \
                 (solver process tree has been force-killed)",
                solve.timeout_seconds, solve.command_line
            ))
        }
        DeadlineWait::PollFailed(error) => {
            return NastranRunOutcome::failed(format!(
                "Failed while waiting on {}: {error}",
                solve.exe_name
            ))
        }
    };

    let stdout = drained_text(stdout_reader);
    let stderr = drained_text(stderr_reader);
    let streams = format!(
        "stdout (tail):\n{}\nstderr (tail):\n{}",
        tail(&stdout, TAIL_LINES),
        tail(&stderr, TAIL_LINES)
    );

    if !status.success() {
        let code = match status.code() {
            Some(code) => code.to_string(),
            None => "a signal".to_owned(),
        };
        let loader_hint = loader_failure_hint(status.code());
        return NastranRunOutcome::failed(format!(
            "{} exited with code {code}{loader_hint} (cwd={}).\n{streams}",
            solve.exe_name,
            solve.work_dir.display()
        ));
    }

    let f06_path = solve.bdf_path.with_extension("f06");
    if !f06_path.exists() {
        return NastranRunOutcome::failed(format!(
            "{} exited 0 but wrote no {} in {}; if this executable is a GUI-mode launcher \
             (e.g. a *w.exe variant) it may have opened a window and returned immediately \
             instead of blocking until the solve finished, or it may write output to a \
             different working directory than the one it was launched from. {streams}",
            solve.exe_name,
            file_name(&f06_path),
            solve.work_dir.display()
        ));
    }

    // Whatever the solver wrote, read as text: upstream opens this with
    // errors="replace" because a print file that is not quite UTF-8 is a reason
    // to keep reading, not a reason to fail the run.
    let content = match std::fs::read(&f06_path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => format!("(could not be read: {error})"),
    };
    let fatals = fatal_lines(&content);
    if fatals.is_empty() {
        return NastranRunOutcome::succeeded();
    }
    let quoted: Vec<&str> = fatals
        .iter()
        .take(FATAL_LINES_REPORTED)
        .map(String::as_str)
        .collect();
    NastranRunOutcome::failed(format!(
        "{} reports {} fatal message(s):\n{}",
        file_name(&f06_path),
        fatals.len(),
        quoted.join("\n")
    ))
}

/// Add a useful diagnosis for the Windows loader status that appears when a
/// launcher is present on disk but its side-by-side runtime is not.
fn loader_failure_hint(code: Option<i32>) -> &'static str {
    // Windows STATUS_DLL_NOT_FOUND. The native launcher can be a regular file
    // while its CRT dependency is absent; naming that distinction is much more
    // useful than an opaque negative exit code in the external-tools panel.
    match code {
        Some(-1_073_741_515) => {
            " (Windows 0xC0000135: a required DLL or side-by-side runtime is missing; inspect the executable manifest and install/runtime PATH)"
        }
        _ => "",
    }
}

/// A path's final component, for a message that names a file rather than a path.
fn file_name(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.display().to_string(),
    }
}

// These tests drive a real subprocess and a real temporary directory, so a
// failed unwrap is the test environment failing rather than a library invariant
// being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_stream_reports_itself_as_empty_rather_than_as_nothing() {
        assert_eq!(tail("", TAIL_LINES), "(empty)");
        assert_eq!(tail("   \n\n ", TAIL_LINES), "(empty)");
    }

    #[test]
    fn a_tail_keeps_the_last_lines_and_drops_the_rest() {
        let text: String = (0..40).map(|i| format!("line {i}\n")).collect();
        assert_eq!(tail(&text, 3), "line 37\nline 38\nline 39");
    }

    #[test]
    fn a_paginated_fatal_message_is_found_on_its_own_line() {
        // The failure this module's text handling exists to prevent: the fatal
        // line follows a form feed, not a newline.
        let f06 = "0 THE FOLLOWING CARD WAS NOT RECOGNIZED\n\
                   \u{c}   *** USER FATAL MESSAGE 9994 (IFP)   \n\
                   0*** USER WARNING MESSAGE 4698\n";
        assert_eq!(fatal_lines(f06), ["*** USER FATAL MESSAGE 9994 (IFP)"]);
    }

    #[test]
    fn a_clean_print_file_reports_no_fatal_messages() {
        assert!(fatal_lines("  * * * END OF JOB * * *\n").is_empty());
    }

    #[test]
    fn the_command_line_names_the_deck_and_scratches_beside_it() {
        let work = TempDir::new("nastran_args");
        let bdf = work.path().join("wing_sol101.bdf");
        std::fs::write(&bdf, "CEND\n").unwrap();

        let arguments = solver_arguments(&bdf);
        assert_eq!(arguments[0], "wing_sol101.bdf");
        assert_eq!(arguments[1], "scr=yes");
        let scratch = arguments[2].strip_prefix("sdirectory=").unwrap();
        assert!(Path::new(scratch).is_absolute());
        // The extended-length prefix would fail the solver's own validation.
        assert!(!scratch.starts_with(r"\\?\"), "{scratch}");
        assert!(Path::new(scratch).exists());
    }

    #[test]
    fn the_split_msc_install_receives_exact_launcher_keywords_as_separate_tokens() {
        let work = TempDir::new("msc_args");
        let bdf = work.path().join("wing_sol101.bdf");
        std::fs::write(&bdf, "CEND\n").unwrap();

        let arguments =
            msc_solver_arguments(&bdf, Path::new("C:/MSC/PATRAN/analysis.exe")).unwrap();
        let directory = plain_path(&absolute_directory(&bdf));
        assert_eq!(
            arguments,
            [
                "wing_sol101.bdf".to_owned(),
                "scr=no".to_owned(),
                format!("sdirectory={directory}"),
                format!("dbs={directory}"),
                "a.solver=C:/MSC/PATRAN/analysis.exe".to_owned(),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_installed_msc_solver_override_is_a_single_whitespace_free_token() {
        let Some(path) = std::env::var_os("ALAS_MSC_SOLVER") else {
            return;
        };
        let token = msc_command_token(Path::new(&path)).unwrap();
        assert!(
            !token.chars().any(char::is_whitespace),
            "MSC's inner launcher must receive one path token: {token:?}"
        );
    }

    #[test]
    fn a_missing_executable_is_reported_rather_than_raised() {
        let outcome = run_nastran(
            Path::new("wing_sol101.bdf"),
            Path::new("no_such_nastran_executable_anywhere"),
            5.0,
        );
        assert!(!outcome.ok);
        assert!(
            outcome.detail.starts_with("Failed to launch"),
            "{}",
            outcome.detail
        );
    }

    #[test]
    fn a_solver_that_never_finishes_is_killed_and_reported_as_a_timeout() {
        let work = TempDir::new("nastran_timeout");
        let outcome = stand_in(&work, sleeping_command(), 0.4, None);
        assert!(!outcome.ok);
        assert!(
            outcome.detail.starts_with("Timed out after 0s running:"),
            "{}",
            outcome.detail
        );
        assert!(outcome.detail.contains("force-killed"));
    }

    #[test]
    fn an_unusable_timeout_is_reported_before_anything_is_launched() {
        let work = TempDir::new("nastran_bad_timeout");
        for timeout_seconds in [-1.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            // The stand-in would succeed if it ran, so a clean outcome or a
            // missing-print-file report would mean it was launched.
            let outcome = stand_in(&work, successful_command(), timeout_seconds, None);
            assert!(!outcome.ok, "{timeout_seconds}: {}", outcome.detail);
            assert!(
                outcome.detail.starts_with("cmd timeout must be")
                    || outcome.detail.starts_with("sh timeout must be"),
                "{timeout_seconds}: {}",
                outcome.detail
            );
            assert!(
                !outcome.detail.contains("wrote no"),
                "{timeout_seconds}: {}",
                outcome.detail
            );
        }
    }

    #[test]
    fn a_solver_that_exits_non_zero_is_reported_with_its_code() {
        let work = TempDir::new("nastran_nonzero");
        let outcome = stand_in(&work, failing_command(), 20.0, None);
        assert!(!outcome.ok);
        assert!(
            outcome.detail.contains("exited with code 1"),
            "{}",
            outcome.detail
        );
        assert!(outcome.detail.contains("stdout (tail):"));
    }

    #[test]
    fn the_windows_loader_failure_has_a_runtime_dependency_hint() {
        assert!(super::loader_failure_hint(Some(-1_073_741_515)).contains("0xC0000135"));
        assert!(super::loader_failure_hint(Some(1)).is_empty());
    }

    #[test]
    fn a_clean_exit_with_no_print_file_is_not_a_successful_solve() {
        let work = TempDir::new("nastran_no_f06");
        let outcome = stand_in(&work, successful_command(), 20.0, None);
        assert!(!outcome.ok);
        assert!(
            outcome.detail.contains("wrote no wing_sol101.f06"),
            "{}",
            outcome.detail
        );
    }

    #[test]
    fn a_print_file_carrying_a_fatal_message_is_not_a_successful_solve() {
        let work = TempDir::new("nastran_fatal");
        let f06 = "\u{c} *** USER FATAL MESSAGE 9994 (IFP)\n\
                    *** USER FATAL MESSAGE 307 (XSORT)\n";
        let outcome = stand_in(&work, successful_command(), 20.0, Some(f06));
        assert!(!outcome.ok);
        assert!(
            outcome
                .detail
                .starts_with("wing_sol101.f06 reports 2 fatal message(s):"),
            "{}",
            outcome.detail
        );
    }

    #[test]
    fn a_clean_exit_with_a_clean_print_file_is_a_successful_solve() {
        let work = TempDir::new("nastran_ok");
        let outcome = stand_in(
            &work,
            successful_command(),
            20.0,
            Some("* * * END OF JOB * * *\n"),
        );
        assert!(outcome.ok, "{}", outcome.detail);
        assert_eq!(outcome.detail, "ok");
    }

    /// Drive `supervise` with a command standing in for the solver.
    ///
    /// Everything after the spawn (the timeout and its tree kill, the exit
    /// status, the print file and its fatal scan) is what these exercise.
    /// The solver's own arguments are covered separately, by
    /// `the_command_line_names_the_deck_and_scratches_beside_it`.
    fn stand_in(
        work: &TempDir,
        (program, arguments): (&str, Vec<String>),
        timeout_seconds: f64,
        f06: Option<&str>,
    ) -> NastranRunOutcome {
        let bdf = work.path().join("wing_sol101.bdf");
        std::fs::write(&bdf, "CEND\n").unwrap();
        if let Some(content) = f06 {
            std::fs::write(work.path().join("wing_sol101.f06"), content).unwrap();
        }
        let mut command = Command::new(program);
        command.args(&arguments).current_dir(work.path());
        supervise(
            command,
            &Solve {
                command_line: format!("{program} {}", arguments.join(" ")),
                exe_name: program.to_owned(),
                bdf_path: bdf,
                work_dir: work.path().to_path_buf(),
                timeout_seconds,
            },
        )
    }

    #[cfg(windows)]
    fn shell(script: &str) -> (&'static str, Vec<String>) {
        ("cmd", vec!["/C".to_owned(), script.to_owned()])
    }

    #[cfg(not(windows))]
    fn shell(script: &str) -> (&'static str, Vec<String>) {
        ("sh", vec!["-c".to_owned(), script.to_owned()])
    }

    fn sleeping_command() -> (&'static str, Vec<String>) {
        #[cfg(windows)]
        let script = "ping -n 60 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 60";
        shell(script)
    }

    fn successful_command() -> (&'static str, Vec<String>) {
        shell("echo solved")
    }

    fn failing_command() -> (&'static str, Vec<String>) {
        shell("echo solved && exit 1")
    }

    /// A directory that deletes itself, so a test leaves no deck behind.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("alas_{label}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
