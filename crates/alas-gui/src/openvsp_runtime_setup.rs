// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Background worker for the optional OpenVSP preview-runtime install button
//! on the OpenVSP Tools card.
//!
//! This module never re-implements `tools/setup_openvsp_preview.ps1`'s hash
//! pinning, safe zip extraction, or atomic staging-then-move install: it
//! shells out to that unmodified script from a background thread and streams
//! its output, the same way [`crate::path_picker`] shells out to a native
//! dialog. That keeps the Rust side and the script's safety properties from
//! silently diverging, and avoids adding a zip-reading dependency for logic
//! the script already gets right. The manual, command-line script remains
//! fully supported; this module only gives it a GUI front end.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use alas_exec::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};

/// Location of the maintained setup script, relative to the repository or
/// packaged-application root. Never edited by this module: only its literal
/// pinned hashes and URLs are off limits, and this module never even reads
/// them, since the script itself is the one thing that has to stay
/// authoritative.
const SCRIPT_RELATIVE_PATH: &str = "tools/setup_openvsp_preview.ps1";

/// The script's own default destination when no OpenVSP install directory is
/// configured yet, matching `tools/setup_openvsp_preview.ps1`'s `$Destination`
/// default. Kept here only as a fallback so the status row and the install
/// button always agree on where they are looking.
const DEFAULT_RELATIVE_DESTINATION: &str = "external tools/OpenVSP-3.51.2-win64/preview-runtime";

/// Whether this platform can run the preview-runtime installer at all.
///
/// The script itself refuses anything but 64-bit Windows
/// (`tools/setup_openvsp_preview.ps1:20-22`); this mirrors that check so the
/// Tools card can hide or disable the actionable control instead of showing
/// a button that would only fail.
pub fn install_supported() -> bool {
    cfg!(target_os = "windows") && cfg!(target_pointer_width = "64")
}

/// The preview runtime's resolved availability, read from `runtime-manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewRuntimeStatus {
    /// This platform cannot run the installer or the runtime it produces.
    Unavailable,
    /// No usable runtime manifest was found at the resolved destination.
    NotInstalled,
    /// A runtime manifest was found and parsed.
    Installed {
        /// The pinned embedded-Python version the manifest recorded.
        python: String,
        /// The pinned OpenVSP Python-bindings version the manifest recorded.
        openvsp: String,
        /// The pinned NumPy wheel version the manifest recorded.
        numpy: String,
    },
}

impl PreviewRuntimeStatus {
    /// A one-line, already-translatable status fragment for the Tools card.
    pub fn label(&self) -> String {
        match self {
            PreviewRuntimeStatus::Unavailable => {
                "unavailable — this platform is not 64-bit Windows".to_owned()
            }
            PreviewRuntimeStatus::NotInstalled => "not installed".to_owned(),
            PreviewRuntimeStatus::Installed {
                python,
                openvsp,
                numpy,
            } => format!("installed (Python {python}, OpenVSP {openvsp}, NumPy {numpy})"),
        }
    }
}

/// The directory the preview runtime must land in.
///
/// `crates/alas-pipeline/src/openvsp/native_preview.rs` looks for the runtime
/// beside the resolved `vspscript.exe`, in `preview-runtime/python.exe`, so an
/// install that ignores the configured OpenVSP directory would assemble a
/// runtime the pipeline can never find. `openvsp_dir` is the same
/// `state.tool_preferences.openvsp_dir` the OpenVSP card's own install-
/// directory field already edits. An unconfigured or blank directory falls
/// back to the script's own default, which matches the bundled dev-checkout
/// layout (`external tools/OpenVSP-3.51.2-win64`).
pub fn resolve_destination(openvsp_dir: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = openvsp_dir.map(str::trim).filter(|dir| !dir.is_empty()) {
        return Some(Path::new(dir).join("preview-runtime"));
    }
    locate_repo_root().map(|root| root.join(DEFAULT_RELATIVE_DESTINATION))
}

/// Read the resolved runtime status from `destination`'s manifest, if any.
pub fn runtime_status(destination: Option<&Path>) -> PreviewRuntimeStatus {
    if !install_supported() {
        return PreviewRuntimeStatus::Unavailable;
    }
    let Some(destination) = destination else {
        return PreviewRuntimeStatus::NotInstalled;
    };
    match read_manifest(&destination.join("runtime-manifest.json")) {
        Some(manifest) => PreviewRuntimeStatus::Installed {
            python: manifest.python,
            openvsp: manifest.openvsp,
            numpy: manifest.numpy,
        },
        None => PreviewRuntimeStatus::NotInstalled,
    }
}

struct PreviewManifest {
    python: String,
    openvsp: String,
    numpy: String,
}

fn read_manifest(path: &Path) -> Option<PreviewManifest> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    Some(PreviewManifest {
        python: value.get("Python")?.as_str()?.to_owned(),
        openvsp: value.get("OpenVSP")?.as_str()?.to_owned(),
        numpy: value.get("NumPy")?.as_str()?.to_owned(),
    })
}

/// Walk up from the running executable looking for the setup script, the
/// same bounded, marker-stopped walk `alas-exec`'s adjacent-tool discovery
/// already uses for `external tools/`: it stops at the first `Cargo.toml` it
/// finds, so a development checkout resolves immediately while a packaged
/// install without the script simply reports "not found" instead of
/// searching the whole filesystem.
fn locate_repo_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut current = exe.parent();
    while let Some(dir) = current {
        if dir.join(SCRIPT_RELATIVE_PATH).is_file() {
            return Some(dir.to_path_buf());
        }
        if dir.join("Cargo.toml").is_file() {
            return None;
        }
        current = dir.parent();
    }
    None
}

fn locate_setup_script() -> Option<PathBuf> {
    locate_repo_root().map(|root| root.join(SCRIPT_RELATIVE_PATH))
}

/// Terminal outcome of one install attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewInstallEvent {
    /// The script exited successfully.
    Completed,
    /// The signal from [`OpenVspRuntimeSetup::cancel`] stopped the script
    /// before it finished. Matches the script's own "failed setup retains
    /// staging files for inspection, does not touch the destination"
    /// contract: nothing already installed is touched.
    Cancelled,
    /// The script exited with a non-zero status; the message is its own
    /// diagnostic text (which archive, expected vs. actual hash, or archive-
    /// layout error), not a re-derived one.
    Failed(String),
}

enum WorkerMessage {
    Stage(String),
    Finished(PreviewInstallEvent),
}

/// Run the setup script to completion or cancellation, streaming every
/// non-empty output line as a stage message.
///
/// `script` and `destination` are explicit parameters, not resolved inside
/// this function, so a test can point it at a local fixture script instead
/// of the real network-fetching one.
fn run_worker(
    script: PathBuf,
    destination: Option<PathBuf>,
    force: bool,
    sender: Sender<WorkerMessage>,
    child_pid: Arc<AtomicU32>,
    cancel_flag: Arc<AtomicBool>,
) {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]);
    command.arg(&script);
    if let Some(destination) = &destination {
        command.arg("-Destination").arg(destination);
    }
    if force {
        // Always passed: this is the only "install" affordance in the GUI,
        // so it doubles as reinstall/repair. The script only reads -Force
        // when the destination already exists, and even then it preserves
        // the previous runtime as a timestamped backup rather than deleting
        // it, so this never trades away the atomic-install/no-deletion
        // guarantee.
        command.arg("-Force");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = sender.send(WorkerMessage::Finished(PreviewInstallEvent::Failed(
                format!("cannot launch the OpenVSP preview-runtime setup script: {error}"),
            )));
            return;
        }
    };
    child_pid.store(child.id(), Ordering::Relaxed);

    // Both streams are drained concurrently: PowerShell's exact routing of
    // `Write-Host` versus a thrown error's message between the process's
    // stdout and stderr is a host-configuration detail this module should
    // not have to depend on, so lines from either stream are forwarded as
    // stage messages, and both are kept for the final diagnostic on failure.
    let stderr_sender = sender.clone();
    let stderr_handle = child
        .stderr
        .take()
        .map(|pipe| thread::spawn(move || stream_lines(pipe, &stderr_sender)));
    let stdout_text = child
        .stdout
        .take()
        .map(|pipe| stream_lines(pipe, &sender))
        .unwrap_or_default();
    let status = child.wait();
    child_pid.store(0, Ordering::Relaxed);
    let stderr_text = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    let outcome = if cancel_flag.load(Ordering::Relaxed) {
        PreviewInstallEvent::Cancelled
    } else {
        match status {
            Ok(status) if status.success() => PreviewInstallEvent::Completed,
            Ok(_) => {
                let diagnostic = if stderr_text.trim().is_empty() {
                    stdout_text
                } else {
                    stderr_text
                };
                PreviewInstallEvent::Failed(cap_diagnostic(&diagnostic))
            }
            Err(error) => PreviewInstallEvent::Failed(format!(
                "cannot wait on the setup script process: {error}"
            )),
        }
    };
    let _ = sender.send(WorkerMessage::Finished(outcome));
}

fn stream_lines<R: std::io::Read>(reader: R, sender: &Sender<WorkerMessage>) -> String {
    let mut full = String::new();
    for line in std::io::BufReader::new(reader)
        .lines()
        .map_while(Result::ok)
    {
        let trimmed = line.trim().to_owned();
        if trimmed.is_empty() {
            continue;
        }
        full.push_str(&trimmed);
        full.push('\n');
        let _ = sender.send(WorkerMessage::Stage(trimmed));
    }
    full
}

/// Keep only the last, most actionable lines of a diagnostic: the script's
/// thrown message is always its final output, while earlier lines are
/// ordinary "Downloading..." progress noise.
fn cap_diagnostic(text: &str) -> String {
    const MAX_LINES: usize = 20;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "the setup script exited without diagnostic output".to_owned();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let tail = if lines.len() > MAX_LINES {
        &lines[lines.len() - MAX_LINES..]
    } else {
        &lines[..]
    };
    tail.join("\n")
}

/// The OpenVSP preview-runtime install button's background-run state.
pub struct OpenVspRuntimeSetup {
    /// Whether an install is currently running.
    pub running: bool,
    cancel_requested: bool,
    /// The most recent stage message, shown next to the button while running.
    pub stage: String,
    /// The diagnostic from the most recent failed install, if any. Cleared
    /// at the start of the next attempt.
    pub error: Option<String>,
    rx: Option<Receiver<WorkerMessage>>,
    child_pid: Arc<AtomicU32>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for OpenVspRuntimeSetup {
    fn default() -> Self {
        Self {
            running: false,
            cancel_requested: false,
            stage: String::new(),
            error: None,
            rx: None,
            child_pid: Arc::new(AtomicU32::new(0)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl OpenVspRuntimeSetup {
    /// Start the background install unless one is already running.
    ///
    /// `destination` should come from [`resolve_destination`]; passing `None`
    /// lets the script fall back to its own default. Does nothing beyond
    /// recording an inline error when this platform cannot run the installer
    /// or the setup script cannot be located.
    pub fn install(&mut self, destination: Option<PathBuf>, force: bool) {
        if self.running || !install_supported() {
            return;
        }
        let Some(script) = locate_setup_script() else {
            self.error = Some(format!(
                "Setup script not found at {SCRIPT_RELATIVE_PATH} beside the application. Run it manually from a source checkout (see docs/openvsp-preview.md)."
            ));
            return;
        };
        self.running = true;
        self.cancel_requested = false;
        self.error = None;
        self.stage = "Starting...".to_owned();
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.child_pid.store(0, Ordering::Relaxed);
        let (sender, receiver) = channel();
        self.rx = Some(receiver);
        let cancel_flag = Arc::clone(&self.cancel_flag);
        let child_pid = Arc::clone(&self.child_pid);
        thread::spawn(move || {
            run_worker(script, destination, force, sender, child_pid, cancel_flag);
        });
    }

    /// Ask a running install to stop before it moves any staged files into
    /// the destination.
    ///
    /// Kills the whole script process tree rather than only the immediate
    /// `powershell.exe`: an in-flight `Invoke-WebRequest` or archive
    /// extraction must not be left running, and its result must never reach
    /// the final `Move-Item`. The script only ever moves the destination
    /// after every archive is downloaded, verified, and extracted, so a kill
    /// during any earlier stage leaves the previous destination, if any,
    /// exactly as it was.
    pub fn cancel(&mut self) {
        if !self.running {
            return;
        }
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.cancel_requested = true;
        self.stage = "Cancelling...".to_owned();
        let pid = self.child_pid.load(Ordering::Relaxed);
        if pid != 0 {
            kill_process_tree(pid);
        }
    }

    /// Whether a cancellation has been requested and the worker has not yet
    /// reported its terminal outcome.
    pub fn is_cancelling(&self) -> bool {
        self.running && self.cancel_requested
    }

    /// Drain stage messages and pick up the terminal outcome, if any, without
    /// blocking the desktop thread. Returns the outcome only on the frame it
    /// arrived, so a caller can log it exactly once.
    pub fn poll(&mut self) -> Option<PreviewInstallEvent> {
        let mut last_stage = None;
        let mut finished = None;
        if let Some(rx) = &self.rx {
            while let Ok(message) = rx.try_recv() {
                match message {
                    WorkerMessage::Stage(line) => last_stage = Some(line),
                    WorkerMessage::Finished(event) => finished = Some(event),
                }
            }
        }
        if let Some(stage) = last_stage {
            self.stage = stage;
        }
        let Some(event) = finished else {
            return None;
        };
        self.running = false;
        self.cancel_requested = false;
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.child_pid.store(0, Ordering::Relaxed);
        self.rx = None;
        match &event {
            PreviewInstallEvent::Completed => self.stage = "Installed.".to_owned(),
            PreviewInstallEvent::Cancelled => self.stage = "Cancelled.".to_owned(),
            PreviewInstallEvent::Failed(error) => {
                self.stage.clear();
                self.error = Some(error.clone());
            }
        }
        Some(event)
    }
}

#[cfg(test)]
mod status_tests {
    use super::{install_supported, locate_repo_root, resolve_destination, runtime_status};
    use super::{PreviewRuntimeStatus, DEFAULT_RELATIVE_DESTINATION};
    use std::path::Path;

    #[test]
    fn install_is_reported_supported_only_on_64_bit_windows() {
        assert_eq!(
            install_supported(),
            cfg!(target_os = "windows") && cfg!(target_pointer_width = "64")
        );
    }

    #[test]
    fn the_status_is_unavailable_whenever_the_platform_cannot_run_the_installer() {
        let status = runtime_status(None);
        if install_supported() {
            assert_eq!(status, PreviewRuntimeStatus::NotInstalled);
        } else {
            assert_eq!(status, PreviewRuntimeStatus::Unavailable);
        }
    }

    #[test]
    fn resolve_destination_prefers_a_configured_openvsp_directory() {
        let destination = resolve_destination(Some("C:/OpenVSP-test"));
        assert_eq!(
            destination,
            Some(Path::new("C:/OpenVSP-test").join("preview-runtime"))
        );
    }

    #[test]
    fn resolve_destination_ignores_a_blank_configured_directory_and_falls_back() {
        assert_eq!(
            resolve_destination(Some("   ")),
            locate_repo_root().map(|root| root.join(DEFAULT_RELATIVE_DESTINATION))
        );
    }
}

// The Windows fixture scripts below spawn a real `powershell.exe`, so a
// failed expect is the test host missing that binary, not a library
// invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(target_os = "windows")]
#[cfg(test)]
mod worker_tests {
    use super::{
        cap_diagnostic, kill_process_tree, run_worker, runtime_status, PreviewInstallEvent,
        PreviewRuntimeStatus, WorkerMessage,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    /// A directory name unique to this process and call, without going
    /// through `Instant`'s `Debug` formatting: on Windows, that format
    /// (`Instant { t: 123.456s }`) embeds a colon, which NTFS reads as an
    /// Alternate-Data-Stream separator inside a path component, so
    /// `fs::create_dir_all` on the raw formatted string failed with
    /// `ERROR_DIRECTORY` (267) instead of creating a directory.
    fn unique_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "alas-openvsp-runtime-setup-test-{label}-{}-{nanos}-{sequence}",
            std::process::id(),
        ))
    }

    fn write_fixture_script(directory: &Path, body: &str) -> PathBuf {
        fs::create_dir_all(directory).expect("create fixture script directory");
        let script = directory.join("fixture.ps1");
        fs::write(&script, body).expect("write fixture PowerShell script");
        script
    }

    #[test]
    fn a_successful_fixture_script_reports_completed_and_streams_its_stage_lines() {
        let script_dir = unique_dir("success-script");
        let script = write_fixture_script(
            &script_dir,
            r#"
param([string]$Destination, [switch]$Force)
Write-Host "Downloading fixture-archive.zip..."
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Set-Content -LiteralPath (Join-Path $Destination 'runtime-manifest.json') '{"Python":"3.13.7","OpenVSP":"3.51.2","NumPy":"2.3.3"}'
Write-Host "OpenVSP preview runtime installed at $Destination"
"#,
        );
        let destination = unique_dir("success-dest");
        let (sender, receiver) = channel();
        let child_pid = Arc::new(AtomicU32::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));

        run_worker(
            script,
            Some(destination.clone()),
            false,
            sender,
            child_pid,
            cancel_flag,
        );

        let mut stages = Vec::new();
        let mut outcome = None;
        while let Ok(message) = receiver.try_recv() {
            match message {
                WorkerMessage::Stage(line) => stages.push(line),
                WorkerMessage::Finished(event) => outcome = Some(event),
            }
        }
        assert_eq!(outcome, Some(PreviewInstallEvent::Completed));
        assert!(
            stages.iter().any(|line| line.contains("Downloading")),
            "{stages:?}"
        );
        assert_eq!(
            runtime_status(Some(&destination)),
            PreviewRuntimeStatus::Installed {
                python: "3.13.7".to_owned(),
                openvsp: "3.51.2".to_owned(),
                numpy: "2.3.3".to_owned(),
            }
        );

        let _ = fs::remove_dir_all(&script_dir);
        let _ = fs::remove_dir_all(&destination);
    }

    #[test]
    fn a_failing_fixture_script_reports_its_thrown_message_and_touches_nothing() {
        let script_dir = unique_dir("failure-script");
        let script = write_fixture_script(
            &script_dir,
            r#"
param([string]$Destination, [switch]$Force)
throw "SHA-256 mismatch for fixture-archive.zip. Expected AAA, got BBB."
"#,
        );
        let destination = unique_dir("failure-dest");
        let (sender, receiver) = channel();
        let child_pid = Arc::new(AtomicU32::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));

        run_worker(
            script,
            Some(destination.clone()),
            false,
            sender,
            child_pid,
            cancel_flag,
        );

        let mut outcome = None;
        while let Ok(message) = receiver.try_recv() {
            if let WorkerMessage::Finished(event) = message {
                outcome = Some(event);
            }
        }
        let Some(PreviewInstallEvent::Failed(message)) = outcome else {
            panic!("expected a Failed outcome, got {outcome:?}");
        };
        assert!(message.contains("SHA-256 mismatch"), "{message}");
        assert!(!destination.exists());

        let _ = fs::remove_dir_all(&script_dir);
    }

    #[test]
    fn cancelling_a_slow_fixture_script_stops_it_before_the_destination_is_touched() {
        let script_dir = unique_dir("slow-script");
        let script = write_fixture_script(
            &script_dir,
            r#"
param([string]$Destination, [switch]$Force)
Write-Host "Downloading fixture-archive.zip..."
Start-Sleep -Seconds 30
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
"#,
        );
        let destination = unique_dir("cancel-dest");
        let (sender, receiver) = channel();
        let child_pid = Arc::new(AtomicU32::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_pid = Arc::clone(&child_pid);
        let worker_cancel = Arc::clone(&cancel_flag);
        let worker_destination = destination.clone();
        let handle = thread::spawn(move || {
            run_worker(
                script,
                Some(worker_destination),
                false,
                sender,
                worker_pid,
                worker_cancel,
            );
        });

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut pid = 0;
        while Instant::now() < deadline {
            pid = child_pid.load(Ordering::Relaxed);
            if pid != 0 {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_ne!(
            pid, 0,
            "the fixture script must have started within the test deadline"
        );

        cancel_flag.store(true, Ordering::Relaxed);
        kill_process_tree(pid);
        handle.join().expect("the worker thread must not panic");

        let mut outcome = None;
        while let Ok(message) = receiver.try_recv() {
            if let WorkerMessage::Finished(event) = message {
                outcome = Some(event);
            }
        }
        assert_eq!(outcome, Some(PreviewInstallEvent::Cancelled));
        assert!(
            !destination.exists(),
            "a cancelled install must never create the destination directory"
        );

        let _ = fs::remove_dir_all(&script_dir);
    }

    #[test]
    fn cap_diagnostic_keeps_the_final_thrown_message_over_earlier_progress_noise() {
        let mut lines = (0..30)
            .map(|n| format!("progress line {n}"))
            .collect::<Vec<_>>();
        lines.push("SHA-256 mismatch for archive.zip".to_owned());
        let text = lines.join("\n");

        let capped = cap_diagnostic(&text);

        assert!(capped.contains("SHA-256 mismatch"));
        assert!(!capped.contains("progress line 0"));
    }

    #[test]
    fn cap_diagnostic_reports_when_there_is_no_output_at_all() {
        assert_eq!(
            cap_diagnostic("   "),
            "the setup script exited without diagnostic output"
        );
    }
}
