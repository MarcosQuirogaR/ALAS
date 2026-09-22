// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::feedback::ParameterFeedback;
use crate::path_picker::PathPicker;
use crate::theme::AppTheme;
pub use crate::viewport::PreviewCamera;
use crate::views::results_view::SolverResultView;
use crate::views::tour_data::TourTarget;
use alas_config::{
    airports, engines, presets, validate, AlasConfig, ConfigNode, DesignVector, Node, Severity,
    ValidationIssue, DESIGN_VARIABLE_SPECS,
};
use alas_exec::{ToolLocator, ToolPreferences};
use alas_pipeline::{PipelineOptions, PipelineResult, RunEvent};
use alas_report::scene::Scene;
use alas_viz::SceneViewState;
use serde_json::Value;
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

/// The four per-run toggles the Inputs page offers, mirroring the reference's
/// `RunOptions`.
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

/// Master interactive state for the ALAS desktop application.
pub struct AppState {
    /// The edited configuration, as a tree of values (the single source of
    /// truth for editing; see the module doc).
    pub config_values: Value,
    /// The structure, labels, kinds and bounds every form is rendered from.
    /// Computed once from the defaults, since only the values change.
    pub schema: Node,
    /// The registry key of the aircraft preset last loaded.
    pub active_preset: String,
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
    /// Whether the "learn-more help" deep-dives are shown.
    pub help_verbose: bool,
    /// Whether the right-hand live-preview dock is open.
    pub preview_open: bool,
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
    /// Whether `pipeline_result` is the final result of a successfully
    /// completed pipeline run. Snapshots published before downstream stages
    /// finish deliberately keep this false, even though their figures are
    /// safe to display.
    pub pipeline_result_complete: bool,
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
    /// The flag that asks a running navigation-data download to stop.
    ///
    /// Deliberately separate from `cancel_flag` below: the pipeline run and
    /// the navdata download share no lifecycle, and reusing one flag for
    /// both would let cancelling one silently abort the other.
    pub navdata_download_cancel: Arc<AtomicBool>,
    /// Background state for the optional OpenVSP preview-runtime installer.
    pub openvsp_runtime_setup: crate::openvsp_runtime_setup::OpenVspRuntimeSetup,
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
    pub run_log_show_warn: bool,
    pub run_log_show_error: bool,
    /// Feedback from the most recent run-log export.
    pub run_log_export_status: Option<String>,
    /// Full-width Run Log tab shown while the pipeline is idle.
    pub run_log_tab: RunLogTab,
    /// The run-log dock height in points, retained while the app is open.
    pub run_log_height: f32,
    /// Whether the guided-workspace run-log dock is visible.
    pub run_log_open: bool,
    /// The current validation findings.
    pub validation_findings: Vec<ValidationIssue>,
    /// The preview figure selected in the dock's exterior tab.
    pub selected_preview_id: String,
    /// The rendered preview scene.
    pub preview_scene: Option<Scene>,
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
    /// Result scenes cached by run/config/theme/figure identity. Figure
    /// builders can perform substantial geometry and plotting work, so they
    /// must not run again merely because egui repainted the window.
    pub result_figure_cache: BTreeMap<String, Option<Arc<Scene>>>,
    /// The path the config Load/Save actions read and write.
    pub config_path: String,
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
    /// Whether the advanced walkthrough guide is open.
    pub show_advanced_guide: bool,
    /// The advanced guide's active chapter index.
    pub guide_chapter: usize,
    /// Whether the first-start external-tool disclosure screen is open.
    ///
    /// Gated on its own marker file, independent of `show_walkthrough`'s: a
    /// user who skips the general walkthrough must still see this once, and
    /// dismissing one must never silently mark the other seen.
    pub show_tool_intro: bool,
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
    /// The clean-sheet sandbox session and workspace mode.
    pub sandbox: crate::sandbox::SandboxSession,
    /// Frames remaining for the boot splash. The reference's `Splash` bridges
    /// a real network wait for a cold-starting Python sidecar; this port calls
    /// the library directly and has nothing to wait for, so this is a short,
    /// fixed-length branded flash rather than a state machine over a wait.
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

/// Whether `marker_path` is being seen here for the first time: if it is
/// absent, it is created immediately (matching the "seen" semantics below,
/// which flip on first display rather than on completion) and `true` is
/// returned exactly once per marker file.
fn first_start_marker_gate(marker_path: &Path) -> bool {
    if marker_path.exists() {
        return false;
    }
    if let Some(parent) = marker_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker_path, b"1");
    true
}

/// The maximum retained run-log lines. A long optimization emits one line per
/// progress update; nobody scrolls back through thousands of superseded lines.
pub(crate) const MAX_LOG_LINES: usize = 1500;

impl Default for AppState {
    fn default() -> Self {
        let tool_locator = ToolLocator::for_current_process();
        let tool_preferences = tool_locator.load_preferences();
        let mut config = AlasConfig::default();
        apply_tool_preferences(&mut config, &tool_preferences);
        let schema = config.schema();
        let config_values = crate::config_edit::full_config_values(&config);

        let preset_names = presets::display_names()
            .into_iter()
            .map(|(name, display)| (name.to_owned(), display.to_owned()))
            .collect();
        let engine_names = engines::available().iter().map(|s| s.to_string()).collect();
        let airport_names = airports::database_with_custom()
            .iter()
            .map(|a| a.name.clone())
            .collect();

        let mut design_values = BTreeMap::new();
        let mut bounds = BTreeMap::new();
        for spec in alas_config::DESIGN_VARIABLE_SPECS {
            design_values.insert(spec.name.to_owned(), spec.default);
            bounds.insert(spec.name.to_owned(), (spec.lower, spec.upper));
        }

        let findings = validate(&config);

        // A desktop shortcut commonly starts in a read-only installation
        // directory (or in `C:\Windows\System32`).  Keep the GUI's default
        // artifact tree in the same per-user data root used for preferences;
        // development checkouts still resolve to their existing `outputs`
        // directory when it is present.  The CLI retains its explicit
        // working-directory default because its output path is a command-line
        // contract.
        let pipeline_options = PipelineOptions {
            output_dir: Some(tool_locator.resolve_data_path(Path::new("outputs"))),
            ..PipelineOptions::default()
        };
        let cfd = crate::cfd::AirfoilCfdState::new(&tool_locator);

        let mut state = Self {
            config_values,
            schema,
            active_preset: String::new(),
            preset_names,
            engine_names,
            airport_names,
            custom_airport_draft: crate::airport_editor::CustomAirportDraft::default(),
            custom_airport_window: crate::views::airport_window::CustomAirportWindow::default(),
            custom_airport_file_path: "alas-airports.json".to_owned(),
            custom_airport_status: None,
            custom_airfoil_file_path: "custom-airfoil.dat".to_owned(),
            custom_airfoil_status: None,
            design_values,
            bounds,
            selected_aux_preset: BTreeMap::new(),
            active_page: "inputs".to_owned(),
            theme: AppTheme::Dark,
            language: Language::En,
            // Fresh settings show the explanatory help by default. An
            // explicitly constructed state keeps the caller's preference.
            help_verbose: true,
            preview_open: true,
            nav_pinned: false,
            nav_hover_open: false,
            reduced_animations: false,
            zoom: 1.0,
            zoom_auto: true,
            preview_tab: PreviewTab::Exterior,
            preview_cameras: BTreeMap::new(),
            result_cameras: BTreeMap::new(),
            run_options: RunOptions::default(),
            pipeline_options,
            is_running: false,
            status_message: "Ready.".to_owned(),
            parameter_feedback: None,
            path_picker: None,
            stage: String::new(),
            run_started: None,
            pipeline_result: None,
            pipeline_result_complete: false,
            mission_profile_manual_edit: false,
            mission_profile_route_signature: String::new(),
            mission_profile_regeneration_prompt: false,
            mission_profile_retained_validation: None,
            mission_profile_window:
                crate::views::mission_profile_inputs::MissionProfileWindow::default(),
            run_identity: 0,
            worker_rx: None,
            navdata_download_rx: None,
            navdata_download_in_progress: false,
            navdata_download_cancel: Arc::new(AtomicBool::new(false)),
            openvsp_runtime_setup: crate::openvsp_runtime_setup::OpenVspRuntimeSetup::default(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            cancellation_requested: false,
            run_events: Vec::new(),
            logs: vec![LogLine {
                text: "ALAS initialized.".to_owned(),
                kind: LogKind::Info,
                elapsed: None,
                run_id: 0,
            }],
            run_log_search: String::new(),
            run_log_show_info: true,
            run_log_show_warn: true,
            run_log_show_error: true,
            run_log_export_status: None,
            run_log_tab: RunLogTab::Console,
            run_log_height: 220.0,
            run_log_open: false,
            validation_findings: findings,
            selected_preview_id: "exterior_3d".to_owned(),
            preview_scene: None,
            preview_scene_revision: 0,
            results_tab: "summary".to_owned(),
            selected_solver_view: SolverResultView::Vlm,
            selected_result_id: "polar_comparison".to_owned(),
            result_scene: None,
            view_states: BTreeMap::new(),
            patran_textures: BTreeMap::new(),
            result_figure_cache: BTreeMap::new(),
            config_path: "alas-config.json".to_owned(),
            tool_locator,
            tool_preferences,
            show_about: false,
            show_storage: false,
            show_view_panel: false,
            show_walkthrough: false,
            walkthrough_step: 0,
            walkthrough_targets: BTreeMap::new(),
            walkthrough_restore: None,
            show_advanced_guide: false,
            guide_chapter: 0,
            show_tool_intro: false,
            tool_intro_navdata_consent: false,
            tool_intro_openvsp_preview_consent: false,
            screening: crate::screening::ScreeningState::default(),
            cfd,
            uav: crate::uav::UavWorkflowState::default(),
            sandbox: crate::sandbox::SandboxSession::default(),
            boot_frames_remaining: 40,
        };

        // Load the first registered preset at startup, the way the reference
        // desktop app loads combo index 0 rather than starting from bare
        // `AlasConfig::default()` values.
        if let Some((first, _)) = state.preset_names.first().cloned() {
            state.load_preset(&first);
        } else {
            state.update_preview_scene();
        }

        // Show the tour on a launch that has never seen it, and mark it seen
        // right away, matching the reference's "seen" semantics, which flip
        // on first display rather than on completion.
        let onboarding_marker = state
            .tool_locator
            .preferences_path()
            .with_file_name(ONBOARDING_MARKER_FILE);
        if first_start_marker_gate(&onboarding_marker) {
            state.begin_walkthrough();
        }

        // Independent first-start gate: a user who dismisses or skips the
        // walkthrough above must still see, once, what external tools ALAS
        // can use and whether any of them can be fetched automatically.
        let tool_intro_marker = state
            .tool_locator
            .preferences_path()
            .with_file_name(EXTERNAL_TOOLS_INTRO_MARKER_FILE);
        if first_start_marker_gate(&tool_intro_marker) {
            state.show_tool_intro = true;
        }

        state
    }
}

#[cfg(test)]
mod first_start_marker_tests {
    use super::first_start_marker_gate;

    fn unique_marker_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "alas-gui-first-start-marker-{label}-{}-{:?}",
            std::process::id(),
            std::time::Instant::now()
        ))
    }

    #[test]
    fn reports_true_and_creates_the_marker_exactly_once() {
        let marker = unique_marker_path("once");
        assert!(!marker.exists());

        assert!(
            first_start_marker_gate(&marker),
            "an absent marker must be reported as first-seen"
        );
        assert!(marker.exists(), "the gate must create the marker itself");
        assert!(
            !first_start_marker_gate(&marker),
            "a second gate check against the same marker must not report first-seen again"
        );

        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn two_independent_markers_gate_independently() {
        let onboarding = unique_marker_path("onboarding");
        let tool_intro = unique_marker_path("tool-intro");

        assert!(first_start_marker_gate(&onboarding));
        // Dismissing/creating one marker must never mark the other seen —
        // the walkthrough and the tool-intro screen are independent gates.
        assert!(
            first_start_marker_gate(&tool_intro),
            "a distinct marker path must still report first-seen"
        );

        let _ = std::fs::remove_file(&onboarding);
        let _ = std::fs::remove_file(&tool_intro);
    }
}
