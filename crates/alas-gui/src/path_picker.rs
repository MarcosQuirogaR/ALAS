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

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn native_path_picker(directory: bool) -> Result<Option<String>, String> {
    let mut command = Command::new("zenity");
    command.arg("--file-selection");
    if directory {
        command.arg("--directory");
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!selected.is_empty()).then_some(selected))
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
    }
}
