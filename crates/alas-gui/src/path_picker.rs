// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native path selection (through `rfd`) for optional external tools without persisting files
//! beside the executable.
//!
//! The actual executable or directory remains an explicit preference. A picker
//! merely fills an empty path field; it never discovers a location and treats
//! it as a runnable solver.

use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;

use crate::state::AppState;

/// The explicit preference a picker result should update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPathTarget {
    /// Installed NASTRAN executable.
    NastranExecutable,
    /// NASTRAN solver binary used for runs.
    NastranSolver,
    /// MSC NASTRAN 95 installation directory.
    Nastran95Directory,
    /// NASTRAN 95 runtime directory.
    Nastran95RuntimeDirectory,
    /// NASTRAN 95 RF-stage directory.
    Nastran95RfStageDirectory,
    /// MSC Patran executable.
    PatranExecutable,
    /// MSES installation directory.
    MsesDirectory,
    /// Navigation data directory.
    NavdataDirectory,
    /// Saved routes directory.
    RoutesDirectory,
    /// AVL executable.
    AvlExecutable,
    /// OpenVSP installation directory.
    OpenVspDirectory,
    /// FlowUnsteady executable.
    FlowUnsteadyExecutable,
    /// Native OpenFOAM binary directory.
    OpenFoamNativeBinDirectory,
    /// Native OpenFOAM project directory.
    OpenFoamNativeProjectDirectory,
    /// Gmsh executable.
    GmshExecutable,
    /// ParaView executable.
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

/// Show the platform dialog through `rfd` (Win32 common dialogs on Windows,
/// the XDG desktop portal on Linux, Cocoa on macOS). Cancelling is `Ok(None)`.
fn native_path_picker(directory: bool) -> Result<Option<String>, String> {
    let dialog = rfd::FileDialog::new();
    let selected = if directory {
        dialog
            .set_title("Select the external-tool installation directory")
            .pick_folder()
    } else {
        let dialog = dialog.set_title("Select an external-tool executable");
        #[cfg(target_os = "windows")]
        let dialog = dialog.add_filter("Executable files", &["exe"]);
        dialog.pick_file()
    };
    Ok(selected.map(|path| path.display().to_string()))
}
