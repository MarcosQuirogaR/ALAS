// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::sync::mpsc::{channel, TryRecvError};
use std::thread;

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
    /// Start the shared navdata downloader without blocking an egui frame.
    pub fn start_navdata_download(&mut self, configured: &str) {
        if self.navdata_download_in_progress {
            return;
        }
        let configured = if configured.trim().is_empty() {
            alas_route::assets::NAVDATA_REL.to_owned()
        } else {
            configured.trim().to_owned()
        };
        let target = self
            .tool_locator
            .resolve_data_path(std::path::Path::new(&configured));
        let specs = alas_route::assets::NAVDATA_FILES
            .iter()
            .map(|file| {
                alas_exec::download::DownloadSpec::new(
                    file.name,
                    alas_route::assets::navdata_file_url(file),
                    file.min_bytes,
                )
            })
            .collect::<Vec<_>>();
        let (sender, receiver) = channel();
        self.navdata_download_rx = Some(receiver);
        self.navdata_download_in_progress = true;
        self.log(
            format!("Downloading navigation data into {}...", target.display()),
            LogKind::Info,
        );
        thread::spawn(move || {
            let result = alas_exec::download::download_files(&specs, &target, 120.0);
            let _ = sender.send(result);
        });
    }

    /// Drain a completed navdata transfer without blocking the UI.
    pub fn poll_navdata_download(&mut self) {
        let Some(receiver) = &self.navdata_download_rx else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("navigation-data downloader stopped unexpectedly".to_owned())
            }
        };
        self.navdata_download_rx = None;
        self.navdata_download_in_progress = false;
        match result {
            Ok(summary) => self.log(
                format!(
                    "Navigation data ready at {} (downloaded {}, skipped {}).",
                    summary.target_dir.display(),
                    summary.downloaded.len(),
                    summary.skipped.len()
                ),
                LogKind::Info,
            ),
            Err(error) => self.log(
                format!("Navigation-data download failed: {error}"),
                LogKind::Error,
            ),
        }
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
    /// Returns `None` while the buffer is transiently unreadable (a numeric
    /// field left mid-edit, say) which is the same tolerance the reference's
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
        self.preview_scene_revision = self.preview_scene_revision.wrapping_add(1);
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

        // The randomizer walkthrough step was removed with the DOE/Random
        // controls, so Results is now the following step.
        state.walkthrough_step = 11;
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
