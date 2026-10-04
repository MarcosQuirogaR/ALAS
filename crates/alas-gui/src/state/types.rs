// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small state types: languages, tabs, run-log lines, run options and worker messages.

use std::time::Duration;

use alas_pipeline::{PipelineResult, RunEvent};

#[derive(Debug, Clone)]
/// Shell state temporarily replaced while the walkthrough exposes its targets.
pub struct WalkthroughRestore {
    pub(crate) active_page: String,
    pub(crate) nav_pinned: bool,
    pub(crate) nav_hover_open: bool,
    pub(crate) preview_open: bool,
}

/// Supported user interface languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// English.
    En,
    /// Spanish.
    Es,
}

impl Language {
    /// The catalog code `alas_i18n` looks a translation up under.
    pub fn code(&self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// The two visibility modes of the unified aircraft viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTab {
    /// The complete aircraft exterior.
    Exterior,
    /// The interior cabin and payload cutaway.
    Cabin,
}

/// Camera identity shared by the exterior and interior viewer modes.
pub const AIRCRAFT_PREVIEW_CAMERA_ID: &str = "aircraft_3d";

/// One run-log line, coloured by severity in the log panel.
#[derive(Debug, Clone)]
pub struct LogLine {
    /// The message text.
    pub text: String,
    /// Its severity, which decides its colour.
    pub kind: LogKind,
    /// Time since the active run began, when this line belongs to a run.
    pub elapsed: Option<Duration>,
    /// Monotonic run identity, or zero for application-level messages.
    pub run_id: u64,
}

/// A run-log line's severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    /// An ordinary progress message.
    Info,
    /// A non-fatal warning.
    Warn,
    /// A failure.
    Error,
}

/// Full-width run-log view selected while no analysis is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunLogTab {
    /// Searchable diagnostic console.
    Console,
    /// Stage progress and elapsed durations.
    Timings,
}

/// The four per-run toggles the Inputs page offers.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Run the design-space optimizer (otherwise analyse the current design).
    pub optimize: bool,
    /// Analyse the baseline design alongside the optimized one.
    pub compare_baseline: bool,
    /// Write the CPACS aircraft and compatibility output files.
    pub write_outputs: bool,
    /// Run the downstream disciplines concurrently.
    pub parallel: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            optimize: true,
            compare_baseline: true,
            write_outputs: true,
            parallel: true,
        }
    }
}

/// Messages emitted from the background pipeline thread.
pub enum WorkerMessage {
    /// A typed pipeline lifecycle event.
    Event(RunEvent),
    /// A report snapshot whose figures are already safe to display while the
    /// downstream stages continue in the worker.
    Snapshot(Box<PipelineResult>),
    /// The finished result, or the error that ended the run.
    Finished(Box<Result<PipelineResult, String>>),
}
