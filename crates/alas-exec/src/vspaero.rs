// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native VSPAERO process execution with observable failure states.
//!
//! OpenVSP's result database is not the solver output contract: result keys
//! have changed across releases while VSPAERO's native `.polar` file has
//! remained the durable batch boundary. This module launches `vspaero`
//! directly and proves that a fresh polar was produced. Aerodynamic parsing
//! belongs to `alas-aero`, which already depends on this crate.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};

/// Wake model declared by the native `.vspaero` setup.
///
/// This is setup provenance, not a convergence result. A frozen wake is a
/// deliberately fixed-surface solve; it must never be reported as a
/// converged free-wake solution merely because the process exited cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroWakeMode {
    /// The wake is allowed to update through the requested iterations.
    FreeWake,
    /// The wake is configured to freeze at the recorded outer-loop iteration.
    FrozenWake {
        /// Iteration at which VSPAERO is configured to stop updating the wake.
        at_iteration: usize,
    },
}

impl VspaeroWakeMode {
    /// Stable text used by retained runtime summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FreeWake => "free_wake",
            Self::FrozenWake { .. } => "frozen_wake",
        }
    }
}

/// Wake orientation represented by a hand-authored VSPAERO case.
///
/// The native case format does not carry an independent wake-direction vector;
/// VSPAERO initializes the wake in the freestream direction. Recording that
/// fact keeps a frozen result distinct from ALAS/AVL cases that use an
/// X-parallel trailing wake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroWakeAlignment {
    /// Native VSPAERO initial wake follows the freestream direction.
    InitialFreeStream,
}

impl VspaeroWakeAlignment {
    /// Stable text used by retained runtime summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InitialFreeStream => "initial_free_stream",
        }
    }
}

/// Wake and linear-solver settings parsed from a native `.vspaero` setup.
///
/// The values describe what the case requested. They do not prove that the
/// native process reached the freeze iteration or that its coefficient history
/// converged; those claims remain the responsibility of the result/history
/// checks in the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VspaeroWakeSettings {
    /// Requested outer wake iterations.
    pub wake_iterations: usize,
    /// Configured wake-freeze iteration.
    pub freeze_wake_at_iteration: usize,
    /// Whether VSPAERO's implicit wake coupling was requested.
    pub implicit_wake: bool,
    /// Wake relaxation factor.
    pub wake_relax: f64,
    /// Forward GMRES convergence factor.
    pub forward_gmres_convergence_factor: f64,
    /// Native initial wake alignment.
    pub initial_wake_alignment: VspaeroWakeAlignment,
}

impl VspaeroWakeSettings {
    /// Classify the declared setup as free or frozen using its own iteration
    /// settings. This does not assert that the process reached that iteration.
    pub fn mode(self) -> VspaeroWakeMode {
        if self.freeze_wake_at_iteration <= self.wake_iterations {
            VspaeroWakeMode::FrozenWake {
                at_iteration: self.freeze_wake_at_iteration,
            }
        } else {
            VspaeroWakeMode::FreeWake
        }
    }
}

/// Native VSPAERO process outcome before aerodynamic parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroProcessStatus {
    /// The `.vspgeom` or `.vspaero` input is absent.
    InputMissing,
    /// The executable could not be launched.
    LaunchFailed,
    /// The process exceeded its deadline and its tree was killed.
    TimedOut,
    /// VSPAERO returned a non-zero process status.
    SolverFailed,
    /// VSPAERO returned success without producing a fresh non-empty polar.
    OutputMissing,
    /// VSPAERO returned success and produced a fresh native polar.
    Completed,
}

impl VspaeroProcessStatus {
    /// Stable text used by retained runtime summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InputMissing => "input_missing",
            Self::LaunchFailed => "launch_failed",
            Self::TimedOut => "timed_out",
            Self::SolverFailed => "solver_failed",
            Self::OutputMissing => "output_missing",
            Self::Completed => "completed",
        }
    }
}

/// Files and diagnostics retained from one native VSPAERO invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct VspaeroProcessResult {
    /// Explicit process-level state.
    pub status: VspaeroProcessStatus,
    /// Extensionless case path passed to VSPAERO.
    pub case_path: PathBuf,
    /// Native polar expected beside the case inputs.
    pub polar_path: PathBuf,
    /// Captured standard output.
    pub stdout_path: PathBuf,
    /// Captured standard error.
    pub stderr_path: PathBuf,
    /// Parsed wake/linear-solver setup values, when the adjacent setup is
    /// complete enough to classify. `None` is an explicit unknown state.
    pub wake_settings: Option<VspaeroWakeSettings>,
    /// Declared wake model derived from `wake_settings`.
    pub wake_mode: Option<VspaeroWakeMode>,
    /// Actionable failure detail, absent only for a completed run.
    pub error: Option<String>,
}

/// Parse wake and linear-solver provenance from native `.vspaero` text.
///
/// Missing or malformed settings return `None`; callers must preserve that
/// unknown state rather than guessing free or frozen wake behavior.
pub fn parse_wake_settings(setup_text: &str) -> Option<VspaeroWakeSettings> {
    let wake_iterations = parse_nonnegative_setting(setup_text, "WakeIters")?;
    let freeze_wake_at_iteration = parse_nonnegative_setting(setup_text, "FreezeWakeAtIteration")?;
    let implicit_wake = parse_flag_setting(setup_text, "ImplicitWake")?;
    let wake_relax = parse_finite_setting(setup_text, "WakeRelax")?;
    let forward_gmres_convergence_factor =
        parse_finite_setting(setup_text, "ForwardGMRESConvergenceFactor")?;
    Some(VspaeroWakeSettings {
        wake_iterations,
        freeze_wake_at_iteration,
        implicit_wake,
        wake_relax,
        forward_gmres_convergence_factor,
        initial_wake_alignment: VspaeroWakeAlignment::InitialFreeStream,
    })
}

/// Read and parse an adjacent native `.vspaero` setup for provenance.
pub fn inspect_wake_settings(setup_path: &Path) -> Option<VspaeroWakeSettings> {
    let setup_text = fs::read_to_string(setup_path).ok()?;
    parse_wake_settings(&setup_text)
}

fn setting_value<'a>(setup_text: &'a str, key: &str) -> Option<&'a str> {
    setup_text
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim())
        .next_back()
}

fn parse_nonnegative_setting(setup_text: &str, key: &str) -> Option<usize> {
    setting_value(setup_text, key)?
        .parse::<u64>()
        .ok()?
        .try_into()
        .ok()
}

fn parse_flag_setting(setup_text: &str, key: &str) -> Option<bool> {
    match setting_value(setup_text, key)?.parse::<u8>().ok()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn parse_finite_setting(setup_text: &str, key: &str) -> Option<f64> {
    let value = setting_value(setup_text, key)?.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

/// Execute native VSPAERO for an extensionless case path.
///
/// The case must already have adjacent `.vspgeom`, `.vkey`, and `.vspaero`
/// files. Stale `.polar` and `.history` files are removed before launch, so
/// prior results cannot turn a failed or non-converged invocation into
/// apparent success.
pub fn run_vspaero(
    executable: &Path,
    case_path: &Path,
    thread_count: usize,
    timeout_seconds: f64,
) -> VspaeroProcessResult {
    let polar_path = case_path.with_extension("polar");
    let history_path = case_path.with_extension("history");
    let setup_path = case_path.with_extension("vspaero");
    let wake_settings = inspect_wake_settings(&setup_path);
    let stdout_path = case_path.with_extension("vspaero.stdout.txt");
    let stderr_path = case_path.with_extension("vspaero.stderr.txt");
    let mut result = VspaeroProcessResult {
        status: VspaeroProcessStatus::InputMissing,
        case_path: case_path.to_path_buf(),
        polar_path: polar_path.clone(),
        stdout_path: stdout_path.clone(),
        stderr_path: stderr_path.clone(),
        wake_mode: wake_settings.map(VspaeroWakeSettings::mode),
        wake_settings,
        error: None,
    };

    let missing = ["vspgeom", "vkey", "vspaero"]
        .into_iter()
        .map(|extension| case_path.with_extension(extension))
        .filter(|path| !path.is_file())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        result.error = Some(format!(
            "VSPAERO input files are missing: {}",
            missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        return result;
    }
    if polar_path.exists() {
        if let Err(error) = fs::remove_file(&polar_path) {
            result.error = Some(format!(
                "cannot remove stale {}: {error}",
                polar_path.display()
            ));
            return result;
        }
    }
    if history_path.exists() {
        if let Err(error) = fs::remove_file(&history_path) {
            result.error = Some(format!(
                "cannot remove stale {}: {error}",
                history_path.display()
            ));
            return result;
        }
    }

    let stdout = match File::create(&stdout_path) {
        Ok(file) => file,
        Err(error) => {
            result.error = Some(format!("cannot create {}: {error}", stdout_path.display()));
            return result;
        }
    };
    let stderr = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            result.error = Some(format!("cannot create {}: {error}", stderr_path.display()));
            return result;
        }
    };
    let working_directory = case_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // VSPAERO resolves its case argument after changing into the case
    // directory. Passing the original relative path would therefore prepend
    // that directory twice (for example `outputs/openvsp/outputs/openvsp`),
    // while an absolute path happens to work. The case file name is valid for
    // both relative and absolute callers once the working directory is set.
    let case_argument = vspaero_case_argument(case_path);
    let mut command = Command::new(executable);
    command
        .args(["-omp", &thread_count.max(1).to_string()])
        .arg(case_argument)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .no_window()
        .new_process_group();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            result.status = VspaeroProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "failed to launch {}: {error}",
                executable.display()
            ));
            return result;
        }
    };

    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds.max(0.1));
    let process_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                result.status = VspaeroProcessStatus::TimedOut;
                result.error = Some(format!(
                    "VSPAERO exceeded the {timeout_seconds:.1} s deadline; process tree force-killed"
                ));
                return result;
            }
            Err(error) => {
                result.status = VspaeroProcessStatus::SolverFailed;
                result.error = Some(format!("cannot poll VSPAERO: {error}"));
                return result;
            }
        }
    };

    if !process_status.success() {
        let stdout_text = fs::read_to_string(&stdout_path).unwrap_or_default();
        let stderr_text = fs::read_to_string(&stderr_path).unwrap_or_default();
        result.status = VspaeroProcessStatus::SolverFailed;
        result.error = Some(format!(
            "VSPAERO exited with {}; case {}; inspect stdout {} and stderr {}; stdout tail: {}; stderr tail: {}",
            process_status
                .code()
                .map_or_else(|| "unknown code".to_owned(), |code| code.to_string()),
            case_path.display(),
            stdout_path.display(),
            stderr_path.display(),
            text_tail(&stdout_text),
            text_tail(&stderr_text),
        ));
        return result;
    }
    let polar_is_fresh = polar_path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 100);
    if !polar_is_fresh {
        result.status = VspaeroProcessStatus::OutputMissing;
        result.error = Some(format!(
            "VSPAERO returned success without a fresh non-empty {}",
            polar_path.display()
        ));
        return result;
    }

    result.status = VspaeroProcessStatus::Completed;
    result.error = None;
    result
}

fn text_tail(text: &str) -> String {
    const MAX_LINES: usize = 20;
    const EMPTY_LOG: &str = "<empty>";

    let tail = text.lines().rev().take(MAX_LINES).collect::<Vec<_>>();
    if tail.is_empty() {
        return EMPTY_LOG.to_owned();
    }
    tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

fn vspaero_case_argument(case_path: &Path) -> &std::ffi::OsStr {
    case_path.file_name().unwrap_or(case_path.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_inputs_are_reported_before_launch() {
        let root = std::env::temp_dir().join(format!(
            "alas-vspaero-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let result = run_vspaero(Path::new("absent-vspaero"), &root.join("case"), 1, 1.0);
        assert_eq!(result.status, VspaeroProcessStatus::InputMissing);
        assert!(result.error.as_deref().is_some_and(|error| {
            error.contains("case.vspgeom")
                && error.contains("case.vkey")
                && error.contains("case.vspaero")
        }));
        assert_eq!(result.wake_settings, None);
        assert_eq!(result.wake_mode, None);
    }

    #[test]
    fn wake_setup_classifies_free_and_frozen_modes_without_claiming_convergence(
    ) -> Result<(), &'static str> {
        let free = parse_wake_settings(
            "WakeIters = 20\nFreezeWakeAtIteration = 10000\nImplicitWake = 0\nWakeRelax = 1\nForwardGMRESConvergenceFactor = 1\n",
        )
        .ok_or("complete native setup should be observable")?;
        assert_eq!(free.mode(), VspaeroWakeMode::FreeWake);
        assert_eq!(free.mode().as_str(), "free_wake");
        assert_eq!(free.initial_wake_alignment.as_str(), "initial_free_stream");
        assert!(!free.implicit_wake);

        let frozen = parse_wake_settings(
            "WakeIters = 20\nFreezeWakeAtIteration = 1\nImplicitWake = 0\nWakeRelax = 0.75\nForwardGMRESConvergenceFactor = 1\n",
        )
        .ok_or("complete frozen setup should be observable")?;
        assert_eq!(
            frozen.mode(),
            VspaeroWakeMode::FrozenWake { at_iteration: 1 }
        );
        assert_eq!(frozen.mode().as_str(), "frozen_wake");
        assert_eq!(frozen.wake_iterations, 20);
        assert_eq!(frozen.wake_relax, 0.75);
        Ok(())
    }

    #[test]
    fn incomplete_or_unsupported_wake_setup_remains_unknown() {
        let base = "WakeIters = 20\nFreezeWakeAtIteration = 1\nImplicitWake = 0\nWakeRelax = 1\nForwardGMRESConvergenceFactor = 1\n";
        assert!(parse_wake_settings(base).is_some());
        assert!(parse_wake_settings(
            "WakeIters = 20\nFreezeWakeAtIteration = 1\nImplicitWake = 2\nWakeRelax = 1\nForwardGMRESConvergenceFactor = 1\n"
        )
        .is_none());
        assert!(parse_wake_settings(
            "WakeIters = 20\nFreezeWakeAtIteration = 1\nImplicitWake = 0\nWakeRelax = nan\nForwardGMRESConvergenceFactor = 1\n"
        )
        .is_none());
        assert!(parse_wake_settings(
            "WakeIters = 20\nFreezeWakeAtIteration = 1\nImplicitWake = 0\nWakeRelax = 1\n"
        )
        .is_none());
        assert!(parse_wake_settings(
            "WakeIters = 18446744073709551616\nFreezeWakeAtIteration = 1\nImplicitWake = 0\nWakeRelax = 1\nForwardGMRESConvergenceFactor = 1\n"
        )
        .is_none());
    }

    #[test]
    fn process_status_text_is_stable() {
        assert_eq!(VspaeroProcessStatus::InputMissing.as_str(), "input_missing");
        assert_eq!(VspaeroProcessStatus::LaunchFailed.as_str(), "launch_failed");
        assert_eq!(VspaeroProcessStatus::TimedOut.as_str(), "timed_out");
        assert_eq!(VspaeroProcessStatus::SolverFailed.as_str(), "solver_failed");
        assert_eq!(
            VspaeroProcessStatus::OutputMissing.as_str(),
            "output_missing"
        );
        assert_eq!(VspaeroProcessStatus::Completed.as_str(), "completed");
    }

    #[test]
    fn diagnostic_tail_keeps_the_last_solver_lines_in_reading_order() {
        assert_eq!(
            text_tail("first\nsecond\nthird\n"),
            "first | second | third"
        );
        assert_eq!(text_tail(""), "<empty>");
    }

    #[test]
    fn relative_case_arguments_are_reduced_to_the_name_inside_the_workdir() {
        let case = Path::new("outputs/openvsp/optimized_aircraft");
        assert_eq!(
            vspaero_case_argument(case),
            std::ffi::OsStr::new("optimized_aircraft")
        );
        let absolute = Path::new(r"C:\tmp\optimized_aircraft");
        assert_eq!(
            vspaero_case_argument(absolute),
            std::ffi::OsStr::new("optimized_aircraft")
        );
    }
}
