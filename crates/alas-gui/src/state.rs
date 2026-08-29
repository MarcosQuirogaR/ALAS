// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Central application state.
//!
//! The edited configuration is held as a `serde_json::Value` -- the same shape
//! the reference desktop app's React `configValues` had -- rather than as a
//! typed [`AlasConfig`]. That is what lets one generic form render every field
//! of every group straight from the schema the derive macro emits, instead of
//! a hand-written control per field that drifts from the model. The typed
//! configuration is recovered with [`AppState::typed_config`] wherever a
//! discipline actually needs it: validation, the live preview, and a run.
//!
//! The struct and its bookkeeping live here; editing a configuration lives in
//! [`crate::config_edit`] and running the pipeline in [`crate::run`], both as
//! further `impl AppState` blocks -- kept in their own files so this one stays
//! under the project's line limit.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::feedback::ParameterFeedback;
use crate::path_picker::PathPicker;
use crate::theme::AppTheme;
pub use crate::viewport::PreviewCamera;
use crate::views::results_view::SolverResultView;
use crate::views::tour_data::{TourTarget, TOUR_STEPS};
use alas_config::{
    airports, engines, presets, validate, AlasConfig, ConfigNode, DesignVector, Node, Severity,
    ValidationIssue, DESIGN_VARIABLE_SPECS,
};
use alas_exec::{ToolLocator, ToolPreferences};
use alas_pipeline::{PipelineOptions, PipelineResult};
use alas_report::scene::Scene;
use alas_viz::SceneViewState;
use serde_json::Value;
#[derive(Debug, Clone)]
/// Shell state temporarily replaced while the walkthrough exposes its targets.
pub struct WalkthroughRestore {
    active_page: String,
    nav_pinned: bool,
    nav_hover_open: bool,
    preview_open: bool,
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
    /// A progress line.
    Progress(String),
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
    /// The most recent finished pipeline result.
    pub pipeline_result: Option<PipelineResult>,
    /// Monotonic identity for the current result-producing run.
    pub run_identity: u64,
    /// The channel a running pipeline reports over.
    pub worker_rx: Option<Receiver<WorkerMessage>>,
    /// The flag that asks a running pipeline to stop.
    pub cancel_flag: Arc<AtomicBool>,
    /// The run log.
    pub logs: Vec<LogLine>,
    /// The run-log dock height in points, retained while the app is open.
    pub run_log_height: f32,
    /// The current validation findings.
    pub validation_findings: Vec<ValidationIssue>,
    /// The preview figure selected in the dock's exterior tab.
    pub selected_preview_id: String,
    /// The rendered preview scene.
    pub preview_scene: Option<Scene>,
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
    /// The airfoil-screening sweep's own run state.
    pub screening: crate::screening::ScreeningState,
    /// Independent fixed-wing UAV inputs, selections, and latest outcome.
    pub uav: crate::uav::UavWorkflowState,
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
        let config_values = serde_json::to_value(&config).unwrap_or(Value::Null);

        let preset_names = presets::display_names()
            .into_iter()
            .map(|(name, display)| (name.to_owned(), display.to_owned()))
            .collect();
        let engine_names = engines::available().iter().map(|s| s.to_string()).collect();
        let airport_names = airports::database()
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

        let mut state = Self {
            config_values,
            schema,
            active_preset: String::new(),
            preset_names,
            engine_names,
            airport_names,
            design_values,
            bounds,
            selected_aux_preset: BTreeMap::new(),
            active_page: "inputs".to_owned(),
            theme: AppTheme::Dark,
            language: Language::En,
            help_verbose: false,
            preview_open: true,
            nav_pinned: false,
            nav_hover_open: false,
            zoom: 1.0,
            zoom_auto: true,
            preview_tab: PreviewTab::Exterior,
            preview_cameras: BTreeMap::new(),
            result_cameras: BTreeMap::new(),
            run_options: RunOptions::default(),
            pipeline_options: PipelineOptions::default(),
            is_running: false,
            status_message: "Ready.".to_owned(),
            parameter_feedback: None,
            path_picker: None,
            stage: String::new(),
            run_started: None,
            pipeline_result: None,
            run_identity: 0,
            worker_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            logs: vec![LogLine {
                text: "ALAS initialized.".to_owned(),
                kind: LogKind::Info,
                elapsed: None,
                run_id: 0,
            }],
            run_log_height: 220.0,
            validation_findings: findings,
            selected_preview_id: "exterior_3d".to_owned(),
            preview_scene: None,
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
            screening: crate::screening::ScreeningState::default(),
            uav: crate::uav::UavWorkflowState::default(),
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
        // right away -- matching the reference's "seen" semantics, which flip
        // on first display rather than on completion.
        let onboarding_marker = state
            .tool_locator
            .preferences_path()
            .with_file_name(ONBOARDING_MARKER_FILE);
        if !onboarding_marker.exists() {
            state.begin_walkthrough();
            if let Some(parent) = onboarding_marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(onboarding_marker, b"1");
        }

        state
    }
}

/// Apply only the user-owned machine locations to a freshly selected design.
/// These locations describe the local environment, not an aircraft, so a
/// preset must never replace a person's solver, navigation-data, or route
/// folders with a checkout-relative default.
fn apply_tool_preferences(config: &mut AlasConfig, preferences: &ToolPreferences) {
    if let Some(path) = &preferences.mses_dir {
        config.mses.mses_dir.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_exe {
        config.structures.nastran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_solver {
        config.structures.nastran_solver_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_dir {
        config.structures.nastran95_dir_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_runtime {
        config.structures.nastran95_runtime_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_rf_stage {
        config.structures.nastran95_rf_stage_path.clone_from(path);
    }
    if let Some(words) = &preferences.nastran95_open_core_words {
        config
            .structures
            .nastran95_open_core_words
            .clone_from(words);
    }
    if let Some(path) = &preferences.patran_exe {
        config.structures.patran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.navdata_dir {
        config.mission.navdata_dir.clone_from(path);
    }
    if let Some(path) = &preferences.routes_dir {
        config.mission.routes_dir.clone_from(path);
    }
}

impl AppState {
    /// Open the walkthrough and remember the shell state it temporarily changes.
    pub fn begin_walkthrough(&mut self) {
        if self.walkthrough_restore.is_none() {
            self.walkthrough_restore = Some(WalkthroughRestore {
                active_page: self.active_page.clone(),
                nav_pinned: self.nav_pinned,
                nav_hover_open: self.nav_hover_open,
                preview_open: self.preview_open,
            });
        }
        self.walkthrough_step = 0;
        self.show_walkthrough = true;
        self.prepare_walkthrough_step();
    }

    /// Make the current step's page or normally-collapsed shell region visible.
    pub(crate) fn prepare_walkthrough_step(&mut self) {
        if !self.show_walkthrough {
            return;
        }
        let Some(step) = TOUR_STEPS.get(self.walkthrough_step) else {
            self.finish_walkthrough();
            return;
        };
        if let Some(page) = step.page {
            self.active_page = page.to_owned();
        }
        match step.target {
            Some(TourTarget::Navigation) => {
                self.nav_pinned = true;
                self.nav_hover_open = false;
            }
            Some(TourTarget::PreviewDock) => self.preview_open = true,
            _ => {}
        }
    }

    /// Close the tour and restore the page and docks it temporarily changed.
    pub(crate) fn finish_walkthrough(&mut self) {
        self.show_walkthrough = false;
        self.walkthrough_targets.clear();
        if let Some(restore) = self.walkthrough_restore.take() {
            self.active_page = restore.active_page;
            self.nav_pinned = restore.nav_pinned;
            self.nav_hover_open = restore.nav_hover_open;
            self.preview_open = restore.preview_open;
        }
    }

    /// Start a fresh collection of response geometry for this frame.
    pub(crate) fn clear_walkthrough_targets(&mut self) {
        self.walkthrough_targets.clear();
    }

    /// Record one shell region from the response egui actually laid out.
    pub(crate) fn record_walkthrough_target(&mut self, target: TourTarget, rect: egui::Rect) {
        if self.show_walkthrough && rect.is_finite() && rect.is_positive() {
            self.walkthrough_targets.insert(target, rect);
        }
    }

    /// Return the measured rectangle for the current walkthrough target.
    pub(crate) fn current_walkthrough_target(&self) -> Option<egui::Rect> {
        let target = TOUR_STEPS.get(self.walkthrough_step)?.target?;
        self.walkthrough_targets.get(&target).copied()
    }

    /// Whether the current tour step owns a particular shell target.
    pub(crate) fn walkthrough_targets(&self, target: TourTarget) -> bool {
        self.show_walkthrough
            && TOUR_STEPS
                .get(self.walkthrough_step)
                .is_some_and(|step| step.target == Some(target))
    }

    /// Return the orbit state belonging to one three-dimensional preview.
    pub fn preview_camera_mut(&mut self, id: impl Into<String>) -> &mut PreviewCamera {
        self.preview_cameras.entry(id.into()).or_default()
    }

    /// Return the orbit state belonging to one three-dimensional result.
    pub fn result_camera_mut(&mut self, id: impl Into<String>) -> &mut PreviewCamera {
        self.result_cameras.entry(id.into()).or_default()
    }

    /// Return the camera shared by both modes of the unified aircraft viewer.
    pub fn active_preview_camera(&self) -> PreviewCamera {
        self.preview_cameras
            .get(AIRCRAFT_PREVIEW_CAMERA_ID)
            .copied()
            .unwrap_or_default()
    }

    /// Return the persistent state belonging to one interactive canvas.
    pub fn view_state_mut(&mut self, key: impl Into<String>) -> &mut SceneViewState {
        self.view_states.entry(key.into()).or_default()
    }

    /// The typed configuration the JSON edit buffer currently represents.
    ///
    /// Returns `None` while the buffer is transiently unreadable -- a numeric
    /// field left mid-edit, say -- which is the same tolerance the reference's
    /// debounced validation showed by wrapping every read in a `try`.
    pub fn typed_config(&self) -> Option<AlasConfig> {
        serde_json::from_value(self.config_values.clone()).ok()
    }

    /// The design point currently shown in the design-space editor.
    pub fn current_design(&self) -> Option<DesignVector> {
        serde_json::to_value(&self.design_values)
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
    }

    /// The edited optimizer bounds in design-vector order.
    pub fn current_design_bounds(&self) -> Option<Vec<(f64, f64)>> {
        DESIGN_VARIABLE_SPECS
            .iter()
            .map(|spec| self.bounds.get(spec.name).copied())
            .collect()
    }

    /// Append a run-log line, trimming the oldest once the cap is reached.
    pub fn log(&mut self, text: impl Into<String>, kind: LogKind) {
        let elapsed = self.run_started.map(|started| started.elapsed());
        self.logs.push(LogLine {
            text: text.into(),
            kind,
            elapsed,
            run_id: if elapsed.is_some() {
                self.run_identity
            } else {
                0
            },
        });
        if self.logs.len() > MAX_LOG_LINES {
            let overflow = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..overflow);
        }
    }

    /// The mutable value of one top-level configuration group.
    pub fn group_mut(&mut self, group: &str) -> Option<&mut Value> {
        self.config_values.get_mut(group)
    }

    /// Whether a blocking (error-severity) validation issue exists, which
    /// disables the Run button.
    pub fn blocked(&self) -> bool {
        self.validation_findings
            .iter()
            .any(|i| i.severity == Severity::Error)
    }

    /// Update the live-preview scene from the current geometry and dock tab.
    pub fn update_preview_scene(&mut self) {
        self.preview_scene = crate::scene::build_preview_scene(self);
    }

    /// Update the results-gallery scene from the current result and selection.
    pub fn update_result_scene(&mut self) {
        self.result_scene = crate::scene::build_result_scene(self);
    }

    /// Build a result figure once per run/theme/id and reuse it between repaints.
    pub fn cached_result_figure(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
    ) -> Option<Arc<Scene>> {
        self.cached_result_figure_with_camera(key, id, config, theme, None)
    }

    /// Build a result figure with an explicit projection camera and cache it.
    pub fn cached_result_figure_with_camera(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
        camera: Option<alas_report::scene::Camera3D>,
    ) -> Option<Arc<Scene>> {
        if let Some(scene) = self.result_figure_cache.get(key) {
            return scene.clone();
        }
        let scene = crate::scene::build_result_figure_with_camera(self, id, config, theme, camera)
            .flatten()
            .map(Arc::new);
        self.result_figure_cache
            .insert(key.to_owned(), scene.clone());
        scene
    }

    /// Replace one cached result scene after its projection camera changes.
    pub fn rebuild_result_figure_with_camera(
        &mut self,
        key: &str,
        id: &str,
        config: &AlasConfig,
        theme: &str,
        camera: alas_report::scene::Camera3D,
    ) -> Option<Arc<Scene>> {
        let scene =
            crate::scene::build_result_figure_with_camera(self, id, config, theme, Some(camera))
                .flatten()
                .map(Arc::new);
        self.result_figure_cache
            .insert(key.to_owned(), scene.clone());
        scene
    }

    /// Elapsed milliseconds since the current run began, or zero when idle.
    pub fn elapsed_ms(&self) -> u128 {
        self.run_started
            .map(|t| t.elapsed().as_millis())
            .unwrap_or(0)
    }
}

pub use crate::nav_overlay::{nav_overlay_open, nav_overlay_open_with_bounds};

#[cfg(test)]
mod walkthrough_tests {
    use super::{apply_tool_preferences, AppState};
    use crate::views::tour_data::TourTarget;
    use alas_config::AlasConfig;
    use alas_exec::ToolPreferences;

    #[test]
    fn user_owned_route_locations_replace_only_their_configuration_defaults() {
        let mut config = AlasConfig::default();
        let preferences = ToolPreferences {
            navdata_dir: Some("D:/aviation/navdata".to_owned()),
            routes_dir: Some("D:/aviation/routes".to_owned()),
            ..ToolPreferences::default()
        };

        apply_tool_preferences(&mut config, &preferences);

        assert_eq!(config.mission.navdata_dir, "D:/aviation/navdata");
        assert_eq!(config.mission.routes_dir, "D:/aviation/routes");
        assert_eq!(config.mission.great_circle_points, 50);
    }

    #[test]
    fn walkthrough_opens_hidden_targets_and_restores_the_shell_afterward() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.active_page = "mission".to_owned();
        state.nav_pinned = false;
        state.nav_hover_open = true;
        state.preview_open = false;

        state.begin_walkthrough();
        state.walkthrough_step = 1;
        state.prepare_walkthrough_step();
        assert!(state.nav_pinned);
        assert!(!state.nav_hover_open);

        state.walkthrough_step = 3;
        state.prepare_walkthrough_step();
        assert_eq!(state.active_page, "inputs");
        assert!(state.preview_open);

        state.walkthrough_step = 12;
        state.prepare_walkthrough_step();
        assert_eq!(state.active_page, "results");

        state.finish_walkthrough();
        assert_eq!(state.active_page, "mission");
        assert!(!state.nav_pinned);
        assert!(state.nav_hover_open);
        assert!(!state.preview_open);
    }

    #[test]
    fn current_spotlight_uses_the_recorded_response_rectangle() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.begin_walkthrough();
        state.walkthrough_step = 1;
        let measured = egui::Rect::from_min_max(egui::pos2(17.0, 31.0), egui::pos2(241.0, 700.0));
        state.record_walkthrough_target(TourTarget::Navigation, measured);

        assert_eq!(state.current_walkthrough_target(), Some(measured));
    }
}
