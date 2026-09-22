// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native path selection for optional external tools without persisting files
//! beside the executable.
//!
//! The actual executable or directory remains an explicit preference. A picker
//! merely fills an empty path field; it never discovers a location and treats
//! it as a runnable solver.

use std::process::Command;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;

use crate::state::AppState;

/// The explicit preference a picker result should update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPathTarget {
    NastranExecutable,
    NastranSolver,
    Nastran95Directory,
    Nastran95RuntimeDirectory,
    Nastran95RfStageDirectory,
    PatranExecutable,
    MsesDirectory,
    NavdataDirectory,
    RoutesDirectory,
    AvlExecutable,
    OpenVspDirectory,
    FlowUnsteadyExecutable,
    OpenFoamNativeBinDirectory,
    OpenFoamNativeProjectDirectory,
    GmshExecutable,
    ParaViewExecutable,
}

/// A completed native selection, cancellation, or launcher error.
#[derive(Debug)]
pub struct PathSelection {
    /// The field that requested this picker.
    pub target: ToolPathTarget,
    /// A selected path, no path after cancellation, or an error from the
    /// platform launcher.
    pub result: Result<Option<String>, String>,
}

/// One picker process that is waiting independently of the GUI event loop.
#[derive(Debug)]
pub struct PathPicker {
    target: ToolPathTarget,
    receiver: Receiver<Result<Option<String>, String>>,
}

impl AppState {
    /// Start one non-blocking native picker when no other selection is open.
    pub(crate) fn begin_path_picker(&mut self, target: ToolPathTarget, directory: bool) {
        if self.path_picker.is_some() {
            return;
        }
        self.path_picker = Some(PathPicker {
            target,
            receiver: spawn_path_picker(directory),
        });
    }

    /// Whether this field's native picker remains open.
    pub(crate) fn path_picker_pending(&self, target: ToolPathTarget) -> bool {
        self.path_picker
            .as_ref()
            .is_some_and(|picker| picker.target == target)
    }

    /// Return a completed result once without blocking the desktop thread.
    pub(crate) fn take_path_selection(&mut self) -> Option<PathSelection> {
        let picker = self.path_picker.as_ref()?;
        let result = match picker.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err("The native path picker closed before returning a selection.".to_owned())
            }
        };
        let target = picker.target;
        self.path_picker = None;
        Some(PathSelection { target, result })
    }
}

fn spawn_path_picker(directory: bool) -> Receiver<Result<Option<String>, String>> {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        let _ = sender.send(native_path_picker(directory));
    });
    receiver
}

#[cfg(target_os = "windows")]
fn native_path_picker(directory: bool) -> Result<Option<String>, String> {
    // The script is constant: user input is never interpolated into a shell
    // command. `-STA` is required by Windows Forms dialogs and the worker
    // keeps the modal dialog off eframe's render thread.
    let script = if directory {
        r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'Select the external-tool installation directory'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.SelectedPath)
}
"#
    } else {
        r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Filter = 'Executable files (*.exe)|*.exe|All files (*.*)|*.*'
$dialog.Multiselect = $false
$dialog.CheckFileExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.FileName)
}
"#
    };
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!selected.is_empty()).then_some(selected))
}

#[cfg(target_os = "macos")]
fn native_path_picker(directory: bool) -> Result<Option<String>, String> {
    let script = if directory {
        "POSIX path of (choose folder with prompt \"Select the external-tool installation directory\")"
    } else {
        "POSIX path of (choose file with prompt \"Select an external-tool executable\")"
    };
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!selected.is_empty()).then_some(selected))
}

/// Native dialog programs probed on Linux/other Unix desktops, in the fixed
/// order they are tried. `zenity` ships with GNOME; `kdialog` is the KDE
/// equivalent. Neither is installed on every minimal or headless distribution.
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const UNIX_DIALOG_BACKENDS: [&str; 2] = ["zenity", "kdialog"];

/// The message shown when no supported picker binary is on `PATH`. The text
/// field a picker fills remains usable without it.
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const UNIX_DIALOG_MISSING_MESSAGE: &str =
    "No native file picker was found (looked for zenity, kdialog). Install one of them, or type the path directly in the field.";

/// The first backend from [`UNIX_DIALOG_BACKENDS`] whose executable exists in
/// one of `path_var`'s directories, checked without touching process state so
/// tests can supply a synthetic `PATH` value.
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn resolve_unix_dialog_backend(path_var: &str) -> Option<&'static str> {
    UNIX_DIALOG_BACKENDS
        .iter()
        .copied()
        .find(|candidate| std::env::split_paths(path_var).any(|dir| dir.join(candidate).is_file()))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn native_path_picker(directory: bool) -> Result<Option<String>, String> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let backend = resolve_unix_dialog_backend(&path_var)
        .ok_or_else(|| UNIX_DIALOG_MISSING_MESSAGE.to_owned())?;
    let mut command = Command::new(backend);
    if backend == "kdialog" {
        command.arg(if directory {
            "--getexistingdirectory"
        } else {
            "--getopenfilename"
        });
        command.arg(".");
    } else {
        command.arg("--file-selection");
        if directory {
            command.arg("--directory");
        }
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!selected.is_empty()).then_some(selected))
}

#[cfg(all(test, not(target_os = "windows"), not(target_os = "macos")))]
mod unix_dialog_backend_tests {
    use super::{resolve_unix_dialog_backend, UNIX_DIALOG_MISSING_MESSAGE};
    use std::fs;

    /// An isolated directory standing in for one `PATH` entry, holding zero
    /// or more fake backend "binaries" (empty files; only presence matters).
    struct FakeBinDir {
        path: std::path::PathBuf,
    }

    impl FakeBinDir {
        fn with_binaries(names: &[&str]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "alas-gui-path-picker-test-{}-{:?}",
                std::process::id(),
                std::time::Instant::now()
            ));
            fs::create_dir_all(&path).expect("create fake PATH directory");
            for name in names {
                fs::write(path.join(name), b"").expect("create fake backend binary");
            }
            Self { path }
        }

        fn path_var(&self) -> String {
            self.path.display().to_string()
        }
    }

    impl Drop for FakeBinDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn reports_the_actionable_error_when_neither_backend_is_on_path() {
        let bin_dir = FakeBinDir::with_binaries(&[]);
        assert_eq!(resolve_unix_dialog_backend(&bin_dir.path_var()), None);
        assert!(UNIX_DIALOG_MISSING_MESSAGE.contains("type the path directly"));
    }

    #[test]
    fn finds_kdialog_when_zenity_is_absent() {
        let bin_dir = FakeBinDir::with_binaries(&["kdialog"]);
        assert_eq!(
            resolve_unix_dialog_backend(&bin_dir.path_var()),
            Some("kdialog")
        );
    }

    #[test]
    fn prefers_zenity_over_kdialog_when_both_are_present() {
        let bin_dir = FakeBinDir::with_binaries(&["kdialog", "zenity"]);
        assert_eq!(
            resolve_unix_dialog_backend(&bin_dir.path_var()),
            Some("zenity")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::ToolPathTarget;

    #[test]
    fn picker_targets_are_distinct_for_each_explicit_preference() {
        assert_ne!(
            ToolPathTarget::NastranExecutable,
            ToolPathTarget::NastranSolver
        );
        assert_ne!(
            ToolPathTarget::Nastran95Directory,
            ToolPathTarget::Nastran95RfStageDirectory
        );
        assert_ne!(
            ToolPathTarget::MsesDirectory,
            ToolPathTarget::OpenVspDirectory
        );
        assert_ne!(ToolPathTarget::MsesDirectory, ToolPathTarget::AvlExecutable);
        assert_ne!(
            ToolPathTarget::NavdataDirectory,
            ToolPathTarget::RoutesDirectory
        );
        assert_ne!(
            ToolPathTarget::FlowUnsteadyExecutable,
            ToolPathTarget::AvlExecutable
        );
        assert_ne!(
            ToolPathTarget::FlowUnsteadyExecutable,
            ToolPathTarget::OpenVspDirectory
        );
    }
}
