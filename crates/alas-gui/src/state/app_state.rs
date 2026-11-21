// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The master interactive state and its startup defaults.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use super::tools::apply_tool_preferences;
pub(super) use super::tools::first_start_marker_gate;
use super::types::*;
use crate::feedback::ParameterFeedback;
use crate::path_picker::PathPicker;
use crate::theme::AppTheme;
pub use crate::viewport::PreviewCamera;
use crate::views::external_tool_catalog::ExternalToolConfig;
use crate::views::results_view::SolverResultView;
use crate::views::tour_data::TourTarget;
use alas_config::{
    airports, engines, presets, validate, AlasConfig, ConfigNode, Node, ValidationIssue,
};
use alas_exec::{ToolLocator, ToolPreferences};
use alas_pipeline::{PipelineOptions, PipelineResult, RunEvent};
use alas_report::scene::Scene;
use alas_viz::SceneViewState;
use serde_json::Value;

/// Master interactive state for the ALAS desktop application.
pub struct AppState {
    /// The edited configuration as a value tree: the single source of truth (module doc).
    pub config_values: Value,
    /// Memo behind [`Self::typed_config`]: the content fingerprint
    /// `config_values` had when `AlasConfig` was last decoded, with the
    /// result. `config_values` is mutated directly all over the crate, so the
    /// memo is validated by content on every read; the fingerprint itself is
    /// memoized per frame and edit revision in `fingerprint_memo`.
    pub(crate) typed_config_memo: std::cell::RefCell<Option<(u64, Arc<AlasConfig>)>>,
    pub(crate) fingerprint_memo: crate::state::state_memo::FingerprintMemo,
    /// Memo behind [`crate::config_edit_design_space::AppState::enforce_design_space_fixed_variables`]:
    /// the `(config_fingerprint, design_values_fingerprint)` pair the design
    /// space was already an enforced fixed point for. The function is
    /// idempotent, so a repeat call with an unchanged pair is a no-op and the
    /// per-frame call skips its `alas_opt::canonicalize_design` work.
    pub(crate) design_space_enforcement_memo: Option<(u64, u64)>,
    /// Fingerprint of the clean-sheet brief the design space was last derived from.
    pub(crate) clean_sheet_brief_fingerprint: Option<(u64, u64)>,
    /// Memo behind [`Self::cached_page_preview`]: a fingerprint of every
    /// input `crate::scene::build_page_preview` reads (config, design, the
    /// preview id, theme, language and that preview's camera), paired with
    /// the scene it produced. `build_page_preview` can run a full mass
    /// analysis for some preview ids (`quick_preview_report`), so an
    /// Advanced form page must not rerun it merely because egui repainted.
    pub(crate) page_preview_cache:
        std::cell::RefCell<std::collections::HashMap<String, (u64, Arc<Scene>)>>,
    /// The structure, labels, kinds and bounds every form is rendered from.
    /// Computed once from the defaults, since only the values change.
    pub schema: Node,
    /// The registry key of the aircraft preset last loaded.
    pub active_preset: String,
    /// The configuration right after the preset named here finished loading:
    /// the value a form compares with for "Modified" (see `views::form::baseline`).
    pub(crate) loaded_baseline: Option<(String, u64, Value)>,
    /// Every aircraft preset, as `(registry key, display name)`.
    pub preset_names: Vec<(String, String)>,
    /// Every selectable engine name.
    pub engine_names: Vec<String>,
    /// Every selectable aerodrome display name.
    pub airport_names: Vec<String>,
    /// Draft values in the custom-airport editor.
    pub custom_airport_draft: crate::airport_editor::CustomAirportDraft,
    /// Whether the detached custom-airport editor is open, and which route
    /// selector opened it.
    pub custom_airport_window: crate::views::airport_window::CustomAirportWindow,
    /// Path used by airport import/export actions.
    pub custom_airport_file_path: String,
    /// Latest custom-airport action status.
    pub custom_airport_status: Option<String>,
    /// Path used by custom-airfoil import actions.
    pub custom_airfoil_file_path: String,
    /// Latest custom-airfoil action status.
    pub custom_airfoil_status: Option<String>,
    /// The nominal/starting value of each design variable, keyed by its name.
    pub design_values: BTreeMap<String, f64>,
    /// The optimizer's lower/upper bound for each design variable.
    pub bounds: BTreeMap<String, (f64, f64)>,
    /// Which aux-preset is selected on each Advanced page, keyed by preset kind.
    pub selected_aux_preset: BTreeMap<String, String>,
    /// The active navigation page's id.
    pub active_page: String,
    /// The colour theme.
    pub theme: AppTheme,
    /// The interface language.
    pub language: Language,
    /// Whether the right-hand live-preview dock is open.
    pub preview_open: bool,
    /// How the live-preview dock framed its aircraft (`None` until measured).
    pub preview_fit: Option<crate::sandbox::scene::LivePreviewFit>,
    /// The dock preview's pan on top of its fit, canvas units.
    pub preview_pan: (f64, f64),
    /// The pitch the dock chose for its pane while the camera is untouched
    /// (a tall pane tilts the default view so the aircraft fills more of it).
    pub preview_auto_pitch: Option<f64>,
    /// Shell-wide UI state: detail level, search, Files page, starting point.
    pub shell: crate::shell_ui::ShellUi,
    /// Whether the navigation rail is pinned open and reserves layout space.
    pub nav_pinned: bool,
    /// Whether the unpinned navigation overlay is currently open from hover.
    pub nav_hover_open: bool,
    /// Whether navigation motion is replaced by immediate state changes.
    pub reduced_animations: bool,
    /// The whole-interface zoom multiplier (View > Zoom).
    pub zoom: f32,
    /// Whether zoom follows the current window size until the user chooses a
    /// manual View > Zoom command.
    pub zoom_auto: bool,
    /// Whether the unified aircraft viewer shows its exterior or interior.
    pub preview_tab: PreviewTab,
    /// Per-preview orbit state. A preview must not move another preview's
    /// aircraft when users drag or select a camera preset.
    pub preview_cameras: BTreeMap<String, PreviewCamera>,
    /// Per-result orbit state, keyed independently from live-preview cameras.
    pub result_cameras: BTreeMap<String, PreviewCamera>,
    /// The per-run toggles.
    pub run_options: RunOptions,
    /// The pipeline execution options a run is launched with.
    pub pipeline_options: PipelineOptions,
    /// Whether a run is in flight.
    pub is_running: bool,
    /// The one-line status shown in the control bar.
    pub status_message: String,
    /// The latest parameter edit, delayed before it reaches the run log.
    pub parameter_feedback: Option<ParameterFeedback>,
    /// The optional external-tool chooser waiting outside the render thread.
    pub path_picker: Option<PathPicker>,
    /// The last stage message a run emitted.
    pub stage: String,
    /// When the current run started, for the elapsed stopwatch.
    pub run_started: Option<Instant>,
    /// The most recent pipeline snapshot or finished pipeline result.
    pub pipeline_result: Option<PipelineResult>,
    /// Whether `pipeline_result` is the final result of a completed run;
    /// snapshots published before downstream stages finish keep this false.
    pub pipeline_result_complete: bool,
    /// The design values on the form when `pipeline_result` was started; a
    /// later edit makes the run's report stale for the previews.
    pub pipeline_result_design_values: Option<BTreeMap<String, f64>>,
    /// The form's configuration at run start, before the dispatch adjustments.
    pub pipeline_result_form_config: Option<alas_config::AlasConfig>,
    /// Whether the user has manually changed a mission-profile phase.
    /// Route changes use this bit to switch from automatic regeneration to
    /// the explicit retain/regenerate prompt.
    pub mission_profile_manual_edit: bool,
    /// Route signature for which the displayed profile was last reconciled.
    pub mission_profile_route_signature: String,
    /// Whether a route change is waiting for the user's profile decision.
    pub mission_profile_regeneration_prompt: bool,
    /// Validation warning for phases retained after a route change.
    pub mission_profile_retained_validation: Option<String>,
    /// Native editor opened by clicking a phase in the mission profile
    /// preview.
    pub(crate) mission_profile_window: crate::views::mission_profile_inputs::MissionProfileWindow,
    /// Monotonic identity for the current result-producing run.
    pub run_identity: u64,
    /// The channel a running pipeline reports over.
    pub worker_rx: Option<Receiver<WorkerMessage>>,
    /// Completion channel for the optional navigation-data download.
    pub navdata_download_rx: Option<Receiver<Result<alas_exec::download::DownloadOutcome, String>>>,
    /// Whether a navigation-data transfer is currently running.
    pub navdata_download_in_progress: bool,
    /// Last completion message for the detached manager, including failures.
    pub navdata_download_feedback: Option<(String, LogKind)>,
    /// The flag that asks a running navigation-data download to stop.
    ///
    /// Deliberately separate from `cancel_flag` below: the pipeline run and
    /// the navdata download share no lifecycle, and reusing one flag for
    /// both would let cancelling one silently abort the other.
    pub navdata_download_cancel: Arc<AtomicBool>,
    /// Background state for the optional OpenVSP preview-runtime installer.
    pub openvsp_runtime_setup: crate::openvsp_runtime_setup::OpenVspRuntimeSetup,
    /// Background state for the "Install all" external-tools flow.
    pub tool_installer: crate::tool_installer::ToolInstaller,
    /// The flag that asks a running pipeline to stop.
    pub cancel_flag: Arc<AtomicBool>,
    /// Whether the user has already requested cancellation for this run.
    pub cancellation_requested: bool,
    /// Typed lifecycle events for the active or most recently completed run.
    pub run_events: Vec<RunEvent>,
    /// The run log.
    pub logs: Vec<LogLine>,
    /// Case-insensitive text filter applied by the run-log toolbar.
    pub run_log_search: String,
    /// Severity visibility toggles for the run-log toolbar.
    pub run_log_show_info: bool,
    /// Whether warning-level run-log lines are shown.
    pub run_log_show_warn: bool,
    /// Whether error-level run-log lines are shown.
    pub run_log_show_error: bool,
    /// Feedback from the most recent run-log export.
    pub run_log_export_status: Option<String>,
    /// Full-width Run Log tab shown while the pipeline is idle.
    pub run_log_tab: RunLogTab,
    /// The run-log dock height in points, retained while the app is open.
    pub run_log_height: f32,
    /// Whether the guided-workspace run-log dock is visible.
    pub run_log_open: bool,
    /// `requirements.cruise_mach` at the previous edit (cruise-speed follow-Mach sync).
    pub(crate) synced_cruise_mach: Option<f64>,
    /// The current validation findings.
    pub validation_findings: Vec<ValidationIssue>,
    /// The preview figure selected in the dock's exterior tab.
    pub selected_preview_id: String,
    /// The rendered preview scene, already adjusted for the dock tab it was
    /// built for; shared with the dock rather than copied each frame.
    pub preview_scene: Option<Arc<Scene>>,
    /// Monotonic generation of the live preview scene and camera projection.
    pub preview_scene_revision: u64,
    /// The active discipline tab in the results gallery ("summary" or a
    /// `results_view::TABS` id).
    pub results_tab: String,
    /// Which optimized aerodynamic branch the results gallery displays.
    pub selected_solver_view: SolverResultView,
    /// The result figure selected in the results gallery.
    pub selected_result_id: String,
    /// The rendered result scene.
    pub result_scene: Option<Scene>,
    /// Persistent pan/zoom for every interactive canvas.
    ///
    /// Viewports are keyed by their owning surface rather than sharing one
    /// camera.  This matters because result cards, screening figures, page
    /// previews, and the dock can all be visible in the same frame.
    pub view_states: BTreeMap<String, SceneViewState>,
    /// Decoded Patran PNGs retained by source path so the results page can
    /// display solver-owned artifacts without decoding them every frame.
    pub patran_textures: BTreeMap<String, egui::TextureHandle>,
    /// Result scenes cached by run/config/theme/figure identity, each paired
    /// with the revision it was built at. Figure builders can perform
    /// substantial geometry and plotting work, so they must not run again
    /// merely because egui repainted the window. The revision lets a static
    /// (non-orbiting) result card pass `SceneView::cache_revision` instead of
    /// falling back to hashing the whole scene graph every frame.
    pub result_figure_cache: BTreeMap<String, (Option<Arc<Scene>>, u64)>,
    /// Monotonic source for `result_figure_cache` revisions; never reset, so a
    /// revision is never reused (a stale GPU texture must not match a new scene).
    pub result_figure_revision_counter: u64,
    /// Worker threads that build result figures off the UI thread.
    pub(crate) figure_jobs: crate::figure_jobs::FigureJobs,
    /// Worker threads that build page previews off the UI thread.
    pub(crate) preview_jobs: std::cell::RefCell<crate::preview_jobs::PreviewJobs>,
    /// The path the config Load/Save actions read and write.
    pub config_path: String,
    /// Dialog, "My aircraft" and import-note state of the file features.
    pub files: crate::user_files::UserFilesState,
    /// Shared external-tool locator used by the GUI worker and setup page.
    pub tool_locator: ToolLocator,
    /// User-level tool locations retained across GUI restarts.
    pub tool_preferences: ToolPreferences,
    /// Whether the About window is open.
    pub show_about: bool,
    /// Whether the storage dialog is open.
    pub show_storage: bool,
    /// Whether View controls are shown in their movable desktop window.
    pub show_view_panel: bool,
    /// Whether the first-run walkthrough is showing.
    pub show_walkthrough: bool,
    /// The walkthrough's current step.
    pub walkthrough_step: usize,
    /// Actual shell response rectangles measured during the current frame.
    pub walkthrough_targets: BTreeMap<TourTarget, egui::Rect>,
    /// Shell state to restore when the walkthrough closes.
    pub walkthrough_restore: Option<WalkthroughRestore>,
    /// Detached External Tools manager; opened once independently of the tour.
    pub show_tool_intro: bool,
    /// Selected settings view, or `None` for the acquisition overview.
    pub tool_intro_selected_config: Option<ExternalToolConfig>,
    /// One-session consent for the manager's "Download all" action.
    pub tool_intro_download_all_consent: bool,
    /// Per-item consent checkbox for the optional X-Plane navigation-data
    /// download, shown on the first-start disclosure screen. Defaults
    /// unchecked; the "Download now" button there stays disabled until it is
    /// checked, so no transfer can start without explicit, per-item consent.
    pub tool_intro_navdata_consent: bool,
    /// Per-item consent checkbox for the optional OpenVSP preview-runtime
    /// download, shown on the first-start disclosure screen. Same unchecked
    /// default and gating as `tool_intro_navdata_consent`.
    pub tool_intro_openvsp_preview_consent: bool,
    /// The airfoil-screening sweep's own run state.
    pub screening: crate::screening::ScreeningState,
    /// Independent OpenFOAM airfoil study window and worker state.
    pub cfd: crate::cfd::AirfoilCfdState,
    /// Independent fixed-wing UAV inputs, selections, and latest outcome.
    pub uav: crate::uav::UavWorkflowState,
    /// The Add-ons page: discovered add-ons, settings and the last run.
    pub addons: crate::addons::AddonsPanel,
    /// The clean-sheet sandbox session and workspace mode.
    pub sandbox: Box<crate::sandbox::SandboxSession>,
    /// Frames remaining for the fixed-length boot splash (nothing is awaited).
    pub boot_frames_remaining: u32,
}

/// File name used inside the same persistent user-data directory as tool
/// preferences. A launcher's working directory is not stable across shortcuts,
/// development builds, and packaged executables.
pub(crate) const ONBOARDING_MARKER_FILE: &str = "onboarding-seen";

/// Marker for the first-start external-tool disclosure screen
/// ([`crate::views::tool_intro`]), kept separate from
/// [`ONBOARDING_MARKER_FILE`] so the two once-per-installation screens are
/// dismissed independently of each other.
pub(crate) const EXTERNAL_TOOLS_INTRO_MARKER_FILE: &str = "external-tools-intro-seen";

/// File in the per-user data folder that exists while the navigation is
/// pinned: it starts collapsed unless the user pinned it in an earlier session.
const NAV_PINNED_MARKER_FILE: &str = "nav-pinned";

impl AppState {
    /// The file whose existence records a pinned navigation.
    pub(crate) fn nav_pinned_marker(&self) -> std::path::PathBuf {
        self.data_dir().join(NAV_PINNED_MARKER_FILE)
    }
}

/// The maximum retained run-log lines. A long optimization emits one line per
/// progress update; nobody scrolls back through thousands of superseded lines.
pub(crate) const MAX_LOG_LINES: usize = 1500;

mod defaults;
