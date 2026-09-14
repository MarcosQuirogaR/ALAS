// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Desktop state and worker boundary for the standalone Airfoil CFD study.
//!
//! The CFD crate owns the case contract and OpenFOAM runner.  This module owns
//! only desktop concerns: the selected library section, persistence, window
//! state, cancellation, and the revision gate that prevents a late worker
//! result from replacing a result for newer inputs.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use alas_cfd::{CfdOutcome, CfdResults, CfdRunEvent, CfdStage, CfdStudyConfig, OperatingInput};
use alas_exec::openfoam::{OpenFoamCapabilities, OpenFoamPreferences};

#[path = "cfd_parts/mod.rs"]
mod cfd_parts;

/// Process-local suffix for case directories.  The timestamp and process id
/// make directories unique across application launches; the counter closes
/// the race when multiple studies start in the same clock tick.
static NEXT_CASE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// The visible tabs in the standalone Airfoil CFD window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfdTab {
    /// Routine flow, geometry, and boundary controls.
    Study,
    /// Mesh, solver, and resource controls.
    Advanced,
    /// Actual coefficients, histories, fields, and quality evidence.
    Results,
    /// Captured lifecycle messages.
    Log,
}

impl Default for CfdTab {
    fn default() -> Self {
        Self::Study
    }
}

/// Axis varied by a sequential CFD sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CfdSweepVariable {
    /// Vary geometric angle of attack while retaining the selected operating
    /// input (speed or Reynolds number).
    AngleOfAttack,
    /// Vary chord Reynolds number and derive speed for every point.
    Reynolds,
}

impl Default for CfdSweepVariable {
    fn default() -> Self {
        Self::AngleOfAttack
    }
}

impl CfdSweepVariable {
    /// Stable, translatable label for the sweep editor.
    pub fn label(self) -> &'static str {
        match self {
            Self::AngleOfAttack => "Angle of attack",
            Self::Reynolds => "Reynolds number",
        }
    }

    /// Unit shown beside a point value.
    pub fn unit(self) -> &'static str {
        match self {
            Self::AngleOfAttack => "deg",
            Self::Reynolds => "-",
        }
    }
}

/// Persistent one-dimensional AoA/Reynolds sweep definition.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CfdSweepSettings {
    /// Independent axis varied in order.
    pub variable: CfdSweepVariable,
    /// First value in the sweep sequence.
    pub start: f64,
    /// Last value in the sweep sequence; it is included when valid.
    pub end: f64,
    /// Positive spacing between points.
    pub step: f64,
}

impl Default for CfdSweepSettings {
    fn default() -> Self {
        Self {
            variable: CfdSweepVariable::AngleOfAttack,
            start: -4.0,
            end: 12.0,
            step: 2.0,
        }
    }
}

impl CfdSweepSettings {
    /// Validate and expand this definition into the exact point values.
    pub fn values(&self) -> Result<Vec<f64>, String> {
        if !self.start.is_finite() || !self.end.is_finite() || !self.step.is_finite() {
            return Err("Sweep start, end and step must be finite.".to_owned());
        }
        if self.step <= 0.0 {
            return Err("Sweep step must be greater than zero.".to_owned());
        }
        let (lower, upper) = match self.variable {
            CfdSweepVariable::AngleOfAttack => (-30.0, 30.0),
            CfdSweepVariable::Reynolds => (1.0e3, 1.0e9),
        };
        if !(lower..=upper).contains(&self.start) || !(lower..=upper).contains(&self.end) {
            return Err(format!(
                "{} sweep values must be within [{lower}, {upper}].",
                self.variable.label()
            ));
        }
        if (self.end - self.start).abs() < f64::EPSILON {
            return Ok(vec![self.start]);
        }
        let direction = if self.end > self.start { 1.0 } else { -1.0 };
        let span = (self.end - self.start).abs();
        let estimated = (span / self.step).ceil() as usize + 1;
        if estimated > 51 {
            return Err("A sweep is limited to 51 sequential CFD cases.".to_owned());
        }
        let mut values = Vec::with_capacity(estimated);
        let mut value = self.start;
        for _ in 0..51 {
            values.push(value);
            let next = value + direction * self.step;
            if (self.end - value) * direction <= self.step {
                if (values.last().copied().unwrap_or(value) - self.end).abs() > 1e-10 {
                    values.push(self.end);
                }
                break;
            }
            value = next;
        }
        if values.len() > 51 {
            return Err("A sweep is limited to 51 sequential CFD cases.".to_owned());
        }
        Ok(values)
    }

    /// Apply one sweep point to a cloned base study contract.
    pub fn config_for_value(&self, base: &CfdStudyConfig, value: f64) -> CfdStudyConfig {
        let mut config = base.clone();
        match self.variable {
            CfdSweepVariable::AngleOfAttack => config.angle_of_attack_deg = value,
            CfdSweepVariable::Reynolds => {
                config.operating_input = OperatingInput::Reynolds;
                config.reynolds = value;
            }
        }
        config
    }
}

/// Status of one point in a sequential sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CfdSweepPointStatus {
    /// Worker has not started the point.
    Pending,
    /// Geometry/mesh/solver work is active for this point.
    Running,
    /// The case completed with the supplied numerical classification.
    Finished(CfdOutcome),
    /// The case could not produce a result.
    Failed,
    /// The point was stopped by the user.
    Cancelled,
}

impl CfdSweepPointStatus {
    /// Compact user-facing status label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Finished(CfdOutcome::NumericallyConverged) => "Numerically converged",
            Self::Finished(CfdOutcome::Unconverged) => "Unconverged",
            Self::Finished(CfdOutcome::Cancelled) => "Cancelled",
            Self::Finished(CfdOutcome::Failed) | Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// Per-point sweep provenance and actual result evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CfdSweepPointResult {
    /// Zero-based sequence index.
    pub index: usize,
    /// Value on the selected sweep axis.
    pub value: f64,
    /// Exact point contract submitted to the worker.
    pub config: CfdStudyConfig,
    /// Point lifecycle/numerical classification.
    pub status: CfdSweepPointStatus,
    /// Actual parsed case result, when one was produced.
    pub result: Option<CfdResults>,
    /// Infrastructure failure detail when no result was returned.
    pub error: Option<String>,
}

impl CfdSweepPointResult {
    fn pending(index: usize, value: f64, config: CfdStudyConfig) -> Self {
        Self {
            index,
            value,
            config,
            status: CfdSweepPointStatus::Pending,
            result: None,
            error: None,
        }
    }
}

/// One message sent by the non-UI CFD worker.
enum CfdWorkerMessage {
    /// A lifecycle event from geometry preparation, meshing, or solving.
    Event {
        run_id: u64,
        input_revision: u64,
        event: CfdRunEvent,
    },
    /// The final result or failure from one run.
    Finished {
        run_id: u64,
        input_revision: u64,
        result: Result<CfdResults, String>,
    },
    /// A sequential sweep point has begun in its own isolated case directory.
    SweepPointStarted {
        run_id: u64,
        input_revision: u64,
        index: usize,
        value: f64,
        config: CfdStudyConfig,
    },
    /// A sweep point has returned actual case evidence or an infrastructure error.
    SweepPointFinished {
        run_id: u64,
        input_revision: u64,
        index: usize,
        result: Result<CfdResults, String>,
    },
    /// The sequential sweep worker has no more points to process.
    SweepFinished { run_id: u64, input_revision: u64 },
}

/// One message sent by the non-UI OpenFOAM probe worker.
enum ProbeMessage {
    Finished(OpenFoamCapabilities),
}

/// State retained while the Airfoil CFD window is open, or between runs.
pub struct AirfoilCfdState {
    /// The complete routine/advanced study contract edited by the window.
    pub config: CfdStudyConfig,
    /// Case-directory root used by future runs and shown in the UI.
    pub case_root: PathBuf,
    /// File used by the Save/Reload case-settings actions.
    pub saved_study_path: PathBuf,
    /// File used by the Save/Reload sweep-settings actions.
    pub saved_sweep_path: PathBuf,
    /// Search text for the immutable database library.
    pub airfoil_filter: String,
    /// Names matching [`airfoil_filter`].
    pub filtered_airfoils: Vec<String>,
    /// Coordinates of the selected database section for the immediate preview.
    pub preview_coordinates: Option<Vec<(f64, f64)>>,
    /// Whether the detached window is visible.
    pub window_open: bool,
    /// Current window tab.
    pub tab: CfdTab,
    /// Whether a solver worker is active.
    pub running: bool,
    /// Whether the OpenFOAM connection probe worker is active.
    pub probing: bool,
    /// Compact stage/status text for the window and top-level shell.
    pub status: String,
    /// Current lifecycle stage, if a run is active.
    pub stage: Option<CfdStage>,
    /// Events received for the current run, retained for inspection.
    pub events: Vec<CfdRunEvent>,
    /// The most recent result whose revision still matches the inputs.
    pub result: Option<CfdResults>,
    /// Sequential AoA/Reynolds sweep definition.
    pub sweep_settings: CfdSweepSettings,
    /// Per-point actual results and provenance from the last sweep.
    pub sweep_results: Vec<CfdSweepPointResult>,
    /// Whether the active worker is processing a sweep rather than one case.
    pub sweep_running: bool,
    /// Human-readable validation or worker error.
    pub error: Option<String>,
    /// Last connection-test capabilities, if a probe has completed.
    pub capabilities: Option<OpenFoamCapabilities>,
    /// Persisted OpenFOAM backend/environment settings.
    pub openfoam_preferences: OpenFoamPreferences,
    /// Persisted Gmsh executable path, when configured.
    pub gmsh_executable: Option<String>,
    /// Optional ParaView executable used to inspect the actual `.foam` case.
    pub paraview_executable: Option<String>,
    /// Case directory belonging to the last accepted run.
    pub last_case_dir: Option<PathBuf>,
    /// Field artifact selected for inspection in the Results tab.
    pub selected_field: Option<String>,
    /// Decoded native contour figures keyed by their exact artifact path.
    /// The cache is process-local UI state; the source PNGs remain in the
    /// reproducible case directory and are never synthesized by the GUI.
    pub contour_textures: BTreeMap<String, egui::TextureHandle>,
    /// Editable path used to import a previously completed, reproducible
    /// `results.json` artifact into the detached window.  Keeping this
    /// explicit lets a user inspect a production case after restarting ALAS
    /// without reconstructing or rerunning it.
    pub result_json_path: String,
    /// Revision incremented whenever any study input changes.
    pub input_revision: u64,
    /// Monotonic ID for worker runs.  IDs are never reused during a session.
    pub run_id: u64,
    run_input_revision: u64,
    rx: Option<Receiver<CfdWorkerMessage>>,
    probe_rx: Option<Receiver<ProbeMessage>>,
    cancel_flag: Arc<AtomicBool>,
}
