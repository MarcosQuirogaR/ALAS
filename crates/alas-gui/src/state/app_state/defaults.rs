// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default (first-start) application state.

use super::*;

impl Default for AppState {
    fn default() -> Self {
        let tool_locator = ToolLocator::for_current_process();
        let tool_preferences = tool_locator.load_preferences();
        let data_dir = tool_locator.user_data_root().to_path_buf();
        let mut config = AlasConfig::default();
        crate::config_edit::apply_interactive_solver(&mut config);
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
            typed_config_memo: std::cell::RefCell::new(None),
            fingerprint_memo: Default::default(),
            design_space_enforcement_memo: None,
            clean_sheet_brief_fingerprint: None,
            page_preview_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            schema,
            active_preset: String::new(),
            loaded_baseline: None,
            preset_names,
            engine_names,
            airport_names,
            custom_airport_draft: crate::airport_editor::CustomAirportDraft::default(),
            custom_airport_window: crate::views::airport_window::CustomAirportWindow::default(),
            custom_airport_file_path: data_dir.join("alas-airports.json").display().to_string(),
            custom_airport_status: None,
            custom_airfoil_file_path: "custom-airfoil.dat".to_owned(),
            custom_airfoil_status: None,
            design_values,
            bounds,
            selected_aux_preset: BTreeMap::new(),
            active_page: "inputs".to_owned(),
            theme: AppTheme::Dark,
            language: Language::En,
            preview_open: true,
            preview_fit: None,
            preview_pan: (0.0, 0.0),
            preview_auto_pitch: None,
            shell: crate::shell_ui::ShellUi::default(),
            // Collapsed at launch unless the user pinned it last time.
            nav_pinned: data_dir.join(NAV_PINNED_MARKER_FILE).exists(),
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
            pipeline_result_design_values: None,
            pipeline_result_form_config: None,
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
            navdata_download_feedback: None,
            navdata_download_cancel: Arc::new(AtomicBool::new(false)),
            openvsp_runtime_setup: crate::openvsp_runtime_setup::OpenVspRuntimeSetup::default(),
            tool_installer: crate::tool_installer::ToolInstaller::default(),
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
            run_log_height: 150.0,
            run_log_open: false,
            synced_cruise_mach: None,
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
            result_figure_revision_counter: 0,
            figure_jobs: Default::default(),
            preview_jobs: Default::default(),
            config_path: data_dir.join("alas-config.json").display().to_string(),
            files: crate::user_files::UserFilesState::default(),
            tool_locator,
            tool_preferences,
            show_about: false,
            show_storage: false,
            show_view_panel: false,
            show_walkthrough: false,
            walkthrough_step: 0,
            walkthrough_targets: BTreeMap::new(),
            walkthrough_restore: None,
            show_tool_intro: false,
            tool_intro_selected_config: None,
            tool_intro_download_all_consent: false,
            tool_intro_navdata_consent: false,
            tool_intro_openvsp_preview_consent: false,
            screening: crate::screening::ScreeningState::default(),
            cfd,
            uav: crate::uav::UavWorkflowState::default(),
            addons: crate::addons::AddonsPanel::default(),
            sandbox: boxed_sandbox_session(),
            boot_frames_remaining: 40,
        };

        // Load the first registered preset at startup rather than starting
        // from bare `AlasConfig::default()` values.
        if let Some((first, _)) = state.preset_names.first().cloned() {
            state.load_preset(&first);
        } else {
            state.update_preview_scene();
        }

        // Show the tour on a launch that has never seen it, and mark it seen
        // right away: "seen" flips on first display rather than on completion.
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

/// The sandbox session lives on the heap: it is the largest member of
/// [`AppState`] (about 85 KiB), and the application is built on the main
/// thread, whose stack is 1 MiB unless the linker raises it. A separate
/// frame also releases the temporary before the rest of the state is built.
#[inline(never)]
fn boxed_sandbox_session() -> Box<crate::sandbox::SandboxSession> {
    Box::default()
}
