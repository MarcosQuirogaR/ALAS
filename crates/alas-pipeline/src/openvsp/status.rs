// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The evidence state an OpenVSP export reports, and its stable spelling.

/// Evidence state of an OpenVSP export artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenVspExportStatus {
    /// The supported input script was written, but no installed runtime read it.
    ScriptWrittenRuntimeUnverified,
    /// The configured executable could not be launched.
    RuntimeLaunchFailed,
    /// The OpenVSP process exceeded its deadline and its process tree was killed.
    RuntimeTimedOut,
    /// The configured timeout was not a finite number of seconds greater than
    /// zero; OpenVSP was not launched.
    InvalidTimeout,
    /// OpenVSP ran, but did not satisfy the completion and native-file checks.
    RuntimeRejected,
    /// OpenVSP read the script and wrote the requested project file.
    Vsp3Materialized,
}

impl OpenVspExportStatus {
    /// Stable status text for the GUI and retained run evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScriptWrittenRuntimeUnverified => "script_written_runtime_unverified",
            Self::RuntimeLaunchFailed => "runtime_launch_failed",
            Self::RuntimeTimedOut => "runtime_timed_out",
            Self::InvalidTimeout => "invalid_timeout",
            Self::RuntimeRejected => "runtime_rejected",
            Self::Vsp3Materialized => "vsp3_materialized",
        }
    }
}
