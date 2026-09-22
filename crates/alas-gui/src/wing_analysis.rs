// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Desktop state and worker boundary for the standalone Wing Analysis window.
//!
//! `alas-aero::wing_analysis` owns the aerodynamic entry; this module owns only
//! desktop concerns: the geometry snapshot the window analyses, the live
//! preview scene built from that snapshot, the revision gate that keeps a late
//! worker result from replacing a newer one, cancellation, and the window's own
//! visibility and tab.
//!
//! # Where this state lives
//!
//! The window state is process-local rather than a field of
//! [`crate::state::AppState`]. Every accessor goes through [`with_window`], so
//! moving it into `AppState` later is a field move plus a change of that one
//! function: nothing else reaches the storage. The struct itself is plain data
//! and holds no global.
//!
//! # Revisions
//!
//! `geometry_revision` counts changes to the analysed surfaces: a different
//! aircraft geometry, a different design point, or the empennage option being
//! toggled. `input_revision` counts those and every other edit that changes
//! what a run would compute. A worker carries the revision it started under and
//! its result is installed only when that revision is still current, so a slow
//! run can never overwrite the state of a newer one.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use alas_aero::wing_analysis::{
    AlphaSweep, AttitudeInput, FlightCondition, SpeedInput, SurfaceSet, WingAnalysisInputs,
    WingAnalysisOutcome, WingModel,
};
use alas_report::scene::Scene;

use crate::viewport::PreviewCamera;

#[path = "wing_analysis_worker.rs"]
mod worker;

pub use worker::WingAnalysisRun;

/// The key the detached viewport and its scene cache are built from.
pub const WING_ANALYSIS_VIEW_KEY: &str = "wing_analysis_preview";

/// Visible tabs of the Wing Analysis window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WingAnalysisTab {
    /// Analysed configuration, live preview and the run inputs.
    #[default]
    Setup,
    /// Actual solved outputs for the current revision.
    Results,
}

/// What the window is doing, as the header reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunPhase {
    /// No run has been started since the last invalidation.
    #[default]
    Idle,
    /// A worker is solving.
    Running,
    /// A stop was requested and the worker has not yet reached a checkpoint.
    Cancelling,
    /// A result for the current revision is installed.
    Finished,
    /// The last run failed or was refused.
    Failed,
}

/// State retained while the Wing Analysis window is open, or between runs.
pub struct WingAnalysisState {
    /// Whether the detached window is visible.
    pub window_open: bool,
    /// Current window tab.
    pub tab: WingAnalysisTab,
    /// The complete run contract the window edits.
    pub inputs: WingAnalysisInputs,
    /// The surfaces the current snapshot lofted, with their references.
    pub model: Option<WingModel>,
    /// Why the current geometry could not be snapshotted, when it could not.
    pub geometry_error: Option<String>,
    /// Name of the configuration the snapshot was taken from.
    pub source_name: String,
    /// Live preview of the analysed surfaces for the current revision.
    pub preview: Option<Arc<Scene>>,
    /// Orbit state of the preview camera.
    pub camera: PreviewCamera,
    /// Whether the moment reference was typed by the user.
    ///
    /// A derived reference follows the snapshot's quarter mean-aerodynamic
    /// chord; a typed one is never overwritten by a geometry refresh.
    pub moment_reference_manual: bool,
    /// Revision of the analysed surfaces.
    pub geometry_revision: u64,
    /// Revision of everything a run reads.
    pub input_revision: u64,
    /// The most recent outcome whose revision still matches the inputs.
    pub result: Option<WingAnalysisOutcome>,
    /// Revision the installed result was produced under.
    pub result_revision: u64,
    /// Current run phase.
    pub phase: RunPhase,
    /// Compact status text for the window header.
    pub status: String,
    /// Validation or worker error text.
    pub error: Option<String>,
    /// Monotonic run identifier; never reused during a session.
    pub run_id: u64,
    /// Signature of the geometry the snapshot was taken from.
    pub(crate) geometry_signature: Option<u64>,
    /// Theme the preview scene was drawn for.
    pub(crate) preview_theme: String,
    pub(crate) run_revision: u64,
    pub(crate) rx: Option<Receiver<WingAnalysisRun>>,
    pub(crate) cancel_flag: Arc<AtomicBool>,
}

impl Default for WingAnalysisState {
    fn default() -> Self {
        Self {
            window_open: false,
            tab: WingAnalysisTab::default(),
            inputs: WingAnalysisInputs {
                surfaces: SurfaceSet::WingOnly,
                condition: FlightCondition {
                    altitude_m: 10_000.0,
                    speed: SpeedInput::Mach(0.5),
                    attitude: AttitudeInput::AngleOfAttack(2.0),
                },
                moment_reference_m: [0.0, 0.0, 0.0],
                spanwise_resolution: 1,
                chordwise_resolution: 8,
                sweep: AlphaSweep::default(),
            },
            model: None,
            geometry_error: None,
            source_name: String::new(),
            preview: None,
            camera: PreviewCamera::isometric(),
            moment_reference_manual: false,
            geometry_revision: 0,
            input_revision: 0,
            result: None,
            result_revision: 0,
            phase: RunPhase::Idle,
            status: String::new(),
            error: None,
            run_id: 0,
            geometry_signature: None,
            preview_theme: String::new(),
            run_revision: 0,
            rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl WingAnalysisState {
    /// Whether a worker is solving right now.
    pub fn running(&self) -> bool {
        matches!(self.phase, RunPhase::Running | RunPhase::Cancelling)
    }

    /// Whether the installed result belongs to the current inputs.
    ///
    /// A result is kept only while its revision matches, so this is the one
    /// question the window asks before showing a number as current.
    pub fn result_is_current(&self) -> bool {
        self.result.is_some() && self.result_revision == self.input_revision
    }

    /// Whether the empennage is part of the analysed configuration.
    pub fn includes_empennage(&self) -> bool {
        self.inputs.surfaces.includes_empennage()
    }

    /// Record an edit to anything a run reads.
    ///
    /// The result is dropped rather than left on screen under new inputs: a
    /// number that no longer belongs to the shown condition is worse than no
    /// number. A running worker is left alone; its revision no longer matches,
    /// so [`Self::poll`] discards whatever it returns.
    pub fn invalidate_inputs(&mut self) {
        self.input_revision = self.input_revision.wrapping_add(1);
        if self.result.is_some() {
            self.result = None;
            self.status = "Inputs changed; the previous result no longer applies.".to_owned();
        }
        self.error = None;
        if matches!(self.phase, RunPhase::Finished | RunPhase::Failed) {
            self.phase = RunPhase::Idle;
        }
    }

    /// Record a change to the analysed surfaces themselves.
    pub fn invalidate_geometry(&mut self) {
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.preview = None;
        self.invalidate_inputs();
    }

    /// Select the modelled surface set, invalidating preview and results when
    /// it actually changes.
    pub fn set_surfaces(&mut self, surfaces: SurfaceSet) {
        if self.inputs.surfaces == surfaces {
            return;
        }
        self.inputs.surfaces = surfaces;
        self.geometry_signature = None;
        self.invalidate_geometry();
    }

    /// The reference quantities of the current snapshot, when one exists.
    pub fn reference(&self) -> Option<alas_aero::wing_analysis::WingReference> {
        self.model.as_ref().map(WingModel::reference)
    }

    /// The modelled surface names of the current snapshot.
    pub fn surface_names(&self) -> Vec<String> {
        self.model
            .as_ref()
            .map(WingModel::surface_names)
            .unwrap_or_default()
    }

    /// Ask a running worker to stop at its next checkpoint.
    pub fn cancel(&mut self) {
        if !self.running() {
            return;
        }
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.phase = RunPhase::Cancelling;
        self.status = "Cancelling; the run stops at its next solved point.".to_owned();
    }
}

thread_local! {
    /// The single Wing Analysis window state of this process.
    static WINDOW: RefCell<WingAnalysisState> = RefCell::new(WingAnalysisState::default());
}

/// Run `action` against the Wing Analysis window state.
///
/// The one accessor to the storage; see the module doc for why the state is
/// held here rather than on [`crate::state::AppState`]. Re-entering this
/// function from inside `action` is a borrow error by construction, which is
/// the intended guard against two live views of one window.
pub fn with_window<R>(action: impl FnOnce(&mut WingAnalysisState) -> R) -> R {
    WINDOW.with(|window| action(&mut window.borrow_mut()))
}

/// Restore the window to its initial state.
///
/// Used by tests, which share one thread-local per test thread and must not
/// inherit another test's window.
pub fn reset_window() {
    with_window(|window| *window = WingAnalysisState::default());
}
