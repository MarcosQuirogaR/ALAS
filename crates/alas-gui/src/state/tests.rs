// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[cfg(test)]
mod first_start_marker_tests {
    use crate::state::app_state::first_start_marker_gate;

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
        // Dismissing/creating one marker must never mark the other seen:
        // the walkthrough and the tool-intro screen are independent gates.
        assert!(
            first_start_marker_gate(&tool_intro),
            "a distinct marker path must still report first-seen"
        );

        let _ = std::fs::remove_file(&onboarding);
        let _ = std::fs::remove_file(&tool_intro);
    }
}

#[cfg(test)]
mod walkthrough_tests {
    use crate::state::tools::apply_tool_preferences;
    use crate::state::AppState;
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
        state.active_page = "inputs".to_owned();
        state.nav_pinned = false;
        state.nav_hover_open = true;
        state.preview_open = false;

        state.begin_walkthrough();
        state.walkthrough_step = 1;
        state.prepare_walkthrough_step();
        assert!(state.nav_pinned);
        assert!(!state.nav_hover_open);

        // Step 4 (index 3) spotlights the Inputs Sandbox Mode button; the
        // 3D Live Preview step that reopens the dock follows it.
        state.walkthrough_step = 4;
        state.prepare_walkthrough_step();
        assert_eq!(state.active_page, "inputs");
        assert!(state.preview_open);

        // The randomizer walkthrough step was removed with the DOE/Random
        // controls and the Sandbox mode step was inserted after it, so
        // Results is displayed step 13 (index 12).
        state.walkthrough_step = 12;
        state.prepare_walkthrough_step();
        assert_eq!(state.active_page, "results");

        state.finish_walkthrough();
        assert_eq!(state.active_page, "inputs");
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

    #[test]
    fn a_fresh_state_has_no_completed_pipeline_result() {
        let state = AppState::default();

        assert!(state.pipeline_result.is_none());
        assert!(!state.pipeline_result_complete);
    }
}

#[cfg(test)]
mod navdata_cancellation_tests {
    use crate::state::{AppState, LogKind};
    use alas_exec::download::{DownloadOutcome, DownloadReport};
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::channel;

    #[test]
    fn cancelling_sets_only_the_navdata_flag_leaving_the_pipeline_flag_untouched() {
        let mut state = AppState {
            navdata_download_in_progress: true,
            ..Default::default()
        };

        state.cancel_navdata_download();

        assert!(state.navdata_download_cancel.load(Ordering::Relaxed));
        assert!(!state.cancel_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn cancelling_with_no_download_in_progress_does_not_arm_the_flag() {
        let mut state = AppState::default();

        state.cancel_navdata_download();

        assert!(!state.navdata_download_cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn a_cancelled_outcome_clears_progress_and_logs_a_cancellation_not_a_failure() {
        let mut state = AppState {
            navdata_download_in_progress: true,
            ..Default::default()
        };
        let (sender, receiver) = channel();
        state.navdata_download_rx = Some(receiver);
        sender
            .send(Ok(DownloadOutcome::Cancelled(DownloadReport {
                target_dir: std::path::PathBuf::from("."),
                downloaded: Vec::new(),
                skipped: Vec::new(),
            })))
            .expect("deliver a cancelled outcome to the polling state");

        state.poll_navdata_download();

        assert!(!state.navdata_download_in_progress);
        let last = state.logs.last().expect("a log line was recorded");
        assert!(last.text.to_lowercase().contains("cancel"), "{}", last.text);
        assert!(!last.text.to_lowercase().contains("fail"), "{}", last.text);
        assert_eq!(
            state.navdata_download_feedback,
            Some((last.text.clone(), LogKind::Warn))
        );
    }
}

#[cfg(test)]
mod log_visibility_tests {
    use crate::state::{AppState, LogKind};

    #[test]
    fn an_error_line_opens_the_run_log_even_when_it_was_closed() {
        let mut state = AppState {
            run_log_open: false,
            ..Default::default()
        };

        state.log("Configuration is not currently valid.", LogKind::Error);

        assert!(state.run_log_open);
    }

    #[test]
    fn info_and_warn_lines_do_not_force_the_run_log_open() {
        let mut state = AppState {
            run_log_open: false,
            ..Default::default()
        };

        state.log("Started pipeline execution.", LogKind::Info);
        assert!(!state.run_log_open);

        state.log("Cancellation requested.", LogKind::Warn);
        assert!(!state.run_log_open);
    }
}

#[cfg(test)]
mod preview_scene_tests {
    use crate::state::AppState;
    use alas_report::scene::Scene;
    use serde_json::json;

    #[test]
    fn an_invalid_custom_geometry_edit_keeps_the_last_valid_scene() {
        let mut state = AppState::default();
        let mut previous = Scene::new(320.0, 200.0, None);
        previous.title = Some("last valid".to_owned());
        state.preview_scene = Some(previous.clone());
        state.preview_scene_revision = 17;
        state.config_values["geometry"]["wing"]["custom_sections"] = json!([{
            "span_fraction": 1.0,
            "leading_edge_x_m": 0.0,
            "chord_m": 1.0,
            "z_m": 0.0,
            "twist_deg": 0.0,
            "airfoil": "naca2410"
        }]);

        state.update_preview_scene();

        assert_eq!(state.preview_scene, Some(previous));
        assert_eq!(state.preview_scene_revision, 17);
    }
}
